//! The kitty graphics protocol, as far as a viewer needs it: images a
//! program sends in the stream (`t=d`, PNG or raw RGB/RGBA, optionally
//! zlib-compressed, in chunks), placed at the cursor, deleted, and handed on
//! as Herdr's graphics scene for the app's image painter. herdr-core answers
//! the program's queries itself, so nothing here ever replies.
//!
//! Each placement leaves a marker in the cell at its top-left corner, two
//! zero-width private-use characters the frame never shows, which together
//! name it among billions, so a mark left behind by a deleted placement
//! never stands for a later one. The marker moves
//! with its row as the screen scrolls, reflows, or switches screens, and goes
//! when the program clears or overwrites that cell, so placements follow
//! the text they sit in without a separate scroll ledger.
//!
//! The stream is untrusted: images are only ever read from it (never from a
//! file, shared memory, or a temporary path the program names), and
//! commands, images, total bytes, and placements are all bounded.

use base64::{
    Engine as _, alphabet,
    engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig},
};
use sheprd_protocol::{
    SurfaceGraphicsAsset, SurfaceGraphicsAssetKey, SurfaceGraphicsFormat, SurfaceGraphicsPlacement,
    SurfaceGraphicsScene, SurfaceGraphicsSource, SurfaceGraphicsTarget,
};
use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    io::Read as _,
    sync::Arc,
};

/// Images kept at once; the least recently used goes first beyond this.
pub(crate) const MAX_IMAGES: usize = 256;
/// Image bytes kept at once, as Herdr keeps at most 64 MiB off screen.
pub(crate) const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
/// Placements kept at once; the oldest goes first beyond this.
pub(crate) const MAX_PLACEMENTS: usize = 1024;
/// Kitty and Ghostty refuse images wider or taller than this.
pub(crate) const MAX_SIDE: u32 = 10_000;
/// A marker's two characters: its high part in Supplementary Private Use
/// Area-A, its low part in Area-B, which no program prints by accident.
const MARKER_HIGH: u32 = 0xF_0000;
const MARKER_LOW: u32 = 0x10_0000;
/// Values each character holds.
const MARKER_SPAN: u32 = 0xFFFD;
/// The most rows above the screen scanned for an image reaching into it:
/// an image taller than this stops showing once its top row scrolls this
/// far up. Scanning is per update, so it stays well short of `MAX_SIDE`.
const MAX_ROWS_ABOVE: usize = 1_000;

/// Lenient about padding, as kitty is.
const BASE64: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

/// Whether `c` is part of a placement marker, which no frame shows.
pub(crate) fn is_marker(c: char) -> bool {
    let c = u32::from(c);
    (MARKER_HIGH..MARKER_HIGH + MARKER_SPAN).contains(&c)
        || (MARKER_LOW..MARKER_LOW + MARKER_SPAN).contains(&c)
}

pub(crate) fn marker_chars(marker: u32) -> Option<[char; 2]> {
    Some([
        char::from_u32(MARKER_HIGH + marker / MARKER_SPAN)?,
        char::from_u32(MARKER_LOW + marker % MARKER_SPAN)?,
    ])
}

/// The markers among a cell's zero-width characters.
pub(crate) fn markers_in(chars: &[char]) -> impl Iterator<Item = u32> + '_ {
    chars.windows(2).filter_map(|pair| {
        let (high, low) = (u32::from(pair[0]), u32::from(pair[1]));
        let high = high
            .checked_sub(MARKER_HIGH)
            .filter(|high| *high < MARKER_SPAN)?;
        let low = low
            .checked_sub(MARKER_LOW)
            .filter(|low| *low < MARKER_SPAN)?;
        Some(high * MARKER_SPAN + low)
    })
}

/// A command's control data: `key=value` pairs, the keys this module reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Control {
    action: u8,
    medium: u8,
    format: u32,
    compressed: bool,
    more: bool,
    /// `i`: the program's image id; 0 for an anonymous image.
    id: u32,
    /// `p`: the placement id within the image.
    placement: u32,
    width: u32,
    height: u32,
    cols: u32,
    rows: u32,
    source: [u32; 4],
    offset: [u32; 2],
    z: i32,
    /// `C=1`: the cursor stays where it is.
    cursor_stays: bool,
    delete: u8,
}

impl Control {
    fn parse(control: &[u8]) -> Self {
        let mut parsed = Self {
            action: b't',
            medium: b'd',
            format: 32,
            delete: b'a',
            ..Self::default()
        };
        for pair in control.split(|byte| *byte == b',') {
            let [key, b'=', value @ ..] = pair else {
                continue;
            };
            let number = || {
                std::str::from_utf8(value)
                    .ok()
                    .and_then(|value| value.parse::<u32>().ok())
                    .unwrap_or(0)
            };
            let letter = value.first().copied().unwrap_or(0);
            match key {
                b'a' => parsed.action = letter,
                b't' => parsed.medium = letter,
                b'f' => parsed.format = number(),
                b'o' => parsed.compressed = letter == b'z',
                b'm' => parsed.more = number() == 1,
                b'i' => parsed.id = number(),
                b'p' => parsed.placement = number(),
                b's' => parsed.width = number(),
                b'v' => parsed.height = number(),
                b'c' => parsed.cols = number(),
                b'r' => parsed.rows = number(),
                b'x' => parsed.source[0] = number(),
                b'y' => parsed.source[1] = number(),
                b'w' => parsed.source[2] = number(),
                b'h' => parsed.source[3] = number(),
                b'X' => parsed.offset[0] = number(),
                b'Y' => parsed.offset[1] = number(),
                b'z' => {
                    parsed.z = std::str::from_utf8(value)
                        .ok()
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(0)
                }
                b'C' => parsed.cursor_stays = number() == 1,
                b'd' => parsed.delete = letter,
                _ => {}
            }
        }
        parsed
    }
}

struct Image {
    key: SurfaceGraphicsAssetKey,
    data: Arc<[u8]>,
    used: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Placement {
    image: u32,
    /// `(image, p)` for a placement the program named, which a later one
    /// with the same pair replaces.
    named: Option<u32>,
    cols: u32,
    rows: u32,
    source: [u32; 4],
    offset: [u32; 2],
    z: i32,
}

/// Where a command placed an image, for the emulator to mark the cell and
/// move the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Placed {
    pub(crate) marker: [char; 2],
    pub(crate) cols: u32,
    pub(crate) rows: u32,
    pub(crate) moves_cursor: bool,
}

/// A transmission still arriving in chunks.
struct Loading {
    control: Control,
    data: Vec<u8>,
}

/// The cell a marker was found in, as rows from the top of the screen
/// (negative above it) and a column.
pub(crate) struct Found {
    pub(crate) marker: u32,
    pub(crate) row: i64,
    pub(crate) column: u16,
}

pub(crate) struct Graphics {
    images: HashMap<u32, Image>,
    bytes: usize,
    clock: u64,
    loading: Option<Loading>,
    placements: HashMap<u32, Placement>,
    next_marker: u32,
    next_anonymous: u32,
    /// The cell size in pixels programs are told, for images placed without
    /// a size in cells.
    cell: (u32, u32),
    /// The placements the last scene listed.
    shown: Vec<SurfaceGraphicsPlacement>,
    /// Whether images keep their pixels; without, they keep only their size,
    /// which still moves the cursor as the program expects.
    pixels: bool,
}

impl Default for Graphics {
    fn default() -> Self {
        Self {
            images: HashMap::new(),
            bytes: 0,
            clock: 0,
            loading: None,
            placements: HashMap::new(),
            next_marker: 0,
            // Anonymous images take ids no program sends.
            next_anonymous: u32::MAX,
            cell: (8, 16),
            shown: Vec::new(),
            pixels: true,
        }
    }
}

impl Graphics {
    /// Keeps images' sizes only, for a viewer that never shows them.
    pub(crate) fn drop_pixels(&mut self) {
        self.pixels = false;
    }

    pub(crate) fn set_cell(&mut self, width: u32, height: u32) {
        self.cell = (width.max(1), height.max(1));
    }

    /// The most rows any placement covers, which bounds how far above the
    /// screen a marker can still show part of its image.
    /// Whether `marker` names a placement still kept; any other marker in
    /// a cell is left from one replaced or deleted.
    pub(crate) fn placed(&self, marker: u32) -> bool {
        self.placements.contains_key(&marker)
    }

    pub(crate) fn tallest(&self) -> usize {
        let rows = self.placements.values().map(|placement| placement.rows);
        usize::try_from(rows.max().unwrap_or(0))
            .unwrap_or(usize::MAX)
            .min(MAX_ROWS_ABOVE)
    }

    /// Whether a scene may have something to say: a placement to show, or
    /// one shown that may be gone.
    pub(crate) fn active(&self) -> bool {
        !self.placements.is_empty() || !self.shown.is_empty()
    }

    /// Runs one command, `ESC _ G` already taken off; what it placed, if
    /// anything.
    pub(crate) fn command(&mut self, command: &[u8]) -> Option<Placed> {
        let (control, payload) = match command.iter().position(|byte| *byte == b';') {
            Some(split) => (&command[..split], &command[split + 1..]),
            None => (command, &[][..]),
        };
        let parsed = Control::parse(control);
        // A chunk after the first carries only `m` (and `q`); it belongs to
        // the transmission in progress.
        if let Some(mut loading) = self.loading.take() {
            if !append(&mut loading.data, payload) {
                return None;
            }
            if parsed.more {
                self.loading = Some(loading);
                return None;
            }
            return self.transmitted(loading.control, loading.data);
        }
        match parsed.action {
            b't' | b'T' => {
                let mut data = Vec::new();
                if !append(&mut data, payload) {
                    return None;
                }
                if parsed.more {
                    self.loading = Some(Loading {
                        control: parsed,
                        data,
                    });
                    return None;
                }
                self.transmitted(parsed, data)
            }
            b'p' => self.place(&parsed, parsed.id),
            b'd' => {
                self.delete(&parsed);
                None
            }
            // Queries and animation are answered or ignored by herdr-core.
            _ => None,
        }
    }

    fn transmitted(&mut self, control: Control, data: Vec<u8>) -> Option<Placed> {
        // Never a file, shared memory, or temporary path the program names.
        if control.medium != b'd' {
            return None;
        }
        let id = if control.id == 0 {
            self.next_anonymous = match self.next_anonymous {
                id if id <= 1 << 31 => u32::MAX,
                id => id - 1,
            };
            self.next_anonymous
        } else {
            control.id
        };
        self.store(&control, id, data)?;
        (control.action == b'T')
            .then(|| self.place(&control, id))
            .flatten()
    }

    fn store(&mut self, control: &Control, id: u32, data: Vec<u8>) -> Option<()> {
        let (format, bytes_per_pixel) = match control.format {
            24 => (SurfaceGraphicsFormat::Rgb, Some(3)),
            32 => (SurfaceGraphicsFormat::Rgba, Some(4)),
            100 => (SurfaceGraphicsFormat::Png, None),
            _ => return None,
        };
        // Sides are checked below, after a PNG's are read; until then they
        // are any `u32`, which saturating keeps from overflowing.
        let expected = bytes_per_pixel.map(|bpp| {
            u64::from(control.width)
                .saturating_mul(u64::from(control.height))
                .saturating_mul(bpp)
        });
        let limit = expected.map_or(MAX_IMAGE_BYTES as u64, |expected| {
            expected.min(MAX_IMAGE_BYTES as u64)
        });
        let data = if control.compressed {
            inflate(&data, limit)?
        } else {
            data
        };
        let (width, height) = match format {
            SurfaceGraphicsFormat::Png => png_size(&data)?,
            _ => (control.width, control.height),
        };
        let sides = 1..=MAX_SIDE;
        if !sides.contains(&width) || !sides.contains(&height) {
            return None;
        }
        if expected.is_some_and(|expected| u64::try_from(data.len()).ok() != Some(expected))
            || data.is_empty()
            || data.len() > MAX_IMAGE_BYTES
        {
            return None;
        }
        let key = SurfaceGraphicsAssetKey {
            source: SurfaceGraphicsSource::Terminal {
                target: SurfaceGraphicsTarget::Pane {
                    pane_id: String::new(),
                },
                image_id: id,
            },
            image_width: width,
            image_height: height,
            format,
            data_len: data.len() as u64,
            data_fingerprint: fingerprint(&data),
        };
        let data = if self.pixels { data } else { Vec::new() };
        if let Some(old) = self.images.remove(&id) {
            self.bytes -= old.data.len();
        }
        self.make_room(data.len());
        self.bytes += data.len();
        self.clock += 1;
        self.images.insert(
            id,
            Image {
                key,
                data: Arc::from(data),
                used: self.clock,
            },
        );
        Some(())
    }

    /// Frees the least recently used images until `incoming` fits.
    fn make_room(&mut self, incoming: usize) {
        while !self.images.is_empty()
            && (self.images.len() >= MAX_IMAGES
                || self.bytes.saturating_add(incoming) > MAX_IMAGE_BYTES)
        {
            let Some(oldest) = self
                .images
                .iter()
                .min_by_key(|(_, image)| image.used)
                .map(|(id, _)| *id)
            else {
                return;
            };
            self.forget(oldest);
        }
    }

    fn forget(&mut self, id: u32) {
        if let Some(image) = self.images.remove(&id) {
            self.bytes -= image.data.len();
        }
        self.placements.retain(|_, placement| placement.image != id);
    }

    fn place(&mut self, control: &Control, id: u32) -> Option<Placed> {
        let image = self.images.get_mut(&id)?;
        self.clock += 1;
        image.used = self.clock;
        let (width, height) = (image.key.image_width, image.key.image_height);
        // The source rectangle is held inside the image; one starting
        // outside it shows nothing.
        let [source_x, source_y, source_width, source_height] = control.source;
        if source_x >= width || source_y >= height {
            return None;
        }
        let side = |wanted: u32, room: u32| if wanted == 0 { room } else { wanted.min(room) };
        let shown_width = side(source_width, width - source_x);
        let shown_height = side(source_height, height - source_y);
        let source = [source_x, source_y, shown_width, shown_height];
        let (cell_width, cell_height) = self.cell;
        let pixels_wide = shown_width.saturating_add(control.offset[0]);
        let pixels_high = shown_height.saturating_add(control.offset[1]);
        // A size in cells wins; one side alone keeps the image's shape.
        let (cols, rows) = match (control.cols, control.rows) {
            (0, 0) => (
                pixels_wide.div_ceil(cell_width),
                pixels_high.div_ceil(cell_height),
            ),
            // One rounding, up, of the whole ratio: a fraction of a pixel
            // dropped first can cost the image a row it reaches into.
            (cols, 0) => (
                cols,
                cells_for(cols, cell_width, shown_height, shown_width, cell_height)?,
            ),
            (0, rows) => (
                cells_for(rows, cell_height, shown_width, shown_height, cell_width)?,
                rows,
            ),
            sized => sized,
        };
        let (cols, rows) = (cols.clamp(1, MAX_SIDE), rows.clamp(1, MAX_SIDE));
        let named = (control.placement != 0).then_some(control.placement);
        if named.is_some() {
            self.placements
                .retain(|_, placement| (placement.image, placement.named) != (id, named));
        }
        while self.placements.len() >= MAX_PLACEMENTS {
            let Some(oldest) = self.placements.keys().min().copied() else {
                break;
            };
            self.placements.remove(&oldest);
        }
        let marker = self.next_marker % (MARKER_SPAN * MARKER_SPAN);
        self.next_marker = self.next_marker.wrapping_add(1);
        self.placements.insert(
            marker,
            Placement {
                image: id,
                named,
                cols,
                rows,
                source,
                offset: control.offset,
                z: control.z,
            },
        );
        Some(Placed {
            marker: marker_chars(marker)?,
            cols,
            rows,
            moves_cursor: !control.cursor_stays,
        })
    }

    /// `a=d`: every placement, or an image's (`i`, and `p` for one of
    /// them). An uppercase target frees the image's data too.
    fn delete(&mut self, control: &Control) {
        match control.delete {
            b'a' | b'A' => {
                self.placements.clear();
                if control.delete == b'A' {
                    let unplaced: Vec<u32> = self.images.keys().copied().collect();
                    for id in unplaced {
                        self.forget(id);
                    }
                }
            }
            b'i' | b'I' => {
                let (id, named) = (
                    control.id,
                    (control.placement != 0).then_some(control.placement),
                );
                self.placements.retain(|_, placement| {
                    placement.image != id
                        || named.is_some_and(|named| placement.named != Some(named))
                });
                if control.delete == b'I' && named.is_none() {
                    self.forget(id);
                }
            }
            // Deleting by cell, column, row, or z is not read yet.
            _ => {}
        }
    }

    /// The scene for the markers found on and above the screen, when it
    /// differs from the last one; it carries the bytes of images the last
    /// scene did not show.
    pub(crate) fn scene(&mut self, found: &[Found]) -> Option<SurfaceGraphicsScene> {
        let placements: Vec<SurfaceGraphicsPlacement> = found
            .iter()
            .filter_map(|found| {
                let marker = found.marker;
                let placement = self.placements.get(&marker)?;
                let image = self.images.get(&placement.image)?;
                visible(
                    marker,
                    placement,
                    &image.key,
                    (found.row, found.column),
                    self.cell.1,
                )
            })
            .collect();
        if placements == self.shown {
            return None;
        }
        let before: HashSet<&SurfaceGraphicsAssetKey> = self
            .shown
            .iter()
            .map(|placement| &placement.asset)
            .collect();
        let mut added = HashSet::new();
        let assets = placements
            .iter()
            .filter(|placement| !before.contains(&placement.asset))
            .filter(|placement| added.insert(placement.asset.clone()))
            .filter_map(|placement| {
                let image = self
                    .images
                    .values()
                    .find(|image| image.key == placement.asset)?;
                Some(SurfaceGraphicsAsset {
                    key: image.key.clone(),
                    data: image.data.to_vec(),
                })
            })
            .collect();
        self.shown.clone_from(&placements);
        Some(SurfaceGraphicsScene {
            assets,
            placements,
            retained_assets: Vec::new(),
        })
    }
}

/// The part of a placement whose top-left cell is `row` rows from the top
/// of the screen; an image starting above the screen loses its top rows.
/// The painter draws the image over its rows below its vertical offset, so
/// a hidden row first takes up the offset and only then cuts the image, by
/// as much of the source as it covered: the rest keeps its shape.
fn visible(
    marker: u32,
    placement: &Placement,
    key: &SurfaceGraphicsAssetKey,
    (row, column): (i64, u16),
    cell_height: u32,
) -> Option<SurfaceGraphicsPlacement> {
    let hidden = u32::try_from((-row).max(0)).ok()?;
    if hidden >= placement.rows {
        return None;
    }
    // `place` held the source rectangle inside the image.
    let [source_x, mut source_y, source_width, mut source_height] = placement.source;
    let mut y_offset = placement.offset[1];
    if hidden > 0 {
        let cell = u64::from(cell_height.max(1));
        let cells = u64::from(placement.rows) * cell;
        // As the painter holds it: inside the cells, leaving a pixel.
        let offset = u64::from(placement.offset[1]).min(cells - 1);
        let drawn = cells - offset;
        let hidden = u64::from(hidden) * cell;
        if hidden <= offset {
            y_offset = u32::try_from(offset - hidden).ok()?;
        } else {
            let cut = (hidden - offset).saturating_mul(u64::from(source_height)) / drawn;
            let cut = u32::try_from(cut).ok()?.min(source_height);
            source_y = source_y.saturating_add(cut);
            source_height -= cut;
            y_offset = 0;
        }
    }
    Some(SurfaceGraphicsPlacement {
        asset: key.clone(),
        logical_placement_id: marker,
        x: column,
        y: u16::try_from(row.max(0)).ok()?,
        cols: placement.cols,
        rows: placement.rows - hidden,
        source_x,
        source_y,
        source_width,
        source_height,
        x_offset: placement.offset[0],
        y_offset,
        z: placement.z,
        scrollback_offset: 0,
    })
}

/// Appends a chunk's decoded payload; false (and nothing kept) when it is
/// not base64 or the transmission grows past what an image may hold.
fn append(data: &mut Vec<u8>, payload: &[u8]) -> bool {
    if payload.is_empty() {
        return true;
    }
    if data.len().saturating_add(payload.len() / 4 * 3) > MAX_IMAGE_BYTES {
        data.clear();
        return false;
    }
    match BASE64.decode_vec(payload, data) {
        Ok(()) => true,
        Err(_) => {
            data.clear();
            false
        }
    }
}

/// zlib-inflates `data` to at most `limit` bytes; `None` past it.
fn inflate(data: &[u8], limit: u64) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(data)
        .take(limit.saturating_add(1))
        .read_to_end(&mut out)
        .ok()?;
    (u64::try_from(out.len()).ok()? <= limit).then_some(out)
}

/// A PNG's width and height from its header.
fn png_size(data: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    let header = data.get(..24)?;
    if &header[..8] != SIGNATURE || &header[12..16] != b"IHDR" {
        return None;
    }
    let side = |bytes: &[u8]| Some(u32::from_be_bytes(bytes.try_into().ok()?));
    Some((side(&header[16..20])?, side(&header[20..24])?))
}

/// The cells along one side for an image given `cells` along the other:
/// `cells * cell * side / (other_side * other_cell)`, rounded up once.
fn cells_for(cells: u32, cell: u32, side: u32, other_side: u32, other_cell: u32) -> Option<u32> {
    let pixels = u64::from(cells)
        .saturating_mul(u64::from(cell))
        .saturating_mul(u64::from(side));
    let per_cell = u64::from(other_side.max(1)).saturating_mul(u64::from(other_cell.max(1)));
    u32::try_from(pixels.div_ceil(per_cell)).ok()
}

fn fingerprint(data: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut hasher);
    hasher.finish()
}
