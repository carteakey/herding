//! A Herdr 2 screen keeps history: a phone reads it back or scrolls a view
//! through it, and the shared emulator stays live for everyone else.

use super::*;
use crate::hub::core::lines::Lines;
use sheprd_terminal::{Emulator, Size};

fn numbered(lines: usize) -> Emulator {
    let mut emulator = Emulator::new(Size::new(10, 3).unwrap(), 100);
    let text = (1..=lines)
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\r\n");
    emulator.feed(text.as_bytes());
    emulator
}

fn plain(read: &Value) -> Vec<String> {
    let text = read["read"]["text"].as_str().unwrap();
    crate::ansi::parse(text)
        .lines
        .iter()
        .map(|runs| {
            let line: String = runs.iter().map(|(_, text)| text.as_str()).collect();
            line.trim_end().to_owned()
        })
        .collect()
}

#[test]
fn recent_history_reads_back_oldest_first_down_to_the_screen() {
    let mut emulator = numbered(10);
    let read = screen::read(
        &mut emulator,
        &json!({"source": "recent", "lines": 1000}),
        &Lines::from(0),
    );
    assert_eq!(read["read"]["history"], 7);
    let expected = (1..=10).map(|line| line.to_string()).collect::<Vec<_>>();
    assert_eq!(plain(&read), expected);
    let read = screen::read(
        &mut emulator,
        &json!({"source": "recent", "lines": 4}),
        &Lines::from(0),
    );
    assert_eq!(plain(&read), ["4", "5", "6", "7", "8", "9", "10"]);
}

#[test]
fn a_scrolled_read_shows_history_and_leaves_the_screen_live() {
    let mut emulator = numbered(10);
    let read = screen::read(&mut emulator, &json!({"scroll": 2}), &Lines::from(0));
    assert_eq!(plain(&read), ["6", "7", "8"]);
    // Past the top shows the top.
    let read = screen::read(&mut emulator, &json!({"scroll": 500}), &Lines::from(0));
    assert_eq!(plain(&read), ["1", "2", "3"]);
    let read = screen::read(&mut emulator, &json!({}), &Lines::from(0));
    assert_eq!(plain(&read), ["8", "9", "10"]);
}

#[test]
fn a_screen_without_history_reads_only_itself() {
    let mut emulator = numbered(2);
    let read = screen::read(
        &mut emulator,
        &json!({"source": "recent", "lines": 1000}),
        &Lines::from(0),
    );
    assert_eq!(read["read"]["history"], 0);
    assert_eq!(plain(&read), ["1", "2", ""]);
}

#[test]
fn feeding_counts_the_lines_pushed_into_history() {
    let mut emulator = Emulator::new(Size::new(10, 3).unwrap(), 5);
    // Nothing counts into an empty history, where no view is scrolled.
    assert_eq!(screen::feed(&mut emulator, b"1\r\n2\r\n3\r\n4"), 0);
    assert_eq!(emulator.scroll_metrics().history, 1);
    assert_eq!(screen::feed(&mut emulator, b"\r\n5\r\n6"), 2);
    // A full history still counts what passes through it.
    assert_eq!(screen::feed(&mut emulator, b"\r\n7\r\n8"), 2);
    assert_eq!(emulator.scroll_metrics().history, 5);
    assert_eq!(screen::feed(&mut emulator, b"\r\n9\r\n10"), 2);
    // A burst past the whole history counts one short of it, which still
    // takes any scrolled view to the top.
    assert_eq!(
        screen::feed(&mut emulator, b"\r\na\r\nb\r\nc\r\nd\r\ne\r\nf"),
        4
    );
    assert_eq!(emulator.scroll_metrics().offset, 0, "left live");
    assert_eq!(screen::feed(&mut emulator, b"x"), 0);
}

#[test]
fn a_scrolled_view_stays_on_its_lines_as_output_arrives() {
    let mut emulator = numbered(10);
    let read = screen::read(
        &mut emulator,
        &json!({"scroll": 2, "since": 0}),
        &Lines::from(0),
    );
    assert_eq!(plain(&read), ["6", "7", "8"]);
    let since = read["read"]["lines"].as_u64().unwrap();
    let pushed = screen::feed(&mut emulator, b"\r\n11\r\n12");
    assert_eq!(pushed, 2);
    let read = screen::read(
        &mut emulator,
        &json!({"scroll": 2, "since": since}),
        &Lines::from(since + pushed),
    );
    assert_eq!(plain(&read), ["6", "7", "8"]);
    assert_eq!(read["read"]["scroll"], 4);
    // A view at the live screen follows it.
    let read = screen::read(
        &mut emulator,
        &json!({"scroll": 0, "since": since}),
        &Lines::from(since + pushed),
    );
    assert_eq!(plain(&read), ["10", "11", "12"]);
}

#[test]
fn leaving_a_full_screen_program_moves_no_view() {
    let mut emulator = numbered(10);
    // Into the alternate screen, which has no history, and out again.
    assert_eq!(screen::feed(&mut emulator, b"\x1b[?1049h\x1b[2Jvim"), 0);
    assert_eq!(emulator.scroll_metrics().history, 0);
    assert_eq!(screen::feed(&mut emulator, b"\x1b[?1049l"), 0);
    assert_eq!(
        emulator.scroll_metrics().history,
        7,
        "the shell's history is back"
    );
}

#[test]
fn a_scrolled_view_keeps_its_place_as_the_screen_rewraps() {
    let mut emulator = Emulator::new(Size::new(10, 3).unwrap(), 100);
    emulator.feed(b"1\r\nabcdefgh\r\n3\r\n4\r\nabcdefgh\r\n6\r\n7\r\n8");
    let mut lines = Lines::default();
    let read = screen::read(&mut emulator, &json!({"scroll": 2, "since": 0}), &lines);
    assert_eq!(plain(&read), ["4", "abcdefgh", "6"]);
    let since = read["read"]["lines"].as_u64().unwrap();
    // Narrower, both long lines take two rows: the one above the view
    // moves nothing, the one in it pushes the view's lines up.
    lines.resize(&mut emulator, Size::new(4, 3).unwrap(), 100);
    let read = screen::read(&mut emulator, &json!({"scroll": 2, "since": since}), &lines);
    assert_eq!(plain(&read), ["4", "abcd", "efgh"]);
    assert_eq!(read["read"]["scroll"], 3);
    // Output after the resize pushes it further.
    let since = read["read"]["lines"].as_u64().unwrap();
    lines.pushed(screen::feed(&mut emulator, b"\r\n9"));
    let read = screen::read(&mut emulator, &json!({"scroll": 3, "since": since}), &lines);
    assert_eq!(plain(&read), ["4", "abcd", "efgh"]);
    // Wider again, from a read before both resizes.
    lines.resize(&mut emulator, Size::new(10, 3).unwrap(), 100);
    let read = screen::read(&mut emulator, &json!({"scroll": 2, "since": 0}), &lines);
    assert_eq!(plain(&read), ["4", "abcdefgh", "6"]);
}
