//! A pure terminal emulator: raw PTY bytes in, Herdr's cell model out.
#![doc = include_str!("../README.md")]

mod cells;
mod damage;
mod emulator;
mod error;
mod graphics;
pub mod input;
mod listener;
mod modes;
mod scan;

pub use damage::Damage;
pub use emulator::{Emulator, Event, Scroll};
pub use error::{Error, Result};
pub use modes::{Modes, MouseEncoding, MouseReporting};
pub use sheprd_backend::{MAX_CELLS, MAX_SIDE, ScrollMetrics, Size};

// The `CellData::modifier` bits, as the app's painter reads them.
// keep-sorted start
pub const BOLD: u16 = 1;
pub const DIM: u16 = 1 << 1;
pub const HIDDEN: u16 = 1 << 7;
pub const ITALIC: u16 = 1 << 2;
pub const REVERSED: u16 = 1 << 6;
pub const STRIKETHROUGH: u16 = 1 << 8;
pub const UNDERLINE: u16 = 1 << 3;
// keep-sorted end

/// The most history lines a terminal keeps, whatever was asked for.
pub const MAX_SCROLLBACK_LINES: usize = 100_000;
/// The most history cells a terminal keeps: wide terminals keep fewer lines.
pub const MAX_SCROLLBACK_CELLS: usize = 10_000_000;
/// The longest OSC string (title, hyperlink) the emulator reads; longer ones
/// are dropped whole rather than buffered without bound.
pub const MAX_OSC_BYTES: usize = 8 * 1024;
/// The longest kitty graphics command the emulator reads, one chunk of an
/// image or a whole one sent unchunked; longer ones are dropped whole.
pub const MAX_GRAPHICS_COMMAND_BYTES: usize = 8 * 1024 * 1024;
/// The most text one OSC 52 clipboard write may carry, decoded. Its OSC
/// string may hold the base64 for this much, past [`MAX_OSC_BYTES`].
pub const MAX_CLIPBOARD_BYTES: usize = 2 * 1024 * 1024;

#[cfg(test)]
mod tests;
