//! A window's side of requests from agents in Herdr 2 terminals on another
//! host (see `core_terminals::requests`): serve each new one once, in the
//! workspace of the tab that shows its terminal, then remove its labels.
//! A request carries no pane, so its page reports to no agent and a reload
//! is never asked for.

use super::{PART, PREFIX, Request, Seen, SeenKey};
use crate::{SheprdWindow, browser::Store};
use gpui::{Context, Window};

impl SheprdWindow {
    pub(super) fn poll_core_browser_requests(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for found in self.core_terminals.label_requests() {
            let Some(index) = self
                .endpoints
                .iter()
                .position(|endpoint| crate::browser::scope(endpoint) == found.scope)
            else {
                continue;
            };
            let Some(workspace) = cx
                .try_global::<Store>()
                .and_then(|store| store.get(found.tab))
                .map(|tab| tab.workspace_id.clone())
            else {
                continue;
            };
            let mut served = Vec::new();
            for request in Request::all(&found.tokens) {
                let name = format!("{PREFIX}{}", request.id);
                let part = format!("{name}{PART}");
                served.extend(
                    found
                        .tokens
                        .iter()
                        .map(|(token, _)| token)
                        .filter(|token| **token == name || token.starts_with(&part))
                        .cloned(),
                );
                let key = SeenKey {
                    scope: found.scope.clone(),
                    workspace: workspace.clone(),
                    id: request.id.clone(),
                    window: None,
                };
                // A request names no pane here, so it never asks for a
                // reload, which needs the pane that opened the page.
                let request = Request {
                    pane: None,
                    ..request
                };
                if cx.default_global::<Seen>().first(key) {
                    self.serve_remote_request(index, workspace.clone(), request, window, cx);
                }
            }
            if !served.is_empty() {
                self.core_terminals
                    .clear_labels(&found.scope, &found.terminal, served);
            }
        }
    }
}
