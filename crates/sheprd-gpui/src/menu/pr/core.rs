//! Pull request lookups for a window with no Herdr session to scope them:
//! only the checkout of the herdr-core pane it shows, which is on this
//! computer and read with its main account, as the Git chip reads it.

use crate::{
    SheprdWindow,
    pull_request::{Input, Origin},
};

/// Stands in for a daemon boot in the lookup scope, which herdr-core does
/// not report: a change of selection or connection still renews it.
const BOOT: &str = "herdr-core";

impl SheprdWindow {
    /// Looks up `input`'s pull request when no Herdr lookups run; whether
    /// any answer changed.
    pub(super) fn update_core_pr(&mut self, input: Input) -> bool {
        let Some(tokens) = self.main_tokens() else {
            self.menu.pr_cache.suspend();
            return false;
        };
        let scope = (
            self.selection_epoch,
            self.endpoints[self.selected_endpoint].generation,
            BOOT.to_owned(),
        );
        let key = tokens.key();
        self.menu.pr_cache.scope(scope, tokens, &key, Origin::Local);
        self.menu.pr_cache.idle = !self.active;
        let now = std::time::Instant::now();
        if self.menu.pr_cache.scan_due(now) {
            self.menu.pr_cache.expire_viewed(&input, now);
            self.menu.pr_cache.schedule([input], now);
        }
        self.menu.pr_cache.poll(now)
    }
}
