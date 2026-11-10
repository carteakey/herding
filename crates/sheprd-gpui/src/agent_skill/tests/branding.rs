use super::*;

#[test]
fn installed_skill_identity_survives_rebranding() {
    for product in ["Herdr UI", "Herding"] {
        assert!(managed(&format!(
            "<!-- herdr-gpui-managed-skill v1: {product} -->"
        )));
    }
    assert_eq!(NAME, "herdr-gpui-browser");
    assert_eq!(AUTOMATIONS_NAME, "herdr-automations");
    assert_eq!(MODS_NAME, "herdr-mods");
    assert_eq!(command(None), "herding");
}

#[test]
fn commands_are_substituted_once_even_when_the_install_path_ends_in_the_command() {
    assert_eq!(
        substitute_command(
            "herding open .\nherdr-gpui open .",
            "herding",
            "herdr-gpui",
            "/opt/bin/herding"
        ),
        "/opt/bin/herding open .\n/opt/bin/herding open ."
    );
}
