//! A Herdr 2 tab's mark shows its own host's agents: terminal ids are only
//! unique within one host.

use crate::{
    browser::Scope,
    core_terminals::{CoreTerminals, Host, panes::Panes},
};
use sheprd_backend::{RunState, TerminalId, TerminalInfo};
use sheprd_client::protocol::AgentStatus;

fn waiting(name: &str) -> TerminalInfo {
    TerminalInfo {
        agent: AgentStatus::Blocked,
        cwd: None,
        foreground: None,
        id: TerminalId::new(name).unwrap(),
        labels: Default::default(),
        name: None,
        reporter: None,
        state: RunState::Running,
        title: None,
    }
}

#[test]
fn a_remote_tab_never_shows_a_local_terminals_status() {
    let mut terminals = CoreTerminals::default();
    terminals.infos.list_for_test(vec![waiting("t1")]);
    let panes = Panes::single(TerminalId::new("t1").unwrap());
    assert_eq!(
        terminals.tab_status(&Host::Local, &panes),
        AgentStatus::Blocked
    );
    let remote = Host::Remote(Scope::endpoint("ssh:box"));
    assert_eq!(
        terminals.tab_status(&remote, &panes),
        AgentStatus::Unknown,
        "the same id on another host is another terminal"
    );
}
