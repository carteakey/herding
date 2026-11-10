use super::*;

#[test]
fn the_installed_launcher_prefers_herding_and_accepts_older_guis() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("herdr-gpui.exe");
    let middle = dir.path().join("sheprd.exe");
    let current = dir.path().join("herding.exe");
    std::fs::write(&old, b"old").unwrap();
    for launcher in ["sheprd-launcher.exe", "herdr-gpui-launcher.exe"] {
        assert_eq!(gui_path(&dir.path().join(launcher)).unwrap(), old);
    }
    std::fs::write(&middle, b"middle").unwrap();
    for launcher in ["sheprd-launcher.exe", "herdr-gpui-launcher.exe"] {
        assert_eq!(gui_path(&dir.path().join(launcher)).unwrap(), middle);
    }
    std::fs::write(&current, b"current").unwrap();
    for launcher in ["sheprd-launcher.exe", "herdr-gpui-launcher.exe"] {
        assert_eq!(gui_path(&dir.path().join(launcher)).unwrap(), current);
    }
}
