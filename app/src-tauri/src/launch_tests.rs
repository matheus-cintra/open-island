use super::*;

#[test]
fn the_three_shipped_agents_are_known() {
    assert_eq!(known_agent("claude"), Ok("claude"));
    assert_eq!(known_agent("codex"), Ok("codex"));
    assert_eq!(known_agent("opencode"), Ok("opencode"));
}

#[test]
fn anything_else_is_refused_by_name() {
    for name in ["rm -rf /", "Claude", "", "claude "] {
        let error = known_agent(name).unwrap_err();
        assert!(error.contains("unknown agent"), "{name:?}: {error}");
    }
}
