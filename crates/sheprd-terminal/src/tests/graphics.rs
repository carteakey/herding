use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use sheprd_protocol::{SurfaceGraphicsFormat, SurfaceGraphicsPlacement};
use std::io::Write as _;

/// A PNG's signature and header, `width` by `height`: all the emulator
/// reads of one.
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend_from_slice(&width.to_be_bytes());
    png.extend_from_slice(&height.to_be_bytes());
    png.extend_from_slice(b"\x08\x06\0\0\0");
    png
}

fn command(control: &str, payload: &[u8]) -> Vec<u8> {
    format!("\x1b_G{control};{}\x1b\\", STANDARD.encode(payload)).into_bytes()
}

fn placements(emulator: &mut Emulator) -> Vec<SurfaceGraphicsPlacement> {
    emulator.take_graphics().unwrap().placements
}

#[test]
fn a_placed_png_shows_at_the_cursor_and_moves_it() {
    let mut emulator = fed(20, 6, b"ab");
    emulator.set_cell_pixels(10, 20);
    // 25 by 50 pixels: three columns and three rows of 10 by 20 cells.
    emulator.feed(&command("a=T,f=100,i=7", &png(25, 50)));
    let scene = emulator.take_graphics().unwrap();
    let [placement] = &scene.placements[..] else {
        panic!("{scene:?}")
    };
    assert_eq!(
        (placement.x, placement.y, placement.cols, placement.rows),
        (2, 0, 3, 3)
    );
    assert_eq!(placement.asset.format, SurfaceGraphicsFormat::Png);
    assert_eq!(
        (placement.asset.image_width, placement.asset.image_height),
        (25, 50)
    );
    assert_eq!(scene.assets.len(), 1);
    assert_eq!(scene.assets[0].data, png(25, 50));
    // Down to the image's last row, and past its last column.
    let cursor = emulator.frame().cursor.unwrap();
    assert_eq!((cursor.x, cursor.y), (5, 2));
    // Nothing changed, so nothing new to say.
    assert!(emulator.take_graphics().is_none());
}

#[test]
fn the_mark_is_never_text_and_cursor_movement_can_be_kept() {
    let mut emulator = emulator(20, 4);
    emulator.feed(&command("a=T,f=100,c=2,r=2,C=1", &png(4, 4)));
    emulator.feed(b"x");
    assert_eq!(line(&emulator.frame(), 0), "x");
    assert_eq!(cell(&emulator.frame(), 0, 0).symbol, "x");
}

#[test]
fn chunked_compressed_rgba_arrives_whole() {
    let pixels = vec![200u8; 2 * 2 * 4];
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(&pixels).unwrap();
    let packed = STANDARD.encode(encoder.finish().unwrap());
    let (first, rest) = packed.split_at(8);
    let mut emulator = emulator(10, 4);
    emulator.feed(format!("\x1b_Ga=T,f=32,s=2,v=2,o=z,c=1,r=1,m=1;{first}\x1b\\").as_bytes());
    assert!(
        emulator.take_graphics().is_none(),
        "nothing until the last chunk"
    );
    emulator.feed(format!("\x1b_Gm=0;{rest}\x1b\\").as_bytes());
    let scene = emulator.take_graphics().unwrap();
    assert_eq!(scene.assets[0].data, pixels);
    assert_eq!(
        scene.placements[0].asset.format,
        SurfaceGraphicsFormat::Rgba
    );
}

#[test]
fn files_and_wrong_sizes_are_refused() {
    let mut emulator = emulator(10, 4);
    // A path the program names is never read.
    emulator.feed(&command("a=T,t=f,f=100", b"/etc/passwd"));
    // Raw pixels that disagree with their size.
    emulator.feed(&command("a=T,f=24,s=2,v=2", &[0; 5]));
    // Sides past the limit.
    emulator.feed(&command("a=T,f=100", &png(20_000, 1)));
    // Raw sides whose byte count overflows: refused, never a panic.
    emulator.feed(&command("a=T,f=32,s=4294967295,v=4294967295", &[0; 4]));
    emulator.feed(&command("a=T,f=32,o=z,s=4294967295,v=4294967295", &[0; 4]));
    assert!(emulator.take_graphics().is_none());
    assert_eq!(emulator.frame().cursor.map(|c| (c.x, c.y)), Some((0, 0)));
}

#[test]
fn images_scroll_with_their_text_and_lose_rows_above_the_screen() {
    let mut emulator = emulator(10, 4);
    emulator.feed(&command("a=T,f=100,i=1,c=2,r=2", &png(10, 20)));
    let first = emulator.take_graphics().unwrap();
    assert_eq!(first.placements[0].y, 0);
    emulator.feed(b"\r\n\r\n\r\n");
    // One row scrolled off: the bottom half shows at the top, and the
    // image's bytes are not sent again.
    let scene = emulator.take_graphics().unwrap();
    assert!(scene.assets.is_empty());
    let [placement] = &scene.placements[..] else {
        panic!("{scene:?}")
    };
    assert_eq!((placement.y, placement.rows), (0, 1));
    assert_eq!((placement.source_y, placement.source_height), (10, 10));
    emulator.feed(b"\r\n");
    assert!(placements(&mut emulator).is_empty());
    // Scrolling back up shows it again, bytes and all.
    emulator.scroll(Scroll::Lines(2));
    let scene = emulator.take_graphics().unwrap();
    assert_eq!(scene.placements[0].y, 0);
    assert_eq!(scene.assets.len(), 1);
}

#[test]
fn deleting_or_clearing_takes_images_away() {
    let mut emulator = emulator(10, 4);
    emulator.feed(&command("a=T,f=100,i=3,c=1,r=1", &png(2, 2)));
    assert_eq!(placements(&mut emulator).len(), 1);
    emulator.feed(b"\x1b_Ga=d,d=i,i=3\x1b\\");
    assert!(placements(&mut emulator).is_empty());
    // The image stays: it can be placed again by id.
    emulator.feed(b"\x1b_Ga=p,i=3,c=1,r=1\x1b\\");
    assert_eq!(placements(&mut emulator).len(), 1);
    // Clearing the screen clears the cell that holds the placement.
    emulator.feed(b"\x1b[H\x1b[2J");
    assert!(placements(&mut emulator).is_empty());
}

#[test]
fn a_named_placement_is_replaced_by_the_next_with_its_name() {
    let mut emulator = emulator(10, 4);
    emulator.feed(&command("a=t,f=100,i=4", &png(2, 2)));
    emulator.feed(b"\x1b_Ga=p,i=4,p=1,c=1,r=1,C=1\x1b\\\x1b[3G");
    emulator.feed(b"\x1b_Ga=p,i=4,p=1,c=1,r=1,C=1\x1b\\");
    let [placement] = &placements(&mut emulator)[..] else {
        panic!("one placement")
    };
    assert_eq!(placement.x, 2);
}

#[test]
fn an_oversized_command_is_dropped_and_output_goes_on() {
    let mut emulator = emulator(10, 2);
    let huge = vec![b'A'; MAX_GRAPHICS_COMMAND_BYTES + 10];
    let mut bytes = b"\x1b_Ga=T,f=100;".to_vec();
    bytes.extend_from_slice(&huge);
    bytes.extend_from_slice(b"\x1b\\ok");
    emulator.feed(&bytes);
    assert!(emulator.take_graphics().is_none());
    assert_eq!(line(&emulator.frame(), 0), "ok");
}

#[test]
fn a_command_split_anywhere_is_the_same() {
    let bytes = [
        b"ab".to_vec(),
        command("a=T,f=100,c=2,r=1", &png(3, 3)),
        b"cd".to_vec(),
    ]
    .concat();
    let mut whole = emulator(10, 3);
    whole.feed(&bytes);
    let mut split = emulator(10, 3);
    for byte in &bytes {
        split.feed(std::slice::from_ref(byte));
    }
    assert_eq!(whole.frame(), split.frame());
    assert_eq!(whole.take_graphics(), split.take_graphics());
    // Other APC strings stay ignored.
    let mut other = fed(10, 1, b"\x1b_Xnot graphics\x1b\\ok");
    assert!(other.take_graphics().is_none());
    assert_eq!(line(&other.frame(), 0), "ok");
}

#[test]
fn without_images_the_cursor_still_moves_past_them() {
    let mut emulator = emulator(20, 6).without_images();
    emulator.set_cell_pixels(10, 20);
    emulator.feed(&command("a=T,f=100", &png(25, 50)));
    let cursor = emulator.frame().cursor.unwrap();
    assert_eq!((cursor.x, cursor.y), (3, 2));
}

#[test]
fn a_tall_image_scrolls_like_its_rows_of_line_feeds() {
    let mut emulator = fed(10, 4, b"one\r\ntwo\r\n");
    // The cursor is on row 2 of 4: an image five rows tall walks one row
    // down and scrolls three, as five line feeds would less one.
    emulator.feed(&command("a=T,f=100,c=1,r=5", &png(2, 2)));
    let frame = emulator.frame();
    assert_eq!(
        frame.cursor.as_ref().map(|cursor| (cursor.x, cursor.y)),
        Some((1, 3))
    );
    assert_eq!(line(&frame, 0), "");
    assert_eq!(emulator.scroll_metrics().history, 3);
    // As tall as the image may be, the move is bounded.
    let mut tall = super::emulator(10, 4);
    tall.feed(&command("a=T,f=100,c=1,r=10000", &png(2, 2)));
    assert_eq!(tall.frame().cursor.map(|cursor| cursor.y), Some(3));
}

#[test]
fn a_source_outside_the_image_is_refused_and_one_past_its_edge_is_held() {
    let mut emulator = emulator(10, 4);
    emulator.feed(&command("a=t,f=100,i=2", &png(4, 4)));
    emulator.feed(b"\x1b_Ga=p,i=2,y=9,c=1,r=1,C=1\x1b\\");
    assert!(emulator.take_graphics().is_none());
    emulator.feed(b"\x1b_Ga=p,i=2,y=2,h=4294967295,c=1,r=2,C=1\x1b\\");
    let [placement] = &placements(&mut emulator)[..] else {
        panic!("one placement")
    };
    assert_eq!((placement.source_y, placement.source_height), (2, 2));
}

#[test]
fn an_unfinished_command_lets_the_next_sequence_and_text_through() {
    let mut emulator = fed(10, 2, b"xx");
    emulator.feed(b"\x1b_Ga=T,f=100;AAAA\x1b[2J\x1b[Hok");
    assert!(emulator.take_graphics().is_none());
    assert_eq!(line(&emulator.frame(), 0), "ok");
}

#[test]
fn a_cropped_source_sizes_from_what_it_shows() {
    let mut emulator = emulator(20, 8);
    emulator.set_cell_pixels(10, 10);
    // The right half of a 40 by 20 image is 20 by 20: four columns are
    // four rows.
    emulator.feed(&command("a=T,f=100,x=20,c=4", &png(40, 20)));
    let [placement] = &placements(&mut emulator)[..] else {
        panic!("one placement")
    };
    assert_eq!((placement.cols, placement.rows), (4, 4));
    assert_eq!((placement.source_x, placement.source_width), (20, 20));
    assert_eq!(emulator.frame().cursor.map(|cursor| cursor.y), Some(3));
}

#[test]
fn a_tall_image_shows_its_bottom_after_its_top_scrolls_far_up() {
    let mut emulator = Emulator::new(size(10, 40), 1_000);
    emulator.feed(&command("a=T,f=100,c=1,r=300,C=1", &png(2, 300)));
    emulator.feed(&b"\r\n".repeat(299));
    let [placement] = &placements(&mut emulator)[..] else {
        panic!("one placement")
    };
    assert_eq!((placement.y, placement.rows), (0, 40));
}

#[test]
fn a_tall_image_scrolls_its_whole_height_past_a_short_screen() {
    let mut emulator = Emulator::new(size(10, 40), 1_000);
    emulator.feed(&command("a=T,f=100,c=1,r=300", &png(2, 300)));
    // Down 39 rows and 260 scrolled: the top 260 rows are above the screen.
    assert_eq!(emulator.scroll_metrics().history, 260);
    let [placement] = &placements(&mut emulator)[..] else {
        panic!("one placement")
    };
    assert_eq!((placement.y, placement.rows), (0, 40));
}

#[test]
fn a_size_from_one_side_rounds_the_other_up_whole() {
    let mut emulator = emulator(40, 10);
    emulator.set_cell_pixels(10, 20);
    // 1 column of 10 px makes 49x100 about 20.4 px tall: 2 rows of 20.
    emulator.feed(&command("a=T,f=100,c=1", &png(49, 100)));
    let tall = placements(&mut emulator);
    assert_eq!((tall[0].cols, tall[0].rows), (1, 2));
    // 1 row of 20 px makes 100x49 about 40.8 px wide: 5 columns of 10.
    let mut emulator = super::emulator(40, 10);
    emulator.set_cell_pixels(10, 20);
    emulator.feed(&command("a=T,f=100,r=1", &png(100, 49)));
    let wide = placements(&mut emulator);
    assert_eq!((wide[0].cols, wide[0].rows), (5, 1));
}

/// A 2x30 image over 2 rows of 20 px cells, `offset` px down its first
/// cell, with its first row scrolled above a 4-row screen.
fn scrolled_with_offset(offset: u32) -> SurfaceGraphicsPlacement {
    let mut emulator = emulator(10, 4);
    emulator.set_cell_pixels(10, 20);
    let control = format!("a=T,f=100,c=1,r=2,Y={offset},C=1");
    emulator.feed(&command(&control, &png(2, 30)));
    emulator.feed(&b"\r\n".repeat(4));
    let [placement] = &placements(&mut emulator)[..] else {
        panic!("one placement")
    };
    placement.clone()
}

#[test]
fn an_offset_image_keeps_its_shape_as_it_scrolls_off_the_top() {
    // 30 source px drawn over the 30 px below a 10 px offset: the hidden
    // 20 px row takes the offset and 10 px of the image.
    let cut = scrolled_with_offset(10);
    assert_eq!((cut.y, cut.rows, cut.y_offset), (0, 1, 0));
    assert_eq!((cut.source_y, cut.source_height), (10, 20));
    // An offset past the hidden row only shrinks: the image is whole.
    let moved = scrolled_with_offset(30);
    assert_eq!((moved.rows, moved.y_offset), (1, 10));
    assert_eq!((moved.source_y, moved.source_height), (0, 30));
}

#[test]
fn an_image_redrawn_in_place_leaves_one_marker_in_its_cell() {
    let mut emulator = emulator(10, 4);
    // A preview redrawing its named placement where the cursor stays.
    for _ in 0..50 {
        emulator.feed(&command("a=T,f=100,i=7,p=1,C=1", &png(2, 2)));
    }
    assert_eq!(emulator.zerowidth_at(0, 0).len(), 2);
    assert_eq!(placements(&mut emulator).len(), 1);
}

#[test]
fn a_deleted_image_leaves_no_marker_behind() {
    let mut emulator = emulator(10, 4);
    emulator.feed(&command("a=T,f=100,i=7,C=1", &png(2, 2)));
    // A second image in the same cell: two markers.
    emulator.feed(&command("a=T,f=100,i=8", &png(2, 2)));
    assert_eq!(emulator.zerowidth_at(0, 0).len(), 4);
    emulator.feed(b"\x1b_Ga=d,d=I,i=7\x1b\\");
    // The next scene sweeps the gone placement's marker out of its cell.
    assert_eq!(placements(&mut emulator).len(), 1);
    assert_eq!(emulator.zerowidth_at(0, 0).len(), 2);
}
