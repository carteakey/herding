//! What a view asks of the history it keeps: searches, copy-mode motions,
//! text between two points, the whole text, scrolling to a line, and a
//! clear. Rows are counted from the top of the retained history, as Herdr 1's
//! daemon counts them, so the app's find and copy mode read both alike.

use super::Emulator;
use crate::{Error, Result};
use alacritty_terminal::{
    grid::{Dimensions, Scroll},
    index::{Column, Direction, Line, Point},
    term::{
        TermMode,
        cell::Flags,
        search::{Match, RegexIter, RegexSearch},
    },
    vi_mode::{ViModeCursor, ViMotion},
};
use sheprd_backend::{
    CopyMotion, History, HistoryAnswer, MAX_SEARCH_MATCHES, MAX_SEARCH_QUERY_BYTES, Matches,
    Search, SearchDirection, TextPoint, TextRange,
};

/// What ends a word for copy-mode word motions: blanks and every ASCII
/// punctuation mark but `_`, so `w` stops at `.` and `/` as Vim's does.
pub(super) const WORD_SEPARATORS: &str = " \t!\"#$%&'()*+,-./:;<=>?@[\\]^`{|}~│";

impl Emulator {
    /// Answers `request`. Only a search can fail: its query may be too long
    /// or too large to search for.
    pub fn history(&mut self, request: &History) -> Result<HistoryAnswer> {
        Ok(match request {
            History::Search(search) => HistoryAnswer::Matches(self.search(search)?),
            History::Motion { cursor, motion } => {
                HistoryAnswer::Point(self.motion(*cursor, *motion))
            }
            History::Read { anchor, cursor } => HistoryAnswer::Text(self.read(*anchor, *cursor)),
            History::Text => HistoryAnswer::Text(self.text()),
            History::ScrollTo { offset } => {
                self.scroll_to(*offset);
                HistoryAnswer::Done
            }
            History::Clear => {
                self.clear();
                HistoryAnswer::Done
            }
        })
    }

    /// Every match of the query, as text, counted; at most
    /// [`MAX_SEARCH_MATCHES`] around the current one are returned.
    pub fn search(&self, search: &Search) -> Result<Matches> {
        if search.query.len() > MAX_SEARCH_QUERY_BYTES {
            return Err(Error::QueryTooLarge);
        }
        if search.query.is_empty() {
            return Ok(Matches {
                revision: self.revision,
                ..Matches::default()
            });
        }
        let mut regex = RegexSearch::new(&escape(&search.query)).map_err(Error::Search)?;
        let origin = match (search.direction, search.previous) {
            (SearchDirection::Forward, Some(previous)) => previous.end,
            (SearchDirection::Backward, Some(previous)) => previous.start,
            (_, None) => search.cursor,
        };
        let mut total: u64 = 0;
        let mut current = None;
        for found in self.matches(&mut regex) {
            let range = self.range(&found);
            match search.direction {
                SearchDirection::Forward if current.is_none() && range.start > origin => {
                    current = Some(total);
                }
                SearchDirection::Backward if range.end < origin => current = Some(total),
                _ => {}
            }
            total += 1;
        }
        let Some(last) = total.checked_sub(1) else {
            return Ok(Matches {
                revision: self.revision,
                ..Matches::default()
            });
        };
        // Nothing past the origin: the search wraps around.
        let current = current.unwrap_or(match search.direction {
            SearchDirection::Forward => 0,
            SearchDirection::Backward => last,
        });
        let window = MAX_SEARCH_MATCHES as u64;
        let first = current
            .saturating_sub(window / 2)
            .min(total.saturating_sub(window));
        let matches = self
            .matches(&mut regex)
            .skip(usize::try_from(first).unwrap_or(usize::MAX))
            .take(MAX_SEARCH_MATCHES)
            .map(|found| self.range(&found))
            .collect();
        Ok(Matches {
            matches,
            total,
            current: u32::try_from(current - first).ok(),
            current_global: Some(current),
            revision: self.revision,
        })
    }

    /// Where `motion` from `cursor` lands. The view stays where it was.
    pub fn motion(&mut self, cursor: TextPoint, motion: CopyMotion) -> TextPoint {
        let motion = match motion {
            CopyMotion::LineEnd => ViMotion::Last,
            CopyMotion::FirstNonBlank => ViMotion::FirstOccupied,
            CopyMotion::NextWordStart => ViMotion::SemanticRight,
            CopyMotion::PreviousWordStart => ViMotion::SemanticLeft,
            CopyMotion::NextWordEnd => ViMotion::SemanticRightEnd,
            CopyMotion::NextBigWordStart => ViMotion::WordRight,
            CopyMotion::PreviousBigWordStart => ViMotion::WordLeft,
            CopyMotion::NextBigWordEnd => ViMotion::WordRightEnd,
            CopyMotion::PreviousParagraph => ViMotion::ParagraphUp,
            CopyMotion::NextParagraph => ViMotion::ParagraphDown,
        };
        let offset = self.term.grid().display_offset();
        let point = self.point(cursor);
        let landed = ViModeCursor::new(point)
            .motion(&mut self.term, motion)
            .point;
        // A vi motion scrolls the view to its point; copy mode scrolls itself.
        self.scroll_to(offset);
        self.text_point(landed)
    }

    /// The text from `anchor` to `cursor`, both inclusive, in either order:
    /// wrapped rows joined, others ended with a newline, trailing blanks
    /// dropped.
    pub fn read(&self, anchor: TextPoint, cursor: TextPoint) -> String {
        let (start, end) = if anchor <= cursor {
            (anchor, cursor)
        } else {
            (cursor, anchor)
        };
        self.term
            .bounds_to_string(self.point(start), self.point(end))
    }

    /// The whole history and screen, as an editor should open it.
    pub fn text(&self) -> String {
        let start = Point::new(self.term.topmost_line(), Column(0));
        let end = Point::new(self.term.bottommost_line(), self.term.last_column());
        let text = self.term.bounds_to_string(start, end);
        let mut text = text.trim_end().to_owned();
        text.push('\n');
        text
    }

    /// Scrolls the view to `offset` lines above the live screen, held to
    /// the history there is.
    pub fn scroll_to(&mut self, offset: usize) {
        let now = self.term.grid().display_offset();
        let delta = i32::try_from(offset)
            .unwrap_or(i32::MAX)
            .saturating_sub(i32::try_from(now).unwrap_or(i32::MAX));
        if delta != 0 {
            self.scroll(super::Scroll::Lines(delta));
        }
    }

    /// Drops the history and the rows above the cursor's line, keeping that
    /// line and the input wrapped onto it at the top, as a terminal's clear
    /// does. Input longer than the screen keeps the history rows it starts
    /// in. An alternate screen keeps no history and is left alone. The
    /// program is not told; its next output draws on the cleared screen.
    pub fn clear(&mut self) {
        if self.term.mode().contains(TermMode::ALT_SCREEN) {
            return;
        }
        let rows = self.term.screen_lines();
        let last = self.term.last_column();
        let limit = super::config(self.size, self.scrollback_lines).scrolling_history;
        let grid = self.term.grid_mut();
        let topmost = Line(-i32::try_from(grid.history_size()).unwrap_or(i32::MAX));
        let mut top = grid.cursor.point.line;
        while top > topmost && grid[top - 1i32][last].flags.contains(Flags::WRAPLINE) {
            top -= 1i32;
        }
        if top.0 < 0 {
            // Every row above the cursor is the input's: keep the history
            // rows it starts in, dropping only the older ones, then let the
            // history grow back to its limit.
            grid.update_history(usize::try_from(-top.0).unwrap_or(0));
            grid.update_history(limit);
        } else {
            let lines = usize::try_from(top.0).unwrap_or(0);
            if lines > 0 {
                let screen = Line(0)..Line(i32::try_from(rows).unwrap_or(i32::MAX));
                grid.scroll_up(&screen, lines);
                grid.cursor.point.line -= top.0;
            }
            grid.clear_history();
        }
        grid.scroll_display(Scroll::Bottom);
        self.damage.full = true;
        self.revision = self.revision.wrapping_add(1);
    }

    /// Every match from the top of the history down.
    fn matches<'a>(
        &'a self,
        regex: &'a mut RegexSearch,
    ) -> RegexIter<'a, crate::listener::Listener> {
        let start = Point::new(self.term.topmost_line(), Column(0));
        let end = Point::new(self.term.bottommost_line(), self.term.last_column());
        RegexIter::new(start, end, Direction::Right, &self.term, regex)
    }

    fn range(&self, found: &Match) -> TextRange {
        TextRange {
            start: self.text_point(*found.start()),
            end: self.text_point(*found.end()),
        }
    }

    /// The grid point of `point`, held inside the history and screen.
    fn point(&self, point: TextPoint) -> Point {
        let history = self.term.grid().history_size();
        let total = history + self.term.screen_lines();
        let row = usize::try_from(point.row)
            .unwrap_or(usize::MAX)
            .min(total.saturating_sub(1));
        let line = i64::try_from(row).unwrap_or(i64::MAX) - i64::try_from(history).unwrap_or(0);
        let column = usize::from(point.col).min(self.term.last_column().0);
        Point::new(
            Line(i32::try_from(line).unwrap_or(i32::MAX)),
            Column(column),
        )
    }

    fn text_point(&self, point: Point) -> TextPoint {
        let history = i64::try_from(self.term.grid().history_size()).unwrap_or(i64::MAX);
        let row = (i64::from(point.line.0) + history).max(0);
        TextPoint {
            row: u32::try_from(row).unwrap_or(u32::MAX),
            col: u16::try_from(point.column.0).unwrap_or(u16::MAX),
        }
    }
}

/// `query` as a regular expression matching it literally.
fn escape(query: &str) -> String {
    let mut escaped = String::with_capacity(query.len());
    for c in query.chars() {
        if "\\.+*?()|[]{}^$#&-~".contains(c) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}
