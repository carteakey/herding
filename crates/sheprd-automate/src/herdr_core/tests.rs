use super::*;
use crate::testing::Fixture;

#[test]
fn a_known_harness_starts_its_own_program_and_another_runs_as_named() {
    assert_eq!(argv("claude"), ["claude"]);
    assert_eq!(argv("opencode"), ["opencode"]);
    assert_eq!(argv("my-agent"), ["my-agent"]);
}

#[test]
fn a_worktree_lands_where_herdr_would_put_it_from_the_base() {
    let fixture = Fixture::new();
    let repo = fixture.repo();
    let root = fixture.root().join("worktrees");
    let checkout = worktree::add(&repo, "automate/nightly-1-2", "main", &root, || false).unwrap();
    assert_eq!(checkout, root.join("repo").join("automate-nightly-1-2"));
    assert!(checkout.join("README.md").is_file());
}
