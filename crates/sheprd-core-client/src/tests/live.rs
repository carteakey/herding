//! Against a real herdr-core in a private home and socket. Opt in with
//! `HERDR_CORE_TEST_BINARY=/path/to/herdr-core cargo test -p sheprd-core-client -- --ignored`.

use crate::{
    Core, Endpoint, Error,
    event::Notice,
    wire::{Attach, Run},
};
use std::{
    ffi::OsString,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

pub(super) struct Sandbox {
    _dir: tempfile::TempDir,
    pub(super) binary: OsString,
    child: Child,
    pub(super) vars: Vec<(&'static str, OsString)>,
}

impl Sandbox {
    /// The private folder holding the core's home and socket.
    #[cfg(unix)]
    pub(super) fn dir(&self) -> &Path {
        self._dir.path()
    }

    pub(super) fn start(binary: OsString) -> Self {
        let parent = std::env::var_os("HERDR_TEST_TMPDIR").unwrap_or_else(|| "/tmp".into());
        let dir = tempfile::Builder::new()
            .prefix("hc")
            .tempdir_in(parent)
            .unwrap();
        let at = |name: &str| dir.path().join(name).into_os_string();
        let vars = vec![
            ("HC_SESSION", OsString::from("live")),
            ("HC_SOCKET", at("c.sock")),
            ("HOME", at("home")),
            ("PATH", std::env::var_os("PATH").unwrap_or_default()),
            ("XDG_CONFIG_HOME", at("config")),
            ("XDG_STATE_HOME", at("state")),
        ];
        let child = Self::command(&binary, &vars)
            .arg("serve")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        Self {
            _dir: dir,
            binary,
            child,
            vars,
        }
    }

    fn command(binary: &OsString, vars: &[(&'static str, OsString)]) -> Command {
        let mut command = Command::new(binary);
        command
            .env_clear()
            .envs(vars.iter().map(|(name, value)| (name, value)));
        command
    }

    fn var(&self, name: &str) -> Option<OsString> {
        self.vars
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.clone())
    }

    fn connect(&self) -> Core {
        let endpoint = Endpoint::resolve(|name| self.var(name)).unwrap();
        assert_eq!(endpoint.socket, Path::new(&self.var("HC_SOCKET").unwrap()));
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match Core::connect(&endpoint) {
                Ok(core) => return core,
                Err(Error::Connect { .. }) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(50))
                }
                Err(error) => panic!("{error}"),
            }
        }
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        // Ends this sandbox's own core and the programs it holds, through its
        // private socket, then reaps the exact child this test started.
        let _ = Self::command(&self.binary, &self.vars)
            .args(["server", "stop"])
            .output();
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.child.try_wait().ok().flatten().is_none() {
            if Instant::now() >= deadline {
                let _ = self.child.kill();
                let _ = self.child.wait();
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

#[test]
#[ignore = "needs HERDR_CORE_TEST_BINARY"]
fn a_real_core_runs_streams_and_stores() {
    let binary = std::env::var_os("HERDR_CORE_TEST_BINARY").expect("HERDR_CORE_TEST_BINARY");
    let sandbox = Sandbox::start(binary);
    let core = sandbox.connect();
    assert_eq!(core.hello().generation, 1);

    let argv = [
        "sh".to_owned(),
        "-c".to_owned(),
        "printf hi-core; sleep 30".to_owned(),
    ];
    let started = core
        .run(&Run {
            argv: &argv,
            name: Some("live"),
            ..Run::default()
        })
        .unwrap();
    let subscription = core.subscribe().unwrap();
    assert!(
        subscription
            .terminals
            .iter()
            .any(|terminal| terminal.address == started.address)
    );

    let attachment = core
        .attach(&Attach {
            cols: 80,
            rows: 24,
            terminal: &started.address,
            ..Attach::default()
        })
        .unwrap();
    let mut seen = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !seen.windows(7).any(|window| window == b"hi-core") {
        assert!(
            Instant::now() < deadline,
            "no output: {:?}",
            String::from_utf8_lossy(&seen)
        );
        let chunk = attachment.read(Duration::from_millis(100)).unwrap();
        assert_eq!(chunk.end, None);
        seen.extend(chunk.bytes);
    }

    let mut events = core.subscribe().unwrap();
    let doc = core.doc_get("herdr-ui-live").unwrap();
    assert_eq!(doc.version, 0);
    let version = core
        .doc_set("herdr-ui-live", &serde_json::json!({"tabs": 1}), 0)
        .unwrap();
    assert_eq!(
        core.doc_get("herdr-ui-live").unwrap().value,
        serde_json::json!({"tabs": 1})
    );
    assert!(matches!(
        core.doc_set("herdr-ui-live", &serde_json::json!(2), 0),
        Err(Error::Refused { .. })
    ));
    assert!(version > 0);
    let deadline = Instant::now() + Duration::from_secs(5);
    let changed = loop {
        assert!(Instant::now() < deadline, "no doc event arrived");
        let notices = events.next(Duration::from_millis(100)).unwrap();
        if let Some(notice) = notices
            .into_iter()
            .find(|notice| matches!(notice, Notice::Doc { .. }))
        {
            break notice;
        }
    };
    assert!(matches!(
        changed,
        Notice::Doc { machine: None, name, version: seen, .. }
            if name == "herdr-ui-live" && seen == version
    ));

    core.kill(&started.address).unwrap();
}

#[test]
#[ignore = "needs HERDR_CORE_TEST_BINARY"]
fn a_real_core_through_the_backend_traits() {
    use sheprd_backend::{
        Backend, ClientPaneInputEvent, Role, Size, Spawn, SurfaceUpdate, Terminal as _,
    };

    let binary = std::env::var_os("HERDR_CORE_TEST_BINARY").expect("HERDR_CORE_TEST_BINARY");
    let sandbox = Sandbox::start(binary);
    let backend = crate::CoreBackend::new(sandbox.connect(), 1000);
    // A chat's terminal carries its endpoint as a label every client lists.
    let spawn = Spawn {
        argv: vec!["sh".into()],
        labels: [("acp".to_owned(), "ab".repeat(16))].into(),
        name: Some("traits".into()),
        ..Spawn::default()
    };
    let info = backend.spawn(&spawn).unwrap();
    assert_eq!(info.labels, spawn.labels);
    assert!(
        backend
            .terminals()
            .unwrap()
            .iter()
            .any(|terminal| terminal.id == info.id && terminal.labels == spawn.labels)
    );

    let mut terminal = backend
        .attach(&info.id, Size::new(40, 6).unwrap(), Role::Active)
        .unwrap();
    terminal
        .input(&ClientPaneInputEvent::TextCommit("echo $((6*7))".into()))
        .unwrap();
    terminal
        .input(&ClientPaneInputEvent::Key {
            code: sheprd_protocol::ClientKeyCode::Enter,
            modifiers: 0,
            kind: sheprd_protocol::ClientKeyKind::Press,
            repeat_count: 1,
            shifted_codepoint: None,
            generated_text: None,
            tracks_release: false,
            physical_key_id: None,
            windows_record: None,
        })
        .unwrap();

    // Apply frames and row patches to a local copy, as the app's painter would.
    let mut screen: Vec<sheprd_protocol::CellData> = Vec::new();
    let mut width = 0usize;
    let text = |screen: &[sheprd_protocol::CellData]| {
        screen
            .iter()
            .map(|cell| cell.symbol.as_str())
            .collect::<String>()
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while !text(&screen).contains("42") {
        assert!(
            Instant::now() < deadline,
            "no answer on screen: {:?}",
            text(&screen)
        );
        for update in terminal.next(Duration::from_millis(100)).unwrap() {
            match update {
                SurfaceUpdate::Frame(frame) => {
                    width = frame.width.into();
                    screen = frame.cells;
                }
                SurfaceUpdate::Rows { rows, .. } => {
                    for row in rows {
                        let start = usize::from(row.y) * width + usize::from(row.x);
                        for (offset, cell) in row.cells.into_iter().enumerate() {
                            if let Some(slot) = screen.get_mut(start + offset) {
                                *slot = cell;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    backend.kill(&info.id).unwrap();
}

#[test]
#[ignore = "needs HERDR_CORE_TEST_BINARY"]
fn an_image_a_program_sends_reaches_the_view() {
    use sheprd_backend::{
        Backend, ClientKeyCode, ClientKeyKind, ClientPaneInputEvent, Role, Size, Spawn,
        SurfaceUpdate, Terminal as _,
    };

    // A 1x1 PNG.
    const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
    let binary = std::env::var_os("HERDR_CORE_TEST_BINARY").expect("HERDR_CORE_TEST_BINARY");
    let sandbox = Sandbox::start(binary);
    let backend = crate::CoreBackend::new(sandbox.connect(), 1000);
    let info = backend
        .spawn(&Spawn {
            argv: vec!["sh".into()],
            ..Spawn::default()
        })
        .unwrap();
    let mut terminal = backend
        .attach(&info.id, Size::new(40, 8).unwrap(), Role::Active)
        .unwrap();
    terminal.cell_pixels(10, 20).unwrap();
    let line = format!("printf '\\033_Ga=T,f=100,c=3,r=2;{PNG}\\033\\\\'");
    terminal
        .input(&ClientPaneInputEvent::TextCommit(line))
        .unwrap();
    terminal
        .input(&ClientPaneInputEvent::Key {
            code: ClientKeyCode::Enter,
            modifiers: 0,
            kind: ClientKeyKind::Press,
            repeat_count: 1,
            shifted_codepoint: None,
            generated_text: None,
            tracks_release: false,
            physical_key_id: None,
            windows_record: None,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let scene = loop {
        assert!(Instant::now() < deadline, "no image arrived");
        let scene = terminal
            .next(Duration::from_millis(100))
            .unwrap()
            .into_iter()
            .find_map(|update| match update {
                SurfaceUpdate::Graphics(scene) => Some(scene),
                _ => None,
            });
        if let Some(scene) = scene {
            break scene;
        }
    };
    let [placement] = &scene.placements[..] else {
        panic!("{scene:?}")
    };
    assert_eq!((placement.cols, placement.rows), (3, 2));
    assert_eq!(
        (placement.asset.image_width, placement.asset.image_height),
        (1, 1)
    );
    assert_eq!(scene.assets.len(), 1);
    backend.kill(&info.id).unwrap();
}

#[test]
#[ignore = "needs HERDR_CORE_TEST_BINARY"]
fn a_terminal_attached_anew_scrolls_back_through_what_it_missed() {
    use sheprd_backend::{
        Backend, ClientMouseKind, ClientMousePosition, ClientPaneInputEvent, Role, Size, Spawn,
        SurfaceUpdate, Terminal as _,
    };

    let binary = std::env::var_os("HERDR_CORE_TEST_BINARY").expect("HERDR_CORE_TEST_BINARY");
    let sandbox = Sandbox::start(binary);
    let backend = crate::CoreBackend::new(sandbox.connect(), 1000);
    let info = backend
        .spawn(&Spawn {
            argv: vec!["sh".into(), "-c".into(), "seq 1 300; exec sleep 600".into()],
            ..Spawn::default()
        })
        .unwrap();
    let size = Size::new(20, 6).unwrap();
    // The whole screen as text after each update, until `done` holds.
    let read_until = |terminal: &mut crate::CoreTerminal, done: &dyn Fn(&str) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut text = String::new();
        while !done(&text) {
            assert!(Instant::now() < deadline, "never shown: {text:?}");
            for update in terminal.next(Duration::from_millis(100)).unwrap() {
                if let SurfaceUpdate::Frame(frame) = update {
                    text = frame
                        .cells
                        .iter()
                        .map(|cell| cell.symbol.as_str())
                        .collect();
                }
            }
        }
    };
    // The output ran with no one attached; a view attached now gets the
    // history the core kept, which the wheel scrolls back through.
    let wait = Instant::now() + Duration::from_secs(10);
    while backend.terminals().unwrap().iter().all(|t| t.id != info.id) {
        assert!(Instant::now() < wait);
    }
    std::thread::sleep(Duration::from_millis(500));
    let mut terminal = backend.attach(&info.id, size, Role::Active).unwrap();
    read_until(&mut terminal, &|text| text.contains("300"));
    terminal
        .input(&ClientPaneInputEvent::Mouse {
            kind: ClientMouseKind::ScrollUp,
            position: ClientMousePosition::Cell { column: 0, row: 0 },
            geometry: None,
            modifiers: 0,
            lines: 100,
        })
        .unwrap();
    read_until(&mut terminal, &|text| text.contains("200"));
    backend.kill(&info.id).unwrap();
}
