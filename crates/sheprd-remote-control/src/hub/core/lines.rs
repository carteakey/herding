//! Where a scrolled phone view's lines went. A view reads at a token and
//! comes back with it; output since then pushed its lines up into history,
//! and a resize rewrapped them, so the screen counts both.

use sheprd_terminal::{Emulator, Size};
use std::collections::VecDeque;

/// The rewraps kept: a view older than them all moves by one line too
/// many per rewrap it missed, as each counts in the token.
const KEPT: usize = 16;

#[derive(Debug, Default)]
pub(super) struct Lines {
    /// Lines pushed into history since the screen opened, plus one per
    /// rewrap, so a view that read just before a rewrap and one that read
    /// just after it hold different tokens.
    token: u64,
    rewraps: VecDeque<Rewrap>,
}

/// One resize's rewrap: each line's rows before and after it, oldest first.
#[derive(Debug)]
struct Rewrap {
    /// The token just before it.
    at: u64,
    after: Vec<u16>,
    before: Vec<u16>,
    /// Lines dropped from the top as history overflowed.
    dropped: usize,
    history: (usize, usize),
}

impl Lines {
    pub(super) fn token(&self) -> u64 {
        self.token
    }

    pub(super) fn pushed(&mut self, lines: u64) {
        self.token = self.token.saturating_add(lines);
    }

    /// Resizes `emulator`, keeping how its lines rewrapped.
    pub(super) fn resize(&mut self, emulator: &mut Emulator, size: Size, kept: usize) {
        if size == emulator.size() {
            return;
        }
        let history = emulator.scroll_metrics().history;
        let before = emulator.line_rows();
        emulator.resize(size);
        let after = emulator.line_rows();
        let history = (history, emulator.scroll_metrics().history);
        if history.0 == 0 && history.1 == 0 {
            // No view scrolls without history.
            return;
        }
        // Lines only go from the top once history is full; elsewhere the
        // count differs by blank rows at the bottom.
        let dropped = if history.1 >= kept {
            before.len().saturating_sub(after.len())
        } else {
            0
        };
        if self.rewraps.len() == KEPT {
            self.rewraps.pop_front();
        }
        self.rewraps.push_back(Rewrap {
            at: self.token,
            after,
            before,
            dropped,
            history,
        });
        self.token = self.token.saturating_add(1);
    }

    /// Where a view `up` rows into history when it read at `since` is now.
    pub(super) fn place(&self, mut up: usize, since: u64) -> usize {
        let mut cursor = since;
        for rewrap in self.rewraps.iter().filter(|rewrap| rewrap.at >= since) {
            up = up.saturating_add(gap(rewrap.at, cursor));
            up = rewrap.place(up);
            cursor = rewrap.at.saturating_add(1);
        }
        up.saturating_add(gap(self.token, cursor))
    }
}

fn gap(to: u64, from: u64) -> usize {
    usize::try_from(to.saturating_sub(from)).unwrap_or(usize::MAX)
}

impl Rewrap {
    /// Follows the row at the top of a view `up` rows into history to the
    /// row it rewrapped onto.
    fn place(&self, up: usize) -> usize {
        // Rows counted from the oldest, where a rewrap below them moves
        // nothing above.
        let Some(mut row) = self.history.0.checked_sub(up) else {
            return up;
        };
        let mut line = 0usize;
        for rows in &self.before {
            let rows = usize::from(*rows);
            if row < rows {
                break;
            }
            row -= rows;
            line += 1;
        }
        let Some(line) = line.checked_sub(self.dropped) else {
            return self.history.1;
        };
        let start: usize = self
            .after
            .iter()
            .take(line)
            .map(|rows| usize::from(*rows))
            .sum();
        let rows = self.after.get(line).map_or(1, |rows| usize::from(*rows));
        let row = start + row.min(rows.saturating_sub(1));
        self.history.1.saturating_sub(row)
    }
}

#[cfg(test)]
impl From<u64> for Lines {
    fn from(token: u64) -> Self {
        Self {
            token,
            rewraps: VecDeque::new(),
        }
    }
}
