//! The Azure DevOps page from the keyboard, as the GitLab page works: Tab and
//! Shift-Tab walk the page's controls and fields in the order they show,
//! Enter or Space uses the selected control, and Escape closes the menu. In a
//! field Enter checks and saves the token typed, and every other key types.

use super::AzureMenu;
use crate::{SheprdWindow, menu::Page};
use gpui::{prelude::*, *};

/// Which way Tab moves the selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Forward,
    Back,
}

/// What a key does on the Azure DevOps page, apart from typing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AzureKey {
    Dismiss,
    /// Enter: saves from a field, else uses the selected control.
    Enter,
    /// Space: uses the selected control; in a field it types a space.
    Space,
    Move(Step),
}

impl AzureKey {
    fn of(keystroke: &Keystroke) -> Option<Self> {
        let modifiers = keystroke.modifiers;
        let bare = modifiers == Modifiers::default();
        let plain = !modifiers.control && !modifiers.platform && !modifiers.alt;
        Some(match keystroke.key.as_str() {
            "escape" => Self::Dismiss,
            "enter" if bare => Self::Enter,
            "space" if bare => Self::Space,
            "tab" if plain && modifiers.shift => Self::Move(Step::Back),
            "tab" if plain => Self::Move(Step::Forward),
            _ => return None,
        })
    }
}

/// A stop on the Azure DevOps page, as Tab walks them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    /// The account menu's GitHub tab.
    GitHub,
    /// The account menu's GitLab tab.
    GitLab,
    /// Forget the token saved for an organization.
    Forget(String),
    /// The organization field, which takes focus rather than being used.
    Organization,
    /// The token field, which takes focus rather than being used.
    Token,
    /// The link to the page that creates a token.
    CreateToken,
    Save,
    Close,
}

impl AzureMenu {
    /// The page's stops in the order they show, which Tab follows.
    pub(crate) fn actions(&self) -> Vec<Action> {
        let mut actions = vec![Action::GitHub, Action::GitLab];
        actions.extend(
            self.organizations
                .iter()
                .filter(|status| status.saved)
                .map(|status| Action::Forget(status.organization.clone())),
        );
        actions.extend([
            Action::Organization,
            Action::Token,
            Action::CreateToken,
            Action::Save,
            Action::Close,
        ]);
        actions
    }

    /// The child of the page's scrolling body that holds `action`, in the
    /// order `render_azure_page` adds them. The tabs, Save, and Close sit
    /// outside the body and always show.
    pub(crate) fn body_child(&self, action: &Action) -> Option<usize> {
        let cards = 1 + usize::from(self.problem.is_some());
        let form = cards + self.organizations.len();
        match action {
            Action::Forget(organization) => self
                .organizations
                .iter()
                .position(|status| status.organization == *organization)
                .map(|index| cards + index),
            Action::Organization => Some(form),
            Action::Token => Some(form + 1),
            Action::CreateToken => Some(form + 2),
            Action::GitHub | Action::GitLab | Action::Save | Action::Close => None,
        }
    }

    /// Whether the keyboard selected `action`.
    pub(crate) fn is_selected(&self, action: &Action) -> bool {
        self.selected.as_ref() == Some(action)
    }
}

impl SheprdWindow {
    /// Uses `action`, as clicking its control does; Forget waits for a save
    /// or forget under way, and a field has nothing to use.
    fn azure_action(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            Action::GitHub => self.menu.page = Some(Page::GitHub),
            Action::GitLab => self.open_gitlab_page(window, cx),
            Action::Forget(organization) if !self.menu.azure.working => {
                self.forget_azure(organization, cx);
            }
            Action::Forget(_) | Action::Organization | Action::Token => {}
            Action::CreateToken => self.open_azure_token_page(cx),
            Action::Save => self.save_azure(cx),
            Action::Close => self.dismiss_menu(window, cx),
        }
    }

    /// Selects `action`, moving focus into its field or out of the fields,
    /// and scrolls it into view so nothing is used unseen.
    fn select_azure(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        let azure = &self.menu.azure;
        let input = match action {
            Action::Organization => azure.organization.as_ref(),
            Action::Token => azure.token.as_ref(),
            _ => None,
        };
        let focus = input.map_or_else(
            || self.menu.focus.clone(),
            |input| input.read(cx).focus.clone(),
        );
        window.focus(&focus, cx);
        if let Some(child) = azure.body_child(&action) {
            azure.scroll.scroll_to_item(child);
        }
        self.menu.azure.selected = Some(action);
    }

    /// Keys while the Azure DevOps page shows.
    pub(in crate::menu) fn azure_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = AzureKey::of(&event.keystroke) else {
            return;
        };
        let azure = &self.menu.azure;
        let fields = [
            (&azure.organization, Action::Organization),
            (&azure.token, Action::Token),
        ];
        let composing = fields
            .iter()
            .filter_map(|(input, _)| input.as_ref())
            .any(|input| input.read(cx).is_composing());
        // A click can move focus, so the field that has it outranks the
        // selection.
        let field = fields.into_iter().find_map(|(input, action)| {
            let input = input.as_ref()?;
            input.read(cx).focus.is_focused(window).then_some(action)
        });
        if composing || (field.is_some() && key == AzureKey::Space) {
            return;
        }
        cx.stop_propagation();
        window.prevent_default();
        match key {
            AzureKey::Dismiss => self.dismiss_menu(window, cx),
            AzureKey::Move(step) => {
                let actions = azure.actions();
                let at = field
                    .as_ref()
                    .or(azure.selected.as_ref())
                    .and_then(|current| actions.iter().position(|action| action == current));
                let next = match (at, step) {
                    (None, Step::Forward) => 0,
                    (None, Step::Back) => actions.len() - 1,
                    (Some(at), Step::Forward) => (at + 1) % actions.len(),
                    (Some(at), Step::Back) => (at + actions.len() - 1) % actions.len(),
                };
                if let Some(action) = actions.get(next).cloned() {
                    self.select_azure(action, window, cx);
                }
            }
            AzureKey::Enter if field.is_some() => self.save_azure(cx),
            AzureKey::Enter | AzureKey::Space => {
                // A control that went away since it was selected is not used.
                let selected = azure
                    .selected
                    .clone()
                    .filter(|selected| azure.actions().contains(selected));
                match selected {
                    Some(action) => self.azure_action(action, window, cx),
                    None if key == AzureKey::Enter => self.save_azure(cx),
                    None => {}
                }
            }
        }
        cx.notify();
    }
}
