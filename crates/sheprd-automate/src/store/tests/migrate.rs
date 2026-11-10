use super::*;
use sheprd_automation::report::Terminals;

/// The tables an older daemon wrote that this change touches.
const SCHEMA_1: &str = "
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
INSERT INTO meta (key, value) VALUES ('schema', '1');
CREATE TABLE steps (
    run INTEGER NOT NULL, position INTEGER NOT NULL, kind TEXT NOT NULL, summary TEXT NOT NULL,
    started_at INTEGER NOT NULL, finished_at INTEGER, state TEXT NOT NULL, exit INTEGER,
    log_tail TEXT NOT NULL DEFAULT '', worktree TEXT, pane TEXT,
    cpu_ms INTEGER, peak_memory INTEGER, PRIMARY KEY (run, position));
CREATE TABLE agents (
    run INTEGER NOT NULL, position INTEGER NOT NULL, path TEXT NOT NULL, harness TEXT NOT NULL,
    pane TEXT NOT NULL, cwd TEXT NOT NULL, started_at INTEGER NOT NULL, done_at INTEGER,
    input INTEGER, output INTEGER, cache_read INTEGER, cache_write INTEGER,
    cpu_ms INTEGER, peak_memory INTEGER, sampled_at INTEGER, PRIMARY KEY (run, position));
INSERT INTO agents (run, position, path, harness, pane, cwd, started_at)
    VALUES (1, 1, '/a.luau', 'claude', 'w1p1', '/tmp', 5);
";

#[test]
fn an_older_database_keeps_its_agents_as_herdr_1_ones() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    Connection::open(&path)
        .unwrap()
        .execute_batch(SCHEMA_1)
        .unwrap();
    let store = Store::open(&path).unwrap();
    let agents = store.agents(false).unwrap();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].terminals, Terminals::Herdr);
    // Opening again finds it current.
    drop(store);
    Store::open(&path).unwrap();
}

#[test]
fn a_herdr_core_agent_reads_back_as_one() {
    let (_dir, store) = store();
    store
        .add_agent(&AgentRow {
            run: 1,
            position: 2,
            path: "/a.luau".to_owned(),
            harness: "claude".to_owned(),
            pane: "t7".to_owned(),
            cwd: "/tmp".to_owned(),
            started_at: 5,
            terminals: Terminals::HerdrCore,
            ..AgentRow::default()
        })
        .unwrap();
    assert_eq!(
        store.agents(false).unwrap()[0].terminals,
        Terminals::HerdrCore
    );
}

#[test]
fn a_database_another_process_already_upgraded_is_left_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    Connection::open(&path)
        .unwrap()
        .execute_batch(SCHEMA_1)
        .unwrap();
    // Both read version 1 before either took the lock.
    let first = Connection::open(&path).unwrap();
    let second = Connection::open(&path).unwrap();
    migrate(&first).unwrap();
    migrate(&second).unwrap();
    drop((first, second));
    assert_eq!(Store::open(&path).unwrap().agents(false).unwrap().len(), 1);
}
