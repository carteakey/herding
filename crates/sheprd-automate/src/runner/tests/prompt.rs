use super::*;

#[test]
fn a_refused_prompt_fails_its_step_but_keeps_the_agents_pane() {
    let fixture = Fixture::new();
    let repo = fixture.repo();
    let script = fixture.script(
        &repo,
        "refused",
        "herdr.manual(function()\n  agent { harness = \"claude\", prompt = \"refuse-me\", check = false }\nend)\n",
    );
    let (run, _) = run_script(&fixture, &script, &repo, false);
    let steps = fixture.store().steps(run).unwrap();
    assert_eq!(steps[0].state, StepState::Failed);
    // The agent runs on, so the run's page can still open it.
    assert_eq!(steps[0].pane.as_deref(), Some("w1p1"));
    assert!(
        steps[0].log_tail.contains("blocked"),
        "{}",
        steps[0].log_tail
    );
    assert_eq!(fixture.store().run_agents(run).unwrap().len(), 1);
}
