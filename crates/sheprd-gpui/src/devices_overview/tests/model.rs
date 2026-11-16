use crate::devices_overview::{Device, Link, Tally, chart::padded, history::STEPS};
use sheprd_client::protocol::AgentStatus::{self, Blocked, Done, Idle, Unknown, Working};

#[test]
fn a_tally_counts_unknown_agents_as_idle() {
    let tally = Tally::of([Working, Working, Blocked, Done, Idle, Unknown]);
    assert_eq!(
        tally,
        Tally {
            working: 2,
            blocked: 1,
            done: 1,
            idle: 2,
        }
    );
    assert_eq!(tally.total(), 6);
    let counts = tally.counts();
    assert_eq!((counts.working, counts.blocked), (2, 1));
}

#[test]
fn devices_sum_into_the_totals() {
    let devices = [
        Device::for_test("mac", Link::Online, &[Working, Idle]),
        Device::for_test("studio", Link::Online, &[Working, Blocked, Done]),
    ];
    assert_eq!(
        Tally::sum(&devices),
        Tally {
            working: 2,
            blocked: 1,
            done: 1,
            idle: 1,
        }
    );
}

#[test]
fn dots_put_working_agents_first_and_stop_at_eight() {
    let statuses: Vec<AgentStatus> = [Idle, Blocked, Working, Done]
        .into_iter()
        .cycle()
        .take(12)
        .collect();
    let device = Device::for_test("studio", Link::Online, &statuses);
    assert_eq!(device.dots.len(), 8);
    assert_eq!(&device.dots[..3], &[Working, Working, Working]);
    assert_eq!(device.tally.total(), 12, "the count still says them all");
}

#[test]
fn search_matches_every_word_against_device_agent_and_workspace() {
    let device = Device::for_test("mac-studio", Link::Online, &[Working]);
    assert!(device.matches(""));
    assert!(device.matches("  "));
    assert!(device.matches("STUDIO"));
    assert!(device.matches("ssh"), "how it is reached");
    assert!(device.matches("claude gpui"), "agent and workspace");
    assert!(!device.matches("claude laptop"));
}

#[test]
fn the_chart_axis_always_spans_two_hours() {
    let short = padded(vec![1, 2, 3]);
    assert_eq!(short.len(), STEPS + 1);
    assert!(short[..STEPS - 2].iter().all(Option::is_none));
    assert_eq!(short[STEPS - 2..], [Some(1), Some(2), Some(3)]);
    let long = padded((0..STEPS + 5).collect());
    assert_eq!(long.len(), STEPS + 1);
    assert_eq!(long.last(), Some(&Some(STEPS + 4)), "the newest steps stay");
}

#[test]
fn only_a_reachable_device_is_counted() {
    use crate::{devices_overview::online_tally, state::ConnectionStatus};
    let tally = |endpoint: &crate::endpoint::Endpoint, live: &crate::state::LiveState| {
        let sessions = crate::session_model::Sessions {
            core: None,
            core_answers: false,
            herdr_1: live.session(),
        };
        online_tally(endpoint, live, &sessions)
    };
    let endpoint = |enabled| {
        crate::endpoint::Endpoint::new(
            "ssh:box".into(),
            "box".into(),
            sheprd_client::ConnectTarget::Ssh {
                target: "box".into(),
                session: "default".into(),
            },
            enabled,
        )
    };
    let mut live = crate::state::LiveState::default();
    live.snapshot = Some(std::sync::Arc::new(crate::sidebar::layout_tests::snapshot(
        1,
    )));
    assert_eq!(tally(&endpoint(true), &live), None, "not connected");
    live.status = ConnectionStatus::Connected;
    let counted = tally(&endpoint(true), &live).unwrap();
    assert_eq!(
        counted.total(),
        live.snapshot.as_ref().unwrap().agents.len()
    );
    assert_eq!(tally(&endpoint(false), &live), None, "disabled");
    live.snapshot = None;
    assert_eq!(tally(&endpoint(true), &live), None, "no snapshot yet");
}

#[test]
fn every_column_names_an_english_message() {
    for (id, _) in crate::devices_overview::table::COLUMNS {
        assert!(id.is_empty() || crate::i18n::takes(id) == Some(&[]), "{id}");
    }
}

#[test]
fn herdr_2_agents_count_on_a_computer_without_herdr_1() {
    use crate::devices_overview::online_tally;
    use sheprd_backend::{RunState, TerminalId, TerminalInfo};
    use sheprd_session::{CoreTab, CoreWorkspace, Session, panes::Panes};
    let endpoint = crate::endpoint::Endpoint::new(
        "local".into(),
        "This Mac".into(),
        sheprd_client::ConnectTarget::Local,
        true,
    );
    let mut live = crate::state::LiveState::default();
    live.missing_installation = true;
    let terminal = TerminalId::new("t1").unwrap();
    let info = TerminalInfo {
        agent: Blocked,
        cwd: None,
        foreground: None,
        id: terminal.clone(),
        labels: Default::default(),
        name: Some("claude".into()),
        reporter: None,
        state: RunState::Running,
        title: None,
    };
    let panes = Panes::single(terminal);
    let core = Session::core(
        [CoreWorkspace {
            branch: None,
            custom_label: false,
            worktree: None,
            cwd: "",
            id: "herdr-2",
            label: "Herdr 2",
        }],
        [CoreTab {
            id: 1,
            panes: &panes,
            title: "",
            workspace_id: "herdr-2",
        }],
        None,
        |_| Some(&info),
    );
    let sessions = crate::session_model::Sessions {
        core: Some(std::sync::Arc::new(core)),
        core_answers: true,
        herdr_1: None,
    };
    assert_eq!(
        online_tally(&endpoint, &live, &sessions).unwrap().blocked,
        1
    );
    let device = Device::new(0, &endpoint, &live, &sessions);
    assert_eq!(device.link, Link::Online);
    assert_eq!(device.agents[0].workspace, "Herdr 2");
    // A click shows it through the app's tab, not Herdr 1's pane focus.
    assert_eq!(device.agents[0].keeper, sheprd_session::Keeper::App);
    assert_eq!(device.agents[0].tab_id, "1");
    assert!(device.matches("claude"));
    let offline = Device::new(0, &endpoint, &live, &Default::default());
    assert_ne!(offline.link, Link::Online);
    assert!(offline.agents.is_empty());
}
