//! A phone's split, zoom, or close of a Herdr 2 tab. The remote control
//! daemon cannot change the layout, which this app keeps with the tab, so
//! it asks (`sheprd_remote_control::control::layout`), and the window
//! holding the tab makes the change as its own commands do. A split is
//! answered once its shell is in the tab, or failed to be.

use super::{CoreTerminals, Host, Purpose, TabId, panes::Axis};
use crate::{SheprdWindow, remote::supervisor::Supervisor};
use gpui::{App, Context, Window};
use sheprd_backend::{Spawn, TerminalId};
use sheprd_remote_control::control::{
    changes::AskId,
    layout::{Answer, Ask, Failure, Op},
};
use std::time::{Duration, Instant};

/// How long a phone's split may take before it counts as failed: past the
/// daemon's own wait, so nothing is held for a phone gone.
const SPLIT_WAIT: Duration = Duration::from_secs(15);

/// A phone's split on its way.
#[derive(Debug)]
pub(super) struct Pending {
    id: AskId,
    /// The terminals the tab had before, which the split adds to.
    before: Vec<TerminalId>,
    since: Instant,
    tab: TabId,
}

/// What a phone's change comes to in one window.
pub(super) enum Change {
    /// Made, or refused, already.
    Answered(Answer),
    /// The tab to close, and the terminals still in it: a split that
    /// landed after the phone's daemon ended the others.
    Close(TabId, Vec<TerminalId>),
    /// The tab whose zoom moved to the phone's pane.
    Zoomed(TabId),
    /// A new name for the tab or one of its panes; `None` clears it.
    Renamed(super::menu::Target, Option<String>),
    /// A new shell to start beside the pane.
    Split(Purpose),
}

impl CoreTerminals {
    /// What `ask` comes to when this window holds its tab; `None` when it
    /// does not, so another window may. A zoom is made here.
    pub(super) fn phone_change(&mut self, ask: &Ask) -> Option<Change> {
        // The phone's daemon reads this computer's core alone: a tab on
        // another host is never the one it names.
        let tab = *self
            .tabs
            .iter()
            .find(|(tab, shown)| tab.number() == ask.tab && shown.host == Host::Local)?
            .0;
        // The tab goes whole: its panes may have left already, their
        // terminals ended first. Any still in it end with it.
        if ask.op == Op::Close {
            let left = self
                .tabs
                .get(&tab)
                .map(|shown| shown.panes.terminals().into_iter().cloned().collect())
                .unwrap_or_default();
            return Some(Change::Close(tab, left));
        }
        let busy = self.starting.is_some() || !self.wanted.is_empty() || self.phone_split.is_some();
        let shown = self.tabs.get_mut(&tab)?;
        let Some(terminal) = TerminalId::new(ask.terminal.as_str())
            .ok()
            .filter(|terminal| shown.panes.contains(terminal))
        else {
            return Some(Change::Answered(Answer::Failed(Failure::Gone)));
        };
        let axis = match &ask.op {
            Op::SplitRight => Axis::Row,
            Op::SplitDown => Axis::Column,
            Op::Close => return Some(Change::Close(tab, Vec::new())),
            // Named as the tab's menu names it: one safe, bounded line.
            Op::RenameTab(name) => {
                let name = name.as_deref().and_then(super::menu::clean_name);
                return Some(Change::Renamed(super::menu::Target::Tab(tab), name));
            }
            Op::RenamePane(name) => {
                let name = name.as_deref().and_then(super::menu::clean_name);
                let target = super::menu::Target::Pane { tab, terminal };
                return Some(Change::Renamed(target, name));
            }
            // Not a tab's: the window answers it before asking here.
            Op::RenameWorkspace { .. } => {
                return Some(Change::Answered(Answer::Failed(Failure::Refused)));
            }
            Op::Zoom => {
                // One pane already fills its tab, as Herdr leaves it.
                if shown.panes.count() < 2 {
                    return Some(Change::Answered(Answer::Done));
                }
                let zoom = (!shown.zoom.is_open()).then(|| terminal.clone());
                shown.set_zoom(zoom);
                // The pane zoomed is the one in use, as a zoom here makes it.
                shown.active = terminal;
                return Some(Change::Zoomed(tab));
            }
        };
        // A mirror only watches: it never splits.
        if shown.observe {
            return Some(Change::Answered(Answer::Failed(Failure::Watching)));
        }
        // One start at a time: a second would take the first one's place.
        if busy {
            return Some(Change::Answered(Answer::Failed(Failure::Busy)));
        }
        self.phone_split = Some(Pending {
            id: ask.id,
            before: shown.panes.terminals().into_iter().cloned().collect(),
            since: Instant::now(),
            tab,
        });
        Some(Change::Split(Purpose::Split {
            tab,
            at: terminal,
            axis,
        }))
    }

    /// The answer to the phone's split once it is settled: the tab has a
    /// new pane, which the phone shows next, or nothing is starting any
    /// more, or it took too long.
    pub(super) fn settled_phone_split(&mut self, now: Instant) -> Option<(AskId, Answer)> {
        let pending = self.phone_split.as_ref()?;
        let placed = self.tabs.get(&pending.tab).and_then(|shown| {
            shown
                .panes
                .terminals()
                .into_iter()
                .find(|terminal| !pending.before.contains(terminal))
                .cloned()
        });
        let idle = self.starting.is_none() && self.wanted.is_empty();
        let late = now.saturating_duration_since(pending.since) >= SPLIT_WAIT;
        if placed.is_none() && !idle && !late {
            return None;
        }
        let pending = self.phone_split.take()?;
        let answer = match placed {
            Some(created) => Answer::Split(created.as_str().to_owned()),
            None => Answer::Failed(Failure::Refused),
        };
        Some((pending.id, answer))
    }
}

impl SheprdWindow {
    /// Makes `ask` when this window holds its tab: `None` when it does not,
    /// `Some(None)` when its answer comes later, from [`Self::answer_phone_split`].
    pub(crate) fn phone_layout(
        &mut self,
        ask: &Ask,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Option<Answer>> {
        if let Op::RenameWorkspace { workspace, name } = &ask.op {
            return self
                .phone_rename_workspace(workspace, name, cx)
                .then_some(Some(Answer::Done));
        }
        let answer = match self.core_terminals.phone_change(ask)? {
            Change::Answered(answer) => Some(answer),
            Change::Renamed(target, name) => {
                self.set_core_name(&target, name, cx);
                Some(Answer::Done)
            }
            Change::Close(tab, left) => {
                for terminal in left {
                    self.end_core_terminal(&Host::Local, terminal);
                }
                self.close_browser_tab(tab, window, cx);
                Some(Answer::Done)
            }
            Change::Zoomed(tab) => {
                // The keyboard stays in the tab it was in, on the pane shown.
                if self
                    .core_terminals
                    .focused_pane(window, cx)
                    .is_some_and(|(focused, _)| focused == tab)
                {
                    self.focus_core_terminal(tab, window, cx);
                }
                Some(Answer::Done)
            }
            Change::Split(purpose) => {
                self.start_core_program(purpose, Spawn::default(), cx);
                None
            }
        };
        cx.notify();
        Some(answer)
    }

    /// Renames a workspace this window keeps for Herdr 2 on this computer;
    /// false when it keeps none by that id, so another window may.
    fn phone_rename_workspace(
        &mut self,
        workspace: &str,
        name: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(index) = self.core_host() else {
            return false;
        };
        let scope = crate::browser::scope(&self.endpoints[index]);
        let kept = cx
            .try_global::<crate::browser::Store>()
            .is_some_and(|store| store.kept_workspace(&scope, workspace).is_some());
        // This computer's, whichever host the window shows.
        if kept {
            crate::browser::Store::update(cx, |store| store.rename_kept(&scope, workspace, name));
            self.refresh_core_session(cx);
            cx.notify();
        }
        kept
    }

    /// Tells the daemon how a phone's split went, once it has.
    pub(super) fn answer_phone_split(&mut self, cx: &mut App) {
        if let Some((id, answer)) = self.core_terminals.settled_phone_split(Instant::now()) {
            Supervisor::send_layout_answer(cx, id, answer);
        }
    }
}
