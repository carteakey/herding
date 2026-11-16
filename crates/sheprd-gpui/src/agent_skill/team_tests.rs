#![allow(clippy::unwrap_used)]

use super::*;

#[test]
fn team_skill_names_the_app_command() {
    assert!(TEAM.starts_with("---\nname: herdr-team\n"));
    assert!(managed(TEAM));
    assert_eq!(team_text(None), TEAM);
    let text = team_text(Some(Path::new("/some path/Herdr")));
    assert!(text.contains("'/some path/Herdr' team status --json"));
    assert!(!text.contains("herdr-gpui team "));
}

#[test]
fn team_skill_install_refresh_and_removal_preserve_user_files() {
    let home = tempfile::tempdir().unwrap();
    let agents = home.path().join(".agents");
    let claude = home.path().join(".claude");
    std::fs::create_dir_all(&agents).unwrap();
    std::fs::create_dir_all(&claude).unwrap();
    let managed_path = file(&agents, TEAM_NAME);
    let user_path = file(&claude, TEAM_NAME);
    std::fs::create_dir_all(user_path.parent().unwrap()).unwrap();
    std::fs::write(&user_path, "personal team skill").unwrap();

    install(home.path(), SKILL).unwrap();
    let first = std::fs::read_to_string(&managed_path).unwrap();
    assert!(managed(&first));
    assert_eq!(
        std::fs::read_to_string(&user_path).unwrap(),
        "personal team skill"
    );

    std::fs::write(&managed_path, format!("{MARKER} -->\nold skill")).unwrap();
    install(home.path(), SKILL).unwrap();
    assert_eq!(std::fs::read_to_string(&managed_path).unwrap(), first);

    let removed = remove(home.path()).unwrap();
    assert!(removed.contains(&managed_path));
    assert!(!managed_path.exists());
    assert_eq!(
        std::fs::read_to_string(&user_path).unwrap(),
        "personal team skill"
    );
}

#[test]
fn team_skill_has_the_supervisor_playbook_and_worker_report_rule() {
    assert!(TEAM.contains("## Supervisor playbook"));
    for rule in [
        "**Tiered review.**",
        "**Structured completion report.**",
        "**Delegate diagnosis.**",
        "**Do not poll panes.**",
        "**CI is the gate.**",
        "a `skipping` job",
    ] {
        assert!(TEAM.contains(rule), "missing supervisor rule: {rule}");
    }
    assert!(TEAM.contains("## Worker rule: always post the structured report"));
}
