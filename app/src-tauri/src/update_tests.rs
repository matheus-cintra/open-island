use super::*;

const SCRIPT: &str = "curl -fsSL https://example.test/install.sh | sh && echo \"it's done\"";
const PROMPT: &str = "Pressione Enter para fechar.";

fn argv(program: &str) -> Vec<String> {
    terminal_argv(Path::new(program), SCRIPT, PROMPT)
}

fn tail() -> Vec<String> {
    ["sh", "-c", SCRIPT, "sh", PROMPT]
        .map(str::to_owned)
        .to_vec()
}

fn with_head(head: &[&str]) -> Vec<String> {
    let mut expected = head
        .iter()
        .map(|part| (*part).to_owned())
        .collect::<Vec<_>>();
    expected.extend(tail());
    expected
}

#[test]
fn kitty_and_foot_take_the_command_positionally() {
    assert_eq!(argv("/usr/bin/kitty"), with_head(&["/usr/bin/kitty"]));
    assert_eq!(argv("/usr/bin/foot"), with_head(&["/usr/bin/foot"]));
}

#[test]
fn wezterm_needs_start_and_gnome_terminal_needs_a_double_dash() {
    assert_eq!(
        argv("/usr/bin/wezterm"),
        with_head(&["/usr/bin/wezterm", "start", "--"])
    );
    assert_eq!(
        argv("/usr/bin/gnome-terminal"),
        with_head(&["/usr/bin/gnome-terminal", "--"])
    );
}

#[test]
fn the_dash_e_family_and_any_unknown_emulator_get_dash_e() {
    for program in [
        "/usr/bin/alacritty",
        "/usr/bin/ghostty",
        "/usr/bin/konsole",
        "/usr/bin/xterm",
        "/opt/weird-term",
    ] {
        assert_eq!(argv(program), with_head(&[program, "-e"]), "{program}");
    }
}

#[test]
fn the_script_and_the_prompt_each_stay_one_argument() {
    let argv = argv("/usr/bin/kitty");
    assert_eq!(argv.len(), 6);
    assert_eq!(argv[3], SCRIPT);
    assert_eq!(argv[5], PROMPT);
}

#[test]
fn the_shipped_script_reinstalls_then_restarts_both_units_and_waits() {
    assert!(INSTALL_SCRIPT.starts_with(
        "curl -fsSL https://raw.githubusercontent.com/matheus-cintra/open-island/master/install.sh | sh && systemctl --user restart open-islandd.service open-island.service;"
    ));
    assert!(INSTALL_SCRIPT.ends_with("printf '\\n%s' \"$1\"; read dummy"));
}

fn lookup_among(available: &'static [&'static str]) -> impl Fn(&str) -> Option<PathBuf> {
    move |name| {
        available
            .contains(&name)
            .then(|| PathBuf::from(format!("/usr/bin/{name}")))
    }
}

#[test]
fn the_terminal_variable_wins_when_it_resolves() {
    let picked = pick_terminal(Some("ghostty"), lookup_among(&["kitty", "ghostty"]));
    assert_eq!(picked, Some(PathBuf::from("/usr/bin/ghostty")));
    let absolute = pick_terminal(Some("/opt/term/bin/foo"), |name| {
        (name == "/opt/term/bin/foo").then(|| PathBuf::from(name))
    });
    assert_eq!(absolute, Some(PathBuf::from("/opt/term/bin/foo")));
}

#[test]
fn a_terminal_variable_that_does_not_resolve_falls_back_to_the_list_in_order() {
    let picked = pick_terminal(
        Some("missing"),
        lookup_among(&["xterm", "foot", "alacritty"]),
    );
    assert_eq!(picked, Some(PathBuf::from("/usr/bin/alacritty")));
    let empty = pick_terminal(Some(""), lookup_among(&["foot"]));
    assert_eq!(empty, Some(PathBuf::from("/usr/bin/foot")));
}

#[test]
fn nothing_available_means_none() {
    assert_eq!(pick_terminal(None, lookup_among(&[])), None);
    assert_eq!(pick_terminal(Some("kitty"), lookup_among(&[])), None);
}
