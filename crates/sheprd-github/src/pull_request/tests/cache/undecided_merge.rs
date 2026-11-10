use super::*;
use crate::pull_request::{PullRequest, Tone};

/// An open pull request whose checks all finished, with GitHub's merge state.
fn settled_checks(merge: MergeState) -> PullRequest {
    let mut pr = fixture().unwrap();
    pr.state = State::Open;
    pr.review_decision = crate::pull_request::ReviewDecision::None;
    pr.merge_state_status = merge;
    pr.status_check_rollup
        .as_mut()
        .unwrap()
        .retain(|check| check.outcome() == Outcome::Passed);
    pr
}

/// The badge `input`'s row would draw from the cache.
fn tone(peer: &Peer, input: &Input) -> Option<Tone> {
    peer.cache.get(input).map(|pr| pr.tone())
}

#[test]
fn an_undecided_merge_state_is_asked_again_as_soon_as_running_checks_are() {
    let mut peer = Peer::new();
    let now = Instant::now();
    let branch = input("conflicted");
    // GitHub answers UNKNOWN until it has worked the merge out, and again
    // after every push to the base.
    peer.cache.schedule([branch.clone()], now);
    peer.cache.poll(now);
    peer.complete(now, Ok(Some(settled_checks(MergeState::Unknown))), None);
    assert_eq!(tone(&peer, &branch), Some(Tone::Waiting));

    // Asked again when running checks would be, not after the quiet wait.
    let reread = now + REFRESH;
    peer.cache
        .schedule([branch.clone()], reread - Duration::from_secs(1));
    assert!(peer.cache.queue.is_empty(), "not before its time");
    peer.cache.schedule([branch.clone()], reread);
    assert_eq!(peer.cache.queue, std::slice::from_ref(&branch));
    peer.cache.poll(reread);
    peer.complete(reread, Ok(Some(settled_checks(MergeState::Dirty))), None);
    assert_eq!(tone(&peer, &branch), Some(Tone::Attention), "conflicts");

    // Decided, it goes back to the quiet wait.
    let entry_due = |peer: &Peer| {
        let store = peer.cache.store.borrow();
        store.entry(&local(), &branch).unwrap().due
    };
    assert_eq!(entry_due(&peer), reread + QUIET_REFRESH);
}

#[test]
fn only_an_open_pull_request_rereads_an_undecided_merge_state() {
    let mut peer = Peer::new();
    let now = Instant::now();
    for (branch, state, wait) in [
        ("open", State::Open, REFRESH),
        ("merged", State::Merged, SETTLED_REFRESH),
        ("closed", State::Closed, SETTLED_REFRESH),
    ] {
        let branch = input(branch);
        let mut pr = settled_checks(MergeState::Unknown);
        pr.state = state;
        peer.cache.schedule([branch.clone()], now);
        peer.cache.poll(now);
        peer.complete(now, Ok(Some(pr)), None);
        let store = peer.cache.store.borrow();
        assert_eq!(store.entry(&local(), &branch).unwrap().due - now, wait);
    }
}
