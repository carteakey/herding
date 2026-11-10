use super::*;

#[test]
fn detects_actual_agent_programs_not_arguments_to_other_programs() {
    let args = |values: &[&str]| {
        values
            .iter()
            .map(|value| (*value).into())
            .collect::<Vec<_>>()
    };
    assert!(is_agent(
        Agent::Codex,
        &args(&["/usr/local/bin/codex", "resume"])
    ));
    assert!(is_agent(
        Agent::Claude,
        &args(&["node", "/opt/node_modules/@anthropic-ai/claude-code/cli.js"])
    ));
    assert!(!is_agent(Agent::Codex, &args(&["echo", "codex"])));
    assert!(!is_agent(Agent::Codex, &args(&["bash", "-c", "codex"])));
    assert!(!is_agent(Agent::Codex, &args(&["claude"])));
}
