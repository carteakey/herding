use super::*;

#[test]
fn folders_are_asked_once_then_again_when_stale() {
    let mut checkouts = Checkouts::default();
    let now = Instant::now();
    let folder = std::env::temp_dir().display().to_string();
    checkouts.poll(vec![folder.clone()], now);
    let asking = checkouts.asking.take().unwrap();
    let answers = asking.recv_timeout(Duration::from_secs(30)).unwrap();
    assert_eq!(answers.len(), 1);
    // A finished batch lands on the next poll.
    let (send, receive) = mpsc::sync_channel(1);
    send.send(vec![(folder.clone(), None)]).unwrap();
    checkouts.asking = Some(receive);
    assert!(checkouts.poll(vec![folder.clone()], now));
    assert!(
        checkouts.asking.is_none(),
        "fresh answers are not asked again"
    );
    assert!(checkouts.get(&folder).is_none(), "not a checkout");
    checkouts.poll(vec![folder.clone()], now + REFRESH);
    assert!(checkouts.asking.is_some(), "stale answers are");
}

#[test]
fn a_folder_no_longer_listed_is_forgotten() {
    let mut checkouts = Checkouts::default();
    let identity = Identity {
        branch: Some("main".into()),
        key: "/src/app/.git".into(),
        label: "app".into(),
        linked: false,
    };
    checkouts.seed("/src/app", Some(identity));
    assert_eq!(checkouts.get("/src/app").unwrap().label, "app");
    assert!(checkouts.poll(Vec::new(), Instant::now()));
    assert!(checkouts.get("/src/app").is_none());
}
