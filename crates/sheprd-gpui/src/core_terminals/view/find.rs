//! Find in a herdr-core pane. The search is the one a Herdr 1 pane's find
//! bar runs ([`crate::find::Search`]), with the same keys, highlights, and
//! count; this view's emulator answers it through the worker instead of the
//! daemon, and the bar is drawn over the view itself, so it moves and hides
//! with its pane.

use super::CoreTerminalView;
use crate::{
    find::{Search, Step, bar},
    scrollback::reveal_offset,
    search_input::{self, SearchInput},
    terminal_painter::Highlight,
};
use gpui::{prelude::*, *};
use sheprd_client::scrollback::ScrollbackResponse;
use std::time::Instant;

pub(super) struct Find {
    input: Entity<SearchInput>,
    search: Search,
    _changed: Subscription,
}

impl CoreTerminalView {
    /// Opens the bar, or returns to it with its query selected. A new bar
    /// starts from the query last searched here.
    pub(crate) fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(find) = &self.find {
            let input = find.input.clone();
            input.update(cx, |input, cx| {
                let text = input.text().to_owned();
                input.set_text_selected(&text, cx);
            });
            window.focus(&input.read(cx).focus.clone(), cx);
            cx.notify();
            return;
        }
        let Some(pane) = self.surface_pane() else {
            return;
        };
        let input = cx.new(SearchInput::new);
        let chrome = self.look.chrome.clone();
        input.update(cx, |input, cx| {
            input.set_placeholder_with(|| crate::i18n::fl!("window-find-placeholder"), cx);
            input.set_appearance(chrome.ui.clone(), chrome.theme.clone(), cx);
        });
        window.focus(&input.read(cx).focus.clone(), cx);
        let changed = cx.subscribe(&input, |this, input, _: &search_input::Changed, cx| {
            let query = input.read(cx).text().to_owned();
            if let Some(find) = &mut this.find {
                find.search.set_query(&query);
            }
            this.flush_find();
            cx.notify();
        });
        self.find = Some(Find {
            input: input.clone(),
            search: Search::new(&pane),
            _changed: changed,
        });
        // After the subscription, so the query is searched like a typed one.
        let query = self.find_query.clone();
        input.update(cx, |input, cx| input.set_text_selected(&query, cx));
        cx.notify();
    }

    /// The next or previous match, opening the bar when nothing is being
    /// searched yet. The next match is the next older one, as Enter.
    pub(crate) fn find_again(&mut self, back: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.find.is_none() {
            self.open_find(window, cx);
            return;
        }
        let step = if back { Step::Newer } else { Step::Older };
        self.find_step(step, cx);
    }

    /// Closes the bar, keeping its query for the next one, and gives the
    /// terminal the keyboard back when the bar held it.
    pub(crate) fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(find) = self.find.take() else {
            return;
        };
        let input = find.input.read(cx);
        input.text().clone_into(&mut self.find_query);
        if input.focus.is_focused(window) {
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }

    /// Whether the bar's field holds the keyboard, so the terminal must not
    /// act on a key bubbling out of it.
    pub(super) fn find_focused(&self, window: &Window, cx: &App) -> bool {
        self.find
            .as_ref()
            .is_some_and(|find| find.input.read(cx).focus.is_focused(window))
    }

    fn find_step(&mut self, step: Step, cx: &mut Context<Self>) {
        let Some(find) = &mut self.find else {
            return;
        };
        find.search.step(step);
        self.flush_find();
        cx.notify();
    }

    /// Sends the search that is due, if any.
    pub(super) fn flush_find(&mut self) {
        let Some(pane) = self.surface_pane() else {
            return;
        };
        let Some((step, params)) = self
            .find
            .as_ref()
            .and_then(|find| find.search.next_request(&pane))
        else {
            return;
        };
        let request = sheprd_backend::History::Search(sheprd_backend::Search {
            query: params.query.clone(),
            direction: params.direction,
            cursor: params.cursor,
            previous: params.previous,
        });
        let id = self.ask(request);
        if let Some(find) = &mut self.find {
            find.search
                .sent(id.to_string(), &params, step, Instant::now());
        }
    }

    /// Applies a search's answer, scrolling its match into view when the
    /// step that asked moves the view.
    pub(super) fn find_answered(&mut self, id: u64, answer: super::history::Answer) {
        let Some(pane) = self.surface_pane() else {
            return;
        };
        let Some(find) = &mut self.find else {
            return;
        };
        let answer = match super::history::scrollback(&pane, answer) {
            Ok(ScrollbackResponse::PaneCopySearch(result)) => Ok(result),
            Ok(_) => Err(sheprd_client::Error::ResponseType),
            Err(error) => Err(error),
        };
        let reveal = find.search.answer(&id.to_string(), answer);
        if let Some(offset) = reveal.and_then(|range| reveal_offset(&pane, range)) {
            self.scroll_history(usize::try_from(offset).unwrap_or(usize::MAX));
        }
    }

    /// Searches again, now and then, while the pane's output changes.
    pub(super) fn poll_find(&mut self) {
        let Some(pane) = self.surface_pane() else {
            return;
        };
        if let Some(find) = &mut self.find {
            find.search.content_changed(&pane, Instant::now());
        }
        self.flush_find();
    }

    /// Whether `id` is the search the bar waits on.
    pub(super) fn awaits_find(&self, id: u64) -> bool {
        self.find
            .as_ref()
            .and_then(|find| find.search.in_flight())
            .is_some_and(|request| request == id.to_string())
    }

    pub(crate) fn find_highlights(&self) -> Vec<Highlight> {
        match (&self.find, self.surface_pane()) {
            (Some(find), Some(pane)) => find.search.highlights(&pane),
            _ => Vec::new(),
        }
    }

    /// Keeps the bar's field in the window's look.
    pub(super) fn restyle_find(&self, cx: &mut Context<Self>) {
        if let Some(find) = &self.find {
            let chrome = self.look.chrome.clone();
            find.input.update(cx, |input, cx| {
                input.set_appearance(chrome.ui.clone(), chrome.theme.clone(), cx);
            });
        }
    }

    fn find_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(find) = &self.find else {
            return;
        };
        let Some(action) = bar::key_action(event, find.input.read(cx)) else {
            return;
        };
        self.find_action(action, window, cx);
        cx.stop_propagation();
        window.prevent_default();
    }

    fn find_action(&mut self, action: bar::Action, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            bar::Action::Close => self.close_find(window, cx),
            bar::Action::Step(step) => self.find_step(step, cx),
        }
    }

    /// The bar, over the top right of the view.
    pub(super) fn render_find(&self, cx: &mut Context<Self>) -> Option<Div> {
        let find = self.find.as_ref()?;
        let label = find
            .search
            .error()
            .map_or_else(|| find.search.label(), str::to_owned);
        let act = std::rc::Rc::new(cx.listener(|this, action: &bar::Action, window, cx| {
            this.find_action(*action, window, cx);
        }));
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .flex()
                .justify_end()
                .p(px(6.))
                .child(bar::render(
                    bar::Bar {
                        input: &find.input,
                        label,
                        failed: find.search.error().is_some(),
                        has_matches: find.search.has_matches(),
                        theme: &self.look.chrome.theme,
                        text_size: self.look.chrome.ui.size,
                    },
                    cx.listener(Self::find_key_down),
                    move |action, window, cx| act(&action, window, cx),
                )),
        )
    }
}
