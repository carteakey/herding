use super::*;

#[test]
fn title_changes_coalesce_to_the_latest() {
    let mut emulator = fed(5, 1, b"\x1b]0;one\x07\x1b]2;two\x1b\\");
    assert_eq!(emulator.title(), Some("two"));
    emulator.feed(b"\x1b]0;three\x07");
    assert_eq!(
        emulator.take_events(),
        [Event::Title(Some("three".to_owned()))]
    );
    assert!(emulator.take_events().is_empty());
}

#[test]
fn setting_the_same_title_again_is_no_change() {
    let mut emulator = fed(5, 1, b"\x1b]0;same\x07");
    let _ = emulator.take_events();
    emulator.feed(b"\x1b]0;other\x07\x1b]0;same\x07");
    assert!(emulator.take_events().is_empty());
}

#[test]
fn bells_are_counted() {
    let mut emulator = fed(5, 1, b"\x07a\x07\x07");
    assert_eq!(emulator.take_events(), [Event::Bell { count: 3 }]);
    assert!(emulator.take_events().is_empty());
}

#[test]
fn queries_and_clipboard_reads_are_ignored() {
    // Device attributes, cursor position, an OSC 52 load, and a write to
    // the X selection rather than the clipboard.
    let mut emulator = fed(5, 1, b"\x1b[c\x1b[6n\x1b]52;c;?\x07\x1b]52;p;aGk=\x07");
    assert!(emulator.take_events().is_empty());
    assert_eq!(line(&emulator.frame(), 0), "");
}

#[test]
fn clipboard_writes_coalesce_to_the_latest_text() {
    let mut emulator = fed(5, 1, b"\x1b]52;c;b25l\x07\x1b]52;c;dHdv\x1b\\");
    assert_eq!(emulator.take_events(), [Event::Clipboard("two".into())]);
    assert!(emulator.take_events().is_empty());
    // An empty write clears the clipboard, as terminals do.
    emulator.feed(b"\x1b]52;c;\x07");
    assert_eq!(emulator.take_events(), [Event::Clipboard(String::new())]);
    // Split anywhere, the write is the same.
    let mut split = super::emulator(5, 1);
    for byte in b"\x1b]52;c;5Lmf5LiN\x07" {
        split.feed(&[*byte]);
    }
    assert_eq!(
        split.take_events(),
        [Event::Clipboard("\u{4e5f}\u{4e0d}".into())]
    );
}

#[test]
fn clipboard_writes_must_be_utf8_base64() {
    // 0xff is not UTF-8, and `!` is not base64.
    let mut emulator = fed(5, 1, b"\x1b]52;c;/w==\x07\x1b]52;c;aGk!\x07");
    assert!(emulator.take_events().is_empty());
}

#[test]
fn clipboard_writes_are_bounded() {
    // The largest write passes the OSC bound other strings have.
    let largest = "QUFB".repeat(MAX_CLIPBOARD_BYTES / 3);
    let mut emulator = emulator(5, 1);
    emulator.feed(format!("\x1b]52;c;{largest}\x07").as_bytes());
    let events = emulator.take_events();
    assert!(
        matches!(&events[..], [Event::Clipboard(text)] if text.len() == MAX_CLIPBOARD_BYTES / 3 * 3)
    );
    // A little more is read but not written; far more is dropped by the
    // scanner, and what follows still prints.
    for extra in [10, 20] {
        let too_long = "QUFB".repeat(MAX_CLIPBOARD_BYTES / 3 + extra);
        emulator.feed(format!("\x1b]52;c;{too_long}\x07").as_bytes());
        assert!(emulator.take_events().is_empty(), "{extra}");
    }
    emulator.feed(b"ok");
    assert_eq!(line(&emulator.frame(), 0), "ok");
    // Another OSC string keeps the smaller bound.
    let title = "a".repeat(MAX_OSC_BYTES + 1);
    emulator.feed(format!("\x1b]0;{title}\x07").as_bytes());
    assert!(emulator.take_events().is_empty());
}

#[test]
fn synchronized_updates_wait_for_their_end() {
    let mut emulator = fed(5, 1, b"\x1b[?2026hheld");
    assert!(emulator.sync_pending());
    assert_eq!(line(&emulator.frame(), 0), "");
    emulator.end_sync();
    assert!(!emulator.sync_pending());
    assert_eq!(line(&emulator.frame(), 0), "held");
    let mut ended = fed(5, 1, b"\x1b[?2026hdone\x1b[?2026l");
    assert!(!ended.sync_pending());
    assert_eq!(line(&ended.frame(), 0), "done");
    ended.end_sync();
}

#[test]
fn modes_follow_the_program() {
    let modes = fed(
        5,
        1,
        b"\x1b[?1h\x1b=\x1b[?2004h\x1b[?1004h\x1b[?1003h\x1b[?1006h\x1b[20h\x1b[>5u",
    )
    .modes();
    assert!(modes.app_cursor && modes.app_keypad && modes.bracketed_paste);
    assert!(modes.focus_reporting && modes.line_feed_new_line);
    assert_eq!(modes.mouse, MouseReporting::Motion);
    assert_eq!(modes.mouse_encoding, MouseEncoding::Sgr);
    assert_eq!(modes.kitty_keyboard, 5);
    let modes = fed(5, 1, b"\x1b[?1000h\x1b[?1005h").modes();
    assert_eq!(
        (modes.mouse, modes.mouse_encoding),
        (MouseReporting::Click, MouseEncoding::Utf8)
    );
    assert_eq!(
        fed(5, 1, b"\x1b[?1002h").modes().mouse,
        MouseReporting::Drag
    );
    assert_eq!(emulator(5, 1).modes().mouse, MouseReporting::Off);
}
