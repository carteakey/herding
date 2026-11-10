use super::*;

#[test]
fn plain_text_lands_in_rows() {
    let frame = fed(10, 3, b"hello\r\nworld").frame();
    assert_eq!((frame.width, frame.height), (10, 3));
    assert_eq!(frame.cells.len(), 30);
    assert_eq!(line(&frame, 0), "hello");
    assert_eq!(line(&frame, 1), "world");
    assert_eq!(line(&frame, 2), "");
    assert_eq!(cell(&frame, 7, 0).symbol, " ");
    assert!(frame.graphics.is_empty());
    frame.validate().unwrap();
}

#[test]
fn long_lines_wrap() {
    let frame = fed(4, 3, b"abcdefghij").frame();
    assert_eq!(line(&frame, 0), "abcd");
    assert_eq!(line(&frame, 1), "efgh");
    assert_eq!(line(&frame, 2), "ij");
}

#[test]
fn wide_characters_skip_their_second_cell() {
    let frame = fed(6, 1, "a界b".as_bytes()).frame();
    assert_eq!(cell(&frame, 1, 0).symbol, "界");
    assert!(!cell(&frame, 1, 0).skip);
    let spacer = cell(&frame, 2, 0);
    assert!(spacer.skip);
    assert_eq!(spacer.symbol, "");
    assert_eq!(cell(&frame, 3, 0).symbol, "b");
}

#[test]
fn a_wide_character_that_does_not_fit_wraps_leaving_a_blank() {
    let frame = fed(3, 2, "ab界".as_bytes()).frame();
    assert_eq!(cell(&frame, 2, 0).symbol, " ");
    assert!(!cell(&frame, 2, 0).skip);
    assert_eq!(cell(&frame, 0, 1).symbol, "界");
    assert!(cell(&frame, 1, 1).skip);
}

#[test]
fn combining_characters_join_their_base() {
    let frame = fed(5, 1, "e\u{301}x".as_bytes()).frame();
    assert_eq!(cell(&frame, 0, 0).symbol, "e\u{301}");
    assert_eq!(cell(&frame, 1, 0).symbol, "x");
}

#[test]
fn tabs_show_as_blanks() {
    let frame = fed(12, 1, b"a\tb").frame();
    assert_eq!(cell(&frame, 1, 0).symbol, " ");
    assert_eq!(cell(&frame, 8, 0).symbol, "b");
}

#[test]
fn rows_the_terminal_wrapped_are_told_from_lines_that_fill_them() {
    let mut emulator = fed(4, 5, b"abcdefghij\r\nwxyz\r\nk");
    // "abcd" and "efgh" go on; "wxyz" fills its row but ends there.
    assert_eq!(emulator.wrapped_rows(), [true, true, false, false, false]);
    // Two lines scroll "abcd" and "efgh" off; scrolled back, the flags
    // follow the rows shown.
    emulator.feed(b"\r\n\r\n");
    assert_eq!(emulator.wrapped_rows(), [false, false, false, false, false]);
    emulator.scroll(Scroll::Lines(1));
    assert_eq!(emulator.wrapped_rows(), [true, false, false, false, false]);
}
