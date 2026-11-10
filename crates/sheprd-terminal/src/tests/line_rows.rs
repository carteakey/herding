//! Lines and the rows they wrap over, read across history and the screen.

use super::*;

#[test]
fn a_wrapped_line_counts_its_rows_through_history_and_screen() {
    let mut emulator = Emulator::new(Size::new(4, 3).unwrap(), 100);
    emulator.feed(b"1\r\nabcdefghij\r\n2\r\n3");
    // `abcdefghij` takes three rows, and the screen's rows are all lines.
    assert_eq!(emulator.line_rows(), [1, 3, 1, 1]);
    emulator.resize(Size::new(10, 3).unwrap());
    assert_eq!(emulator.line_rows(), [1, 1, 1, 1]);
}
