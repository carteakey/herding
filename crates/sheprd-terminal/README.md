# sheprd-terminal

The terminal emulator behind herdr-core terminals. Herdr 1 sends rendered
cell grids; herdr-core sends each terminal's raw PTY output (its first paint
is VT bytes too), so the client emulates the terminal itself. This crate
wraps Zed's fork of `alacritty_terminal` and turns its grid into the same
`sheprd_protocol::FrameData` the app's painter already draws.

It is pure and deterministic: no GPUI, I/O, threads, or clock. The same bytes
give the same frame however they are split across `Emulator::feed` calls.
It never answers terminal queries (herdr-core does), writes to the
clipboard, or opens hyperlinks. An OSC 52 clipboard write comes out as an
event with its decoded UTF-8 text for the caller to apply; reads are refused.
Images a program sends with the kitty graphics protocol come out as Herdr's
graphics scene for the app's painter.

## Use

- `Emulator::new(Size, scrollback_lines)`, then `feed(bytes)` as output
  arrives.
- `take_damage()` returns `Damage::Full(FrameData)` (first take, resize,
  alternate screen, clears, scrolling the view) or `Damage::Rows` with whole
  changed rows as `PaneSurfacePatchRow` plus the cursor. Rows only index
  hyperlinks the last full frame listed; a new link makes the take full.
- `frame()` is the viewport at any time; it does not reset damage.
- `modes()`, `title()`, `take_events()` (one coalesced title change, the
  latest OSC 52 clipboard write, and a bell count), `scroll()` and `scroll_metrics()`.
- `history(&History)` answers what a view asks of its history: `search()`
  (smart-case, literal, wrapping, at most `MAX_SEARCH_MATCHES` around the
  current match), `motion()` (copy-mode words, line ends, paragraphs),
  `read()`, `text()`, `scroll_to()`, and `clear()`, which drops the history
  and the rows above the cursor's line, leaving an alternate screen alone.
  `revision()` counts changes to the text; history points hold for one.
- `encode(&ClientPaneInputEvent)` (or `input::encode` with `Modes`) turns the
  app's semantic input into PTY bytes: xterm keys, the kitty keyboard
  protocol, Win32 input records (DECSET 9001), mouse reports, and bracketed
  paste.
- `take_graphics()` returns the kitty graphics placements on screen when
  they changed, as a `SurfaceGraphicsScene` carrying the bytes of each image
  the previous one did not show; `set_cell_pixels()` sizes images placed
  without a size in cells. Only images sent in the stream are read (`t=d`,
  PNG or raw RGB/RGBA, zlib or not, chunked or not), never a file, shared
  memory, or temporary path the program names. Each placement marks its
  top-left cell with a zero-width private-use character the frame never
  shows, so it scrolls, reflows, and clears with its text. Deletion by
  image and of all placements is read; by cell, row, column, or z is not,
  nor are animation and Unicode placeholders.
- `sync_pending()` and `end_sync()` let the caller time out a synchronized
  update (DECSET 2026) with its own clock.

Everything is bounded: sizes (`Size` from herdr-backend, with `MAX_SIDE`
and `MAX_CELLS`), history
(`MAX_SCROLLBACK_LINES`, `MAX_SCROLLBACK_CELLS`), OSC strings
(`MAX_OSC_BYTES`, longer ones are dropped whole; an OSC 52 write may carry
up to `MAX_CLIPBOARD_BYTES` of text), graphics commands
(`MAX_GRAPHICS_COMMAND_BYTES`), images (count, bytes, sides) and placements,
and repeats
(`input::MAX_REPEAT`).

## Cell contract

Colors in `CellData::fg` and `bg` keep what the program chose, and the app's
theme resolves them:

| Top byte | Meaning |
| --- | --- |
| `0` | Low byte `1..=16` is palette index `value - 1` (black to bright white); `0` is the default foreground or background |
| `1` | `0x010000XX`: 256-color index `XX` |
| `2` | `0x02RRGGBB`: RGB |

Alacritty's dim colors become the matching normal color with `DIM`.

`CellData::modifier` bits are the crate's `BOLD` (1), `DIM` (1 << 1),
`ITALIC` (1 << 2), `UNDERLINE` (1 << 3, every underline style), `REVERSED`
(1 << 6), `HIDDEN` (1 << 7), and `STRIKETHROUGH` (1 << 8).

A wide character's second cell is `skip: true` with an empty symbol;
combining characters join their base cell's symbol. OSC 8 links are listed
once in `FrameData::hyperlinks` and cells carry their index.

`CursorState::shape` is a DECSCUSR parameter: 0 is the user's default style
(no program chose one), 1/2 block, 3/4 underline, 5/6 bar, odd values blink.
A hidden cursor is `visible: false`; one scrolled out of view is `None`.

Input modifier bits (`input::SHIFT` 1, `CONTROL` 2, `ALT` 4, `SUPER` 8,
`HYPER` 16, `META` 32) are the ones `ClientPaneInputEvent` carries.
