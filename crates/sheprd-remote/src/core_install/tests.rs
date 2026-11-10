use super::*;
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

/// A private folder holding the machine's home, a fake `curl` that serves
/// `archive`, and the archive itself: a tarball with a `herdr-core`.
struct Machine {
    root: PathBuf,
}

impl Machine {
    fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "sheprd-core-install-{}-{} ' x",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["home", "tools", "archive"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        let core = root.join("archive/herdr-core");
        std::fs::write(&core, "#!/bin/sh\necho core\n").unwrap();
        let status = Command::new("tar")
            .arg("-czf")
            .arg(root.join("core.tar.gz"))
            .arg("-C")
            .arg(root.join("archive"))
            .arg("herdr-core")
            .status()
            .unwrap();
        assert!(status.success());
        // `curl -fsSL URL -o FILE`: copy the archive to FILE.
        let curl = root.join("tools/curl");
        std::fs::write(
            &curl,
            format!(
                "#!/bin/sh\nwhile [ $# -gt 1 ]; do [ \"$1\" = -o ] && exec /bin/cp {} \"$2\"; shift; done\nexit 22\n",
                shell_quote(root.join("core.tar.gz").to_str().unwrap())
            ),
        )
        .unwrap();
        make_executable(&curl);
        Self { root }
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    fn pinned(&self, sha256: &str) -> Pinned {
        let build = Build {
            url: "https://releases.example/core.tar.gz".into(),
            sha256: sha256.into(),
        };
        Pinned {
            version: "0.0.1".into(),
            targets: PLATFORMS
                .iter()
                .map(|(_, _, target)| ((*target).to_owned(), build.clone()))
                .collect(),
        }
    }

    fn checksum(&self) -> String {
        let output = Command::new("/bin/sh")
            .arg("-c")
            .arg("(sha256sum \"$0\" 2>/dev/null || shasum -a 256 \"$0\") | cut -d' ' -f1")
            .arg(self.root.join("core.tar.gz"))
            .output()
            .unwrap();
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    /// The install run by `/bin/sh` here, as a provider's command runs it
    /// there, with only the fake curl and the system's tools on PATH.
    fn install(&self, pinned: &Pinned) -> Result<Outcome, Error> {
        // Only the fake home is searched: a core installed on the machine
        // running the tests must not be found.
        let script = render(pinned, Dir::UserBin).unwrap();
        assert!(script.contains(CANDIDATES));
        let script = script.replace(
            CANDIDATES,
            r#"for path in "$HOME/.local/bin/herdr-core"; do"#,
        );
        let mut command = Command::new("/bin/sh");
        command.arg("-c").env_clear().env("HOME", self.home()).env(
            "PATH",
            format!("{}:/usr/bin:/bin", self.root.join("tools").display()),
        );
        run(&script, Target::Command(command), &AtomicBool::new(false))
    }
}

impl Drop for Machine {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn the_pinned_release_has_a_checked_build_for_every_posix_machine() {
    let pinned = pinned().unwrap();
    assert!(!pinned.version.is_empty());
    for (_, _, target) in PLATFORMS {
        let build = &pinned.targets[*target];
        assert!(build.url.starts_with("https://"), "{target}");
        assert_eq!(build.sha256.len(), 64, "{target}");
    }
    let script = script(Dir::UserBin).unwrap();
    assert!(script.contains(&pinned.targets["linux-x86_64"].sha256));
}

#[test]
fn a_machine_without_a_core_gets_the_pinned_one_and_hc() {
    let machine = Machine::new();
    let pinned = machine.pinned(&machine.checksum());
    assert_eq!(machine.install(&pinned).unwrap(), Outcome::Installed);
    let bin = machine.home().join(".local/bin");
    let output = Command::new(bin.join("herdr-core")).output().unwrap();
    assert_eq!(output.stdout, b"core\n");
    assert_eq!(
        std::fs::read_link(bin.join("hc")).unwrap(),
        Path::new("herdr-core")
    );
    // Run again, it finds the core it installed and changes nothing.
    assert_eq!(machine.install(&pinned).unwrap(), Outcome::Present);
}

#[test]
fn a_users_own_core_is_never_replaced() {
    let machine = Machine::new();
    let bin = machine.home().join(".local/bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("herdr-core"), "#!/bin/sh\necho mine\n").unwrap();
    make_executable(&bin.join("herdr-core"));
    let pinned = machine.pinned(&machine.checksum());
    assert_eq!(machine.install(&pinned).unwrap(), Outcome::Present);
    assert_eq!(
        std::fs::read_to_string(bin.join("herdr-core")).unwrap(),
        "#!/bin/sh\necho mine\n"
    );
}

#[test]
fn an_existing_hc_is_left_alone() {
    let machine = Machine::new();
    let bin = machine.home().join(".local/bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("hc"), "other").unwrap();
    let pinned = machine.pinned(&machine.checksum());
    assert_eq!(machine.install(&pinned).unwrap(), Outcome::Installed);
    assert_eq!(std::fs::read_to_string(bin.join("hc")).unwrap(), "other");
}

#[test]
fn an_image_install_ignores_a_core_only_the_building_user_has() {
    let machine = Machine::new();
    let own = machine.home().join(".local/bin");
    std::fs::create_dir_all(&own).unwrap();
    std::fs::write(own.join("herdr-core"), "#!/bin/sh\n").unwrap();
    make_executable(&own.join("herdr-core"));
    let shared = machine.root.join("image-bin");
    let pinned = machine.pinned(&machine.checksum());
    let script = render(&pinned, Dir::At(shared.to_str().unwrap())).unwrap();
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .env_clear()
        .env("HOME", machine.home())
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", machine.root.join("tools").display()),
        );
    assert_eq!(
        run(&script, Target::Command(command), &AtomicBool::new(false)).unwrap(),
        Outcome::Installed
    );
    assert!(shared.join("herdr-core").exists());
}

#[test]
fn a_download_that_does_not_match_its_checksum_installs_nothing() {
    let machine = Machine::new();
    let pinned = machine.pinned(&"0".repeat(64));
    assert!(matches!(machine.install(&pinned), Err(Error::Checksum)));
    assert!(!machine.home().join(".local/bin/herdr-core").exists());
}

#[test]
fn a_machine_without_curl_says_so() {
    let machine = Machine::new();
    std::fs::remove_file(machine.root.join("tools/curl")).unwrap();
    let pinned = machine.pinned(&machine.checksum());
    let script = render(&pinned, Dir::UserBin).unwrap();
    let mut command = Command::new("/bin/sh");
    // No folder with a curl on PATH, and the tools the check needs by path.
    command
        .arg("-c")
        .env_clear()
        .env("HOME", machine.home())
        .env("PATH", machine.root.join("tools"));
    assert!(matches!(
        run(&script, Target::Command(command), &AtomicBool::new(false)),
        Err(Error::NoCurl)
    ));
}

#[test]
fn an_install_folder_must_be_absolute() {
    let pinned = pinned().unwrap();
    assert!(matches!(render(&pinned, Dir::At("bin")), Err(Error::Dir)));
    assert!(
        render(&pinned, Dir::At("/usr/local/bin"))
            .unwrap()
            .contains("dir='/usr/local/bin'")
    );
}

#[test]
fn output_without_the_final_line_is_unfinished() {
    assert_eq!(
        outcome(b"noise\nherdr-core-install:installed\n").unwrap(),
        Outcome::Installed
    );
    assert!(matches!(
        outcome(b"herdr-core-install:installed\nmore"),
        Err(Error::Unfinished)
    ));
}

/// The real pinned build, downloaded for this machine. Opt in with
/// `cargo test -p sheprd-remote core_install -- --ignored`; it needs the network.
#[test]
#[ignore = "downloads the pinned herdr-core"]
fn the_real_pinned_build_installs_and_runs() {
    let machine = Machine::new();
    let script = script(Dir::UserBin).unwrap();
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .env_clear()
        .env("HOME", machine.home())
        .env("PATH", "/usr/bin:/bin");
    assert_eq!(
        run(&script, Target::Command(command), &AtomicBool::new(false)).unwrap(),
        Outcome::Installed
    );
    let output = Command::new(machine.home().join(".local/bin/herdr-core"))
        .arg("--version")
        .output()
        .unwrap();
    let version = String::from_utf8(output.stdout).unwrap();
    assert!(version.contains(&pinned().unwrap().version), "{version}");
}
