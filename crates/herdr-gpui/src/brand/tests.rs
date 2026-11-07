use super::*;

#[test]
fn tokens_flip_on_background_luminance() {
    let dark = Theme::default(); // background 0x101419
    assert!(is_dark(&dark));
    assert_eq!(Brand::accent(&dark), ACCENT_DARK);
    assert_eq!(Brand::running(&dark), STATUS_RUNNING_DARK);

    // A light background: near-white with near-black foreground text.
    let light = Theme {
        background: 0xF5F2EE,
        foreground: 0x1A1A1A,
        ..Theme::default()
    };
    assert!(!is_dark(&light));
    assert_eq!(Brand::accent(&light), ACCENT_LIGHT);
    assert_eq!(Brand::blocked(&light), STATUS_BLOCKED_LIGHT);
}
