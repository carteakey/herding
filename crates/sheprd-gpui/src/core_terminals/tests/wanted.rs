//! Terminals waiting for their host's core to connect, each host's apart.

use super::super::{CoreTerminals, Host, Purpose};
use crate::browser::Scope;
use sheprd_backend::Spawn;

fn tab(scope: &Scope) -> Purpose {
    Purpose::Tab {
        scope: scope.clone(),
        workspace: "w1".into(),
    }
}

#[test]
fn forgetting_a_host_keeps_the_terminals_waiting_for_others() {
    let local = Scope::local("/x.sock".as_ref());
    let remote = Scope::endpoint("ssh:box");
    let mut terminals = CoreTerminals::default();
    for scope in [&local, &remote, &local] {
        terminals
            .wanted
            .push_back((tab(scope), Host::of(scope), Spawn::default(), None));
    }

    terminals.forget_host(&Host::Remote(remote));

    let hosts: Vec<Host> = terminals
        .wanted
        .iter()
        .map(|(_, host, ..)| host.clone())
        .collect();
    assert_eq!(hosts, vec![Host::Local, Host::Local]);
}
