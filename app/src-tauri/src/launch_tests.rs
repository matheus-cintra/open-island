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

fn lookup_among(available: &'static [&'static str]) -> impl Fn(&str) -> Option<PathBuf> {
    move |name| {
        available
            .contains(&name)
            .then(|| PathBuf::from(format!("/usr/bin/{name}")))
    }
}

#[test]
fn kitty_runs_the_agent_inside_the_folder_through_sh() {
    assert_eq!(
        session_argv(
            Path::new("/usr/bin/kitty"),
            Path::new("/home/x/proj"),
            "claude"
        ),
        [
            "/usr/bin/kitty",
            "sh",
            "-c",
            "cd \"$1\" && exec \"$2\"",
            "sh",
            "/home/x/proj",
            "claude"
        ]
        .map(str::to_owned)
    );
}

#[test]
fn alacritty_takes_the_same_command_after_dash_e() {
    let argv = session_argv(
        Path::new("/usr/bin/alacritty"),
        Path::new("/home/x/proj"),
        "codex",
    );
    assert_eq!(argv[..2], ["/usr/bin/alacritty", "-e"].map(str::to_owned));
    assert_eq!(
        argv[2..5],
        ["sh", "-c", "cd \"$1\" && exec \"$2\""].map(str::to_owned)
    );
    assert_eq!(
        argv[5..],
        ["sh", "/home/x/proj", "codex"].map(str::to_owned)
    );
}

#[test]
fn available_keeps_the_shipped_order_and_only_what_resolves() {
    assert_eq!(
        available(lookup_among(&["opencode", "claude"])),
        vec!["claude".to_owned(), "opencode".to_owned()]
    );
    assert_eq!(available(|_| None), Vec::<String>::new());
    assert_eq!(
        available(lookup_among(&["claude", "codex", "opencode"])),
        AGENTS.map(str::to_owned)
    );
}

#[test]
fn a_relative_or_missing_folder_is_refused_before_anything_spawns() {
    let relative = open("relative/dir", "claude").unwrap_err();
    assert!(relative.contains("is not a directory"), "{relative}");
    let missing = open("/nonexistent-open-island", "claude").unwrap_err();
    assert!(missing.contains("is not a directory"), "{missing}");
}

#[test]
fn an_unknown_agent_is_refused_even_with_a_real_folder() {
    let error = open("/tmp", "rm -rf /").unwrap_err();
    assert!(error.contains("unknown agent"), "{error}");
}
