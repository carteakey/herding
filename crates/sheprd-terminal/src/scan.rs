//! A small pass over the byte stream before vte sees it. It does three
//! things vte cannot: it tracks Win32 input mode (DECSET/DECRST `?9001`),
//! which alacritty ignores, it bounds OSC strings, which vte buffers without
//! a limit when built with `std`, and it takes kitty graphics commands
//! (`ESC _ G … ESC \`), which vte ignores, out of the stream in order. An
//! OSC 52 clipboard write may be longer than the other OSC strings, up to
//! the base64 of [`MAX_CLIPBOARD_BYTES`]. Its state carries across feeds, so
//! a sequence split anywhere gives the same result as one fed whole.

use crate::{MAX_CLIPBOARD_BYTES, MAX_GRAPHICS_COMMAND_BYTES, MAX_OSC_BYTES};

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
const CAN: u8 = 0x18;
const SUB: u8 = 0x1a;
const WIN32_INPUT_MODE: u32 = 9001;
/// Parameters past this are as invalid to vte (u16) as to the mode check.
const PARAM_CAP: u32 = 100_000;
/// How an OSC 52 clipboard write begins, as the scanner holds it.
const OSC_52: &[u8] = b"]52;";
/// The longest OSC 52 string: its selector and the base64 for
/// [`MAX_CLIPBOARD_BYTES`], with room for a long selector.
const MAX_OSC_52_BYTES: usize = MAX_CLIPBOARD_BYTES.div_ceil(3) * 4 + 64;

/// What the scanner passes on, in stream order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Run<'a> {
    /// Bytes for vte.
    Bytes(&'a [u8]),
    /// One kitty graphics command: its control data and payload, between
    /// `ESC _ G` and `ESC \`.
    Graphics(&'a [u8]),
}

#[derive(Debug, Default)]
pub(crate) struct Scanner {
    state: State,
    win32_input: bool,
    /// The held OSC string, from its `]` on, or the held graphics command
    /// after its `G`; vte has seen only the ESC that began it.
    osc: Vec<u8>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum State {
    #[default]
    Ground,
    Escape,
    Csi(Csi),
    /// Holding an OSC string until its terminator.
    Osc,
    /// Holding an OSC string whose last byte was ESC.
    OscEscape,
    /// Dropping an OSC string longer than [`MAX_OSC_BYTES`].
    Overflow,
    /// Dropping an oversized OSC string whose last byte was ESC.
    OverflowEscape,
    /// After `ESC _`: a graphics command if `G` follows.
    ApcStart,
    /// Holding a graphics command until its `ESC \`.
    Graphics,
    /// Holding a graphics command whose last byte was ESC.
    GraphicsEscape,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Csi {
    started: bool,
    private: bool,
    invalid: bool,
    param: u32,
    matched: bool,
}

impl Csi {
    fn end_param(&mut self) {
        self.matched |= self.param == WIN32_INPUT_MODE;
        self.param = 0;
    }
}

impl Scanner {
    pub(crate) fn win32_input(&self) -> bool {
        self.win32_input
    }

    /// Passes `bytes` on to `forward` in order, minus held or dropped OSC
    /// strings, which it passes on whole once they end, and graphics
    /// commands, which it passes on as such.
    pub(crate) fn feed(&mut self, bytes: &[u8], mut forward: impl FnMut(Run<'_>)) {
        let mut run = 0;
        let mut index = 0;
        while index < bytes.len() {
            let byte = bytes[index];
            match self.state {
                State::Ground => {
                    if byte == ESC {
                        self.state = State::Escape;
                    }
                }
                State::Escape => self.escape(byte, bytes, &mut run, index, &mut forward),
                State::Csi(mut csi) => self.state = csi_byte(&mut csi, byte, &mut self.win32_input),
                State::Osc => match byte {
                    BEL | CAN | SUB => {
                        self.osc.push(byte);
                        forward(Run::Bytes(&self.osc));
                        self.end_osc(&mut run, index + 1);
                    }
                    ESC => self.state = State::OscEscape,
                    _ if self.osc.len() >= self.osc_limit() => {
                        // vte holds the ESC that began this string; `\`
                        // makes it a harmless ST.
                        forward(Run::Bytes(b"\\"));
                        self.osc = Vec::new();
                        self.state = State::Overflow;
                    }
                    _ => self.osc.push(byte),
                },
                State::OscEscape => {
                    self.osc.push(ESC);
                    if byte == b'\\' {
                        self.osc.push(byte);
                        forward(Run::Bytes(&self.osc));
                        self.end_osc(&mut run, index + 1);
                    } else {
                        // ESC ends the string and begins another sequence
                        // with this byte, which is read again as such.
                        forward(Run::Bytes(&self.osc));
                        self.end_osc(&mut run, index);
                        self.state = State::Escape;
                        continue;
                    }
                }
                State::Overflow => match byte {
                    ESC => self.state = State::OverflowEscape,
                    BEL | CAN | SUB => self.end_osc(&mut run, index + 1),
                    _ => {}
                },
                State::OverflowEscape => {
                    if byte == b'\\' {
                        self.end_osc(&mut run, index + 1);
                    } else {
                        forward(Run::Bytes(&[ESC]));
                        self.end_osc(&mut run, index);
                        self.state = State::Escape;
                        continue;
                    }
                }
                State::ApcStart => {
                    if byte == b'G' {
                        self.osc.clear();
                        self.state = State::Graphics;
                        run = index + 1;
                    } else {
                        // Another APC string, which vte ignores itself.
                        forward(Run::Bytes(b"_"));
                        self.end_osc(&mut run, index);
                        continue;
                    }
                }
                State::Graphics => match byte {
                    ESC => self.state = State::GraphicsEscape,
                    CAN | SUB => {
                        // Cancelled: vte drops the string too.
                        forward(Run::Bytes(b"_"));
                        self.end_osc(&mut run, index);
                        continue;
                    }
                    _ if self.osc.len() >= MAX_GRAPHICS_COMMAND_BYTES => {
                        // Too long to hold: vte reads the rest as an APC
                        // string it ignores, up to its end.
                        forward(Run::Bytes(b"_"));
                        self.osc = Vec::new();
                        self.end_osc(&mut run, index);
                        continue;
                    }
                    _ => self.osc.push(byte),
                },
                State::GraphicsEscape => {
                    if byte == b'\\' {
                        // vte holds the ESC that began the command: an empty
                        // APC string closes it before the command runs, so
                        // what the command writes is read from the ground.
                        forward(Run::Bytes(b"_\x1b\\"));
                        forward(Run::Graphics(&self.osc));
                        self.end_osc(&mut run, index + 1);
                    } else {
                        // ESC ends the command unfinished, and begins
                        // another sequence with this byte: vte reads it as
                        // an APC string the ESC ends, then that sequence.
                        forward(Run::Bytes(b"_\x1b"));
                        self.end_osc(&mut run, index);
                        self.state = State::Escape;
                        continue;
                    }
                }
            }
            index += 1;
        }
        if !self.holding() && run < bytes.len() {
            forward(Run::Bytes(&bytes[run..]));
        }
    }

    fn escape(
        &mut self,
        byte: u8,
        bytes: &[u8],
        run: &mut usize,
        index: usize,
        forward: &mut impl FnMut(Run<'_>),
    ) {
        self.state = match byte {
            b'[' => State::Csi(Csi::default()),
            b']' | b'_' => {
                if *run < index {
                    forward(Run::Bytes(&bytes[*run..index]));
                }
                *run = index + 1;
                self.osc.clear();
                if byte == b'_' {
                    State::ApcStart
                } else {
                    self.osc.push(byte);
                    State::Osc
                }
            }
            b'c' => {
                // RIS resets every mode.
                self.win32_input = false;
                State::Ground
            }
            ESC => State::Escape,
            // vte runs C0 controls inside an escape without leaving it.
            0x00..=0x17 | 0x19 | 0x1c..=0x1f | 0x7f => State::Escape,
            _ => State::Ground,
        };
    }

    fn end_osc(&mut self, run: &mut usize, next: usize) {
        if self.osc.capacity() > MAX_OSC_BYTES {
            self.osc = Vec::new();
        } else {
            self.osc.clear();
        }
        *run = next;
        self.state = State::Ground;
    }

    /// How long the held string may grow: a clipboard write further.
    fn osc_limit(&self) -> usize {
        if self.osc.starts_with(OSC_52) {
            MAX_OSC_52_BYTES
        } else {
            MAX_OSC_BYTES
        }
    }

    fn holding(&self) -> bool {
        matches!(
            self.state,
            State::Osc
                | State::OscEscape
                | State::Overflow
                | State::OverflowEscape
                | State::ApcStart
                | State::Graphics
                | State::GraphicsEscape
        )
    }
}

fn csi_byte(csi: &mut Csi, byte: u8, win32_input: &mut bool) -> State {
    match byte {
        ESC => State::Escape,
        CAN | SUB => State::Ground,
        b'0'..=b'9' => {
            csi.started = true;
            csi.param = (csi.param * 10 + u32::from(byte - b'0')).min(PARAM_CAP);
            State::Csi(*csi)
        }
        b';' | b':' => {
            csi.started = true;
            csi.end_param();
            State::Csi(*csi)
        }
        b'<'..=b'?' => {
            if !csi.started && !csi.private && byte == b'?' {
                csi.private = true;
            } else {
                csi.invalid = true;
            }
            csi.started = true;
            State::Csi(*csi)
        }
        0x20..=0x2f => {
            csi.invalid = true;
            State::Csi(*csi)
        }
        0x40..=0x7e => {
            csi.end_param();
            if csi.private && !csi.invalid && csi.matched {
                match byte {
                    b'h' => *win32_input = true,
                    b'l' => *win32_input = false,
                    _ => {}
                }
            }
            State::Ground
        }
        _ => State::Csi(*csi),
    }
}
