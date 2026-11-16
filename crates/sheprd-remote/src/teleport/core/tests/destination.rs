use super::*;

fn calls(home: &tempfile::TempDir) -> Vec<String> {
    std::fs::read_to_string(home.path().join("calls"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn a_destination_core_starts_shells_and_types_their_programs() {
    let (home, host) = fake_core();
    let cancelled = AtomicBool::new(false);
    assert!(available(&host, &cancelled).unwrap());
    std::fs::write(home.path().join("broken"), "").unwrap();
    assert!(!available(&host, &cancelled).unwrap());
    let started = start_shell(&host, "/w/sub dir", &cancelled).unwrap();
    assert_eq!(started, "t5");
    type_line(&host, "t5", "npm run dev", &cancelled).unwrap();
    assert_eq!(
        calls(&home)[2..],
        [
            "run --bg --json --no-parent --cwd /w/sub dir",
            "send --json t5 npm run dev",
        ]
    );
}

#[test]
fn terminals_of_an_unfinished_move_end_and_kept_ones_stay() {
    let (home, host) = fake_core();
    let mut spawned = Spawned::new(&host);
    spawned.push("t5".into());
    spawned.push("t6".into());
    drop(spawned);
    assert_eq!(calls(&home), ["kill --json t5", "kill --json t6"]);
    let mut spawned = Spawned::new(&host);
    spawned.push("t7".into());
    spawned.keep();
    assert_eq!(calls(&home).len(), 2);
}

#[test]
fn split_trees_become_the_destinations_panes() {
    let mut panes = Panes::single(id("a"));
    panes.split(&id("a"), Axis::Row, id("b")).unwrap();
    panes.split(&id("b"), Axis::Column, id("c")).unwrap();
    panes.set_share(&[], Share::from(700));
    let renamed: HashMap<String, String> = [("a", "t4"), ("b", "t5"), ("c", "t6")]
        .into_iter()
        .map(|(from, to)| (from.to_owned(), to.to_owned()))
        .collect();
    let moved = super::super::panes(&tree(&panes), &renamed).unwrap();
    let ids: Vec<&str> = moved.terminals().iter().map(|id| id.as_str()).collect();
    assert_eq!(ids, ["t4", "t5", "t6"]);
    assert_eq!(tree(&moved), {
        let mut expected = tree(&panes);
        rename(&mut expected, &renamed);
        expected
    });
    // A pane with no terminal standing in for it cannot be laid out.
    assert!(super::super::panes(&Node::Pane("x".into()), &renamed).is_none());
}

fn rename(node: &mut Node, names: &HashMap<String, String>) {
    match node {
        Node::Pane(id) => *id = names[id.as_str()].clone(),
        Node::Split { first, second, .. } => {
            rename(first, names);
            rename(second, names);
        }
    }
}
