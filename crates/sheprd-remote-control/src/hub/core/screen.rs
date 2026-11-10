//! Emulate raw output once per visited pane, shared by paired phones.
//! A screen keeps the terminal's recent history, so a phone scrolls back
//! through it, and holds the terminal at a phone's size while one asks.

use super::lines::Lines;
use crate::hub::{ApiMethod, Error, Result};
use serde_json::{Value, json};
use sheprd_client::protocol::{CellData, ClientKeyCode, ClientKeyKind, ClientPaneInputEvent};
use sheprd_core_client::{
    Attachment, Core, End,
    wire::{Attach, Terminal},
};
use sheprd_terminal::{Emulator, Scroll, Size};
use std::{
    fmt::Write as _,
    time::{Duration, Instant},
};

/// The history lines a screen keeps: the most a phone reads back at once.
pub(super) const HISTORY: u32 = 1000;
/// The longest a new screen waits for the core's first paint.
const PAINT: Duration = Duration::from_millis(500);

pub(super) struct Screen {
    address: String,
    attachment: Attachment,
    /// A viewer starts as an observer: a driving attach sets the terminal
    /// to its size at once, and an observer's input is silently dropped.
    driving: bool,
    emulator: Emulator,
    /// The phones holding the terminal at their size, the latest last. The
    /// core sizes a terminal to its latest driver and back to the one
    /// before when that one leaves, so the latest hold wins.
    holds: Vec<(u64, Size)>,
    /// Whether the core listed the terminal at the latest hold's size
    /// since it was set. After that, a listing at another size is a
    /// driver that took the terminal back, which the emulator follows.
    hold_listed: bool,
    /// Where a scrolled view's lines went as output arrived and as the
    /// screen rewrapped.
    lines: Lines,
    lost: bool,
    /// Whether a driver took the terminal back from the holds: the latest
    /// hold then no longer sets its size, and closing a view leaves it
    /// alone. A phone that asks for its size again takes it back.
    taken_back: bool,
    sync_since: Option<Instant>,
    touched: Instant,
}

impl Screen {
    /// Watches `terminal` at the size its driver set, leaving that alone.
    pub(super) fn open(core: &Core, terminal: &Terminal) -> Result<Self> {
        // Only this first attach paints history: a later one would add it
        // to the emulator's history a second time.
        let attachment = core.attach(&Attach {
            cols: terminal.cols,
            history: HISTORY,
            observe: true,
            rows: terminal.rows,
            terminal: &terminal.address,
            ..Attach::default()
        })?;
        let attached = &attachment.terminal;
        let size = Size::new(attached.cols, attached.rows).map_err(|_| Error::Response)?;
        let mut screen = Self {
            address: terminal.address.clone(),
            attachment,
            driving: false,
            // The phone shows no images, but the cursor must move past them.
            emulator: Emulator::new(size, HISTORY as usize).without_images(),
            holds: Vec::new(),
            hold_listed: false,
            lines: Lines::default(),
            lost: false,
            sync_since: None,
            taken_back: false,
            touched: Instant::now(),
        };
        screen.settle()?;
        Ok(screen)
    }

    /// Reads the first paint, which follows the attach's answer: an attach
    /// that replaced this one before it was read would lose its history.
    fn settle(&mut self) -> Result<()> {
        let started = Instant::now();
        let mut wait = PAINT;
        while !self.lost && started.elapsed() < PAINT {
            let chunk = self.attachment.read(wait)?;
            self.feed(&chunk.bytes);
            if chunk.end.is_some() {
                self.lost = true;
            }
            if chunk.bytes.is_empty() {
                break;
            }
            // The rest of a paint comes right after its start.
            wait = Duration::from_millis(20);
        }
        Ok(())
    }

    /// Takes the driver's role to type, at the size the core reports now,
    /// so it changes nothing on the desktop. Keeps the emulator: its modes
    /// decide how input is encoded, and the new attach paints over it.
    fn drive(&mut self, core: &Core) -> Result<()> {
        if self.driving {
            return Ok(());
        }
        let current = core
            .list(false)?
            .terminals
            .into_iter()
            .find(|terminal| terminal.address == self.address)
            .ok_or(super::Error::Disconnected)?;
        self.attachment = core.attach(&Attach {
            cols: current.cols,
            rows: current.rows,
            terminal: &self.address,
            ..Attach::default()
        })?;
        self.driving = true;
        self.follow_size(&current)
    }

    /// Holds the terminal at `size` for the phone `hold` names until it is
    /// released, taking the driver's role at that size.
    pub(super) fn hold(&mut self, core: &Core, hold: u64, size: Size) -> Result<()> {
        self.touched = Instant::now();
        // What the attach it replaces sent belongs before the new paint.
        self.drain()?;
        // Recorded only once it took: nothing could release a failed one.
        if self.driving {
            self.attachment.resize(size.cols(), size.rows())?;
        } else {
            self.attachment = core.attach(&Attach {
                cols: size.cols(),
                rows: size.rows(),
                terminal: &self.address,
                ..Attach::default()
            })?;
            self.driving = true;
        }
        self.holds.retain(|(id, _)| *id != hold);
        self.holds.push((hold, size));
        self.hold_listed = false;
        self.taken_back = false;
        self.resize(size);
        Ok(())
    }

    /// Lets `hold` go. The latest hold left takes over; with none, the
    /// screen goes back to observing, which lets the core size the terminal
    /// back to the driver before. Typing takes the role again when needed.
    pub(super) fn release(&mut self, core: &Core, hold: u64) -> Result<()> {
        let before = self.holds.len();
        self.holds.retain(|(id, _)| *id != hold);
        if self.holds.len() == before {
            return Ok(());
        }
        if let Some((_, size)) = self.holds.last().copied() {
            // Resizing would make the screen the latest driver again, over
            // the size the desktop took back.
            if self.taken_back {
                return Ok(());
            }
            self.hold_listed = false;
            self.attachment.resize(size.cols(), size.rows())?;
            self.resize(size);
            return Ok(());
        }
        if !self.driving {
            return Ok(());
        }
        let size = self.emulator.size();
        let observing = self.drain().and_then(|()| {
            Ok(core.attach(&Attach {
                cols: size.cols(),
                observe: true,
                rows: size.rows(),
                terminal: &self.address,
                ..Attach::default()
            })?)
        });
        let observing = match observing {
            Ok(observing) => observing,
            Err(error) => {
                // The driving attach must not outlive the last hold: the
                // screen goes, taking it along, and the next read opens a
                // watching one.
                self.lost = true;
                return Err(error);
            }
        };
        self.attachment = observing;
        self.driving = false;
        let attached = self.attachment.terminal.clone();
        self.follow_size(&attached)
    }

    pub(super) fn expired(&self) -> bool {
        self.lost || (self.holds.is_empty() && self.touched.elapsed() > Duration::from_secs(30))
    }

    /// How long nobody has read or typed into it. A held screen is never
    /// idle: a phone is showing it at its size.
    pub(super) fn idle(&self) -> Duration {
        if self.holds.is_empty() {
            self.touched.elapsed()
        } else {
            Duration::ZERO
        }
    }

    #[cfg(all(test, unix))]
    pub(super) fn age(&mut self, by: Duration) {
        self.touched -= by;
    }

    #[cfg(all(test, unix))]
    pub(super) fn size(&self) -> Size {
        self.emulator.size()
    }

    /// Sizes the emulator as the core last listed the terminal. A hold's
    /// size is newer than listings until one shows it; a listing at another
    /// size after that is a driver that took the terminal back, as the
    /// desktop does when its pane resizes, and the phone follows it rather
    /// than fight over the size.
    pub(super) fn follow_size(&mut self, terminal: &Terminal) -> Result<()> {
        let listed = Size::new(terminal.cols, terminal.rows).map_err(|_| Error::Response)?;
        let size = match self.holds.last() {
            Some((_, held)) if !self.hold_listed => {
                self.hold_listed = listed == *held;
                *held
            }
            Some((_, held)) => {
                self.taken_back |= listed != *held;
                listed
            }
            None => listed,
        };
        self.resize(size);
        Ok(())
    }

    /// Resizes the emulator, keeping where a scrolled view's lines went as
    /// the screen rewraps into history or out of it.
    fn resize(&mut self, size: Size) {
        self.lines
            .resize(&mut self.emulator, size, HISTORY as usize);
    }

    pub(super) fn call(&mut self, core: &Core, method: ApiMethod, params: &Value) -> Result<Value> {
        self.touched = Instant::now();
        self.drain()?;
        match method {
            ApiMethod::PaneRead => Ok(read(&mut self.emulator, params, &self.lines)),
            ApiMethod::PaneSendText => {
                let text = params["text"].as_str().ok_or(Error::Response)?;
                self.drive(core)?;
                self.send(ClientPaneInputEvent::Paste(text.to_owned()))?;
                Ok(json!({}))
            }
            ApiMethod::PaneSendKeys => {
                let keys = params["keys"].as_array().ok_or(Error::Response)?;
                self.drive(core)?;
                for key in keys {
                    self.send(key_event(key.as_str().ok_or(Error::Response)?)?)?;
                }
                Ok(json!({}))
            }
            _ => Err(Error::Response),
        }
    }

    fn send(&self, event: ClientPaneInputEvent) -> Result<()> {
        if self.lost {
            return Err(super::Error::Disconnected.into());
        }
        self.attachment.write(&self.emulator.encode(&event))?;
        Ok(())
    }

    fn feed(&mut self, bytes: &[u8]) {
        self.lines.pushed(feed(&mut self.emulator, bytes));
    }

    fn drain(&mut self) -> Result<()> {
        if self.lost {
            return Err(super::Error::Disconnected.into());
        }
        // A continuously writing terminal must not hold the shared source lock.
        let mut bytes = 0;
        loop {
            let chunk = self.attachment.read(Duration::ZERO)?;
            self.feed(&chunk.bytes);
            bytes += chunk.bytes.len();
            if matches!(chunk.end, Some(End::Lost | End::Closed(_))) {
                self.lost = true;
            }
            if chunk.bytes.is_empty() || chunk.end.is_some() || bytes >= 1024 * 1024 {
                break;
            }
        }
        if self.emulator.sync_pending() {
            let since = self.sync_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= Duration::from_millis(150) {
                self.emulator.end_sync();
                self.sync_since = None;
            }
        } else {
            self.sync_since = None;
        }
        self.emulator.take_events();
        Ok(())
    }
}

/// Feeds `bytes`, and counts the lines they pushed into history. Scrolled
/// up one line, the emulator keeps showing the same line as output pushes
/// lines into history, so its offset moves by exactly that many; a burst
/// past the whole history counts one short of it, which still takes any
/// scrolled view to the top. The emulator is left live.
pub(super) fn feed(emulator: &mut Emulator, bytes: &[u8]) -> u64 {
    // Without history (a new screen, or a full-screen program's) no view
    // is scrolled, so nothing needs counting: and leaving a full-screen
    // program brings back history that is not new.
    if emulator.scroll_metrics().history == 0 {
        emulator.feed(bytes);
        return 0;
    }
    emulator.scroll(Scroll::Bottom);
    emulator.scroll(Scroll::Lines(1));
    emulator.feed(bytes);
    let pushed = emulator.scroll_metrics().offset.saturating_sub(1);
    emulator.scroll(Scroll::Bottom);
    pushed as u64
}

/// The screen, scrolled up `scroll` lines into history, or with
/// `source: recent` the last `lines` of history above it. `history` says
/// how far a view can scroll. A view that read at `since` (the `lines`
/// token each read answers) stays on the lines it showed as output pushes
/// them up and resizes rewrap them, and the answer's `scroll` is where it
/// is now.
pub(super) fn read(emulator: &mut Emulator, params: &Value, lines: &Lines) -> Value {
    let cols = emulator.size().cols();
    let history = emulator.scroll_metrics().history;
    let count = |key: &str| {
        params[key]
            .as_u64()
            .map_or(0, |lines| usize::try_from(lines).unwrap_or(usize::MAX))
    };
    if params["source"] == "recent" {
        let cells = recent(emulator, count("lines").min(history));
        return json!({"read": {"text": ansi(&cells, cols), "history": history}});
    }
    let mut up = count("scroll");
    if up > 0
        && let Some(since) = params["since"].as_u64()
    {
        up = lines.place(up, since);
    }
    let up = up.min(history);
    let cells = scrolled(emulator, up);
    json!({"read": {
        "text": ansi(&cells, cols),
        "history": history,
        "lines": lines.token(),
        "scroll": up,
    }})
}

/// The viewport `up` lines into history; the emulator is left live.
fn scrolled(emulator: &mut Emulator, up: usize) -> Vec<CellData> {
    if up == 0 {
        return emulator.frame().cells;
    }
    emulator.scroll(Scroll::Bottom);
    emulator.scroll(Scroll::Lines(i32::try_from(up).unwrap_or(i32::MAX)));
    let cells = emulator.frame().cells;
    emulator.scroll(Scroll::Bottom);
    cells
}

/// `lines` of history, oldest first, then the live screen, gathered a
/// viewport at a time.
fn recent(emulator: &mut Emulator, lines: usize) -> Vec<CellData> {
    let cols = usize::from(emulator.size().cols());
    let rows = usize::from(emulator.size().rows());
    let mut cells = Vec::with_capacity((lines + rows) * cols);
    let mut up = lines;
    loop {
        let page = scrolled(emulator, up);
        // Above the live screen, a page shows `rows` lines from `up`.
        let take = if up == 0 { rows } else { up.min(rows) };
        cells.extend(page.into_iter().take(take * cols));
        if up == 0 {
            return cells;
        }
        up -= take;
    }
}

fn key_event(key: &str) -> Result<ClientPaneInputEvent> {
    let mut parts: Vec<&str> = key.split('+').collect();
    let name = parts.pop().ok_or(Error::Response)?;
    let mut modifiers = 0;
    for part in parts {
        modifiers |= match part {
            "ctrl" => sheprd_terminal::input::CONTROL,
            "alt" => sheprd_terminal::input::ALT,
            "shift" => sheprd_terminal::input::SHIFT,
            _ => return Err(Error::Response),
        };
    }
    let code = match name {
        "backspace" => ClientKeyCode::Backspace,
        "down" => ClientKeyCode::Down,
        "enter" => ClientKeyCode::Enter,
        "esc" => ClientKeyCode::Esc,
        "left" => ClientKeyCode::Left,
        "right" => ClientKeyCode::Right,
        "tab" => ClientKeyCode::Tab,
        "up" => ClientKeyCode::Up,
        "backtick" => ClientKeyCode::Char('`'),
        "minus" => ClientKeyCode::Char('-'),
        "plus" => ClientKeyCode::Char('+'),
        "slash" => ClientKeyCode::Char('/'),
        "space" => ClientKeyCode::Char(' '),
        name if name.starts_with('f') && name.len() > 1 => {
            let number = name[1..].parse::<u8>().map_err(|_| Error::Response)?;
            if !(1..=12).contains(&number) {
                return Err(Error::Response);
            }
            ClientKeyCode::F(number)
        }
        name if name.chars().count() == 1 => {
            ClientKeyCode::Char(name.chars().next().ok_or(Error::Response)?)
        }
        _ => return Err(Error::Response),
    };
    Ok(ClientPaneInputEvent::Key {
        code,
        generated_text: None,
        kind: ClientKeyKind::Press,
        modifiers,
        physical_key_id: None,
        repeat_count: 1,
        shifted_codepoint: None,
        tracks_release: false,
        windows_record: None,
    })
}

fn ansi(cells: &[CellData], cols: u16) -> String {
    let mut text = String::new();
    let mut style = None;
    for row in cells.chunks(usize::from(cols)) {
        if !text.is_empty() {
            text.push_str("\x1b[0m\n");
            style = None;
        }
        for cell in row.iter().filter(|cell| !cell.skip) {
            let next = (cell.fg, cell.bg, cell.modifier);
            if style != Some(next) {
                text.push_str("\x1b[0");
                for (bit, code) in [(1, 1), (2, 2), (4, 3), (8, 4), (64, 7), (128, 8), (256, 9)] {
                    if cell.modifier & bit != 0 {
                        let _ = write!(text, ";{code}");
                    }
                }
                color(&mut text, cell.fg, false);
                color(&mut text, cell.bg, true);
                text.push('m');
                style = Some(next);
            }
            text.extend(cell.symbol.chars().filter(|c| !c.is_control()));
        }
    }
    text.push_str("\x1b[0m");
    text
}

fn color(text: &mut String, value: u32, background: bool) {
    let prefix = if background { 48 } else { 38 };
    match value >> 24 {
        1 => {
            let _ = write!(text, ";{prefix};5;{}", value & 255);
        }
        2 => {
            let _ = write!(
                text,
                ";{prefix};2;{};{};{}",
                (value >> 16) & 255,
                (value >> 8) & 255,
                value & 255
            );
        }
        _ if (1..=16).contains(&value) => {
            let index = value - 1;
            let base = if background { 40 } else { 30 };
            let _ = write!(
                text,
                ";{}",
                base + index % 8 + if index >= 8 { 60 } else { 0 }
            );
        }
        _ => {}
    }
}
