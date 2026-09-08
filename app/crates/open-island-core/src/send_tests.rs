use super::*;
use crate::{
    runner::FakeRunner,
    terminal::{MultiplexerKind, MultiplexerLayer, TerminalLayer},
};

fn host(kind: &str, env: &[(&str, &str)]) -> TerminalInfo {
    TerminalInfo {
        kind: kind.to_owned(),
        raise_pid: 77,
        agent_pid: 42,
        multiplexer: None,
        terminal: Some(TerminalLayer {
            kind: kind.to_owned(),
            pid: 77,
            window_id: None,
        }),
        editor: None,
        env: env
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect(),
    }
}

fn with_multiplexer(
    mut info: TerminalInfo,
    kind: MultiplexerKind,
    pane_id: Option<&str>,
) -> TerminalInfo {
    info.multiplexer = Some(MultiplexerLayer {
        kind,
        pane_id: pane_id.map(str::to_owned),
        session_name: Some("main".to_owned()),
        socket: None,
        server_pid: Some(5),
    });
    info
}

fn never(_: &Path) -> bool {
    false
}

fn always(_: &Path) -> bool {
    true
}

fn args(step: &SendStep) -> Vec<&str> {
    step.args.iter().map(String::as_str).collect()
}

#[test]
fn a_multiplexer_step_wins_over_the_emulator() {
    let kitty = host("kitty", &[("KITTY_LISTEN_ON", "unix:/tmp/kitty-1")]);
    let steps = [
        JumpStep::TmuxSelectPane {
            socket: Some("/tmp/tmux-1000/default".into()),
            pane: "%3".into(),
        },
        JumpStep::KittyFocusWindow {
            window_id: "9".into(),
        },
    ];
    assert_eq!(
        channel_for(&kitty, &steps, None, &never),
        Ok(Channel::Tmux {
            socket: Some("/tmp/tmux-1000/default".into()),
            pane: "%3".into()
        })
    );
}

#[test]
fn a_gone_tmux_pane_blocks_the_channel() {
    let steps = [JumpStep::TmuxPaneGone, JumpStep::RaiseWindow { pid: 77 }];
    assert_eq!(
        channel_for(&host("kitty", &[]), &steps, None, &never),
        Err(Blocked::PaneGone)
    );
}

#[test]
fn zellij_carries_its_session_and_pane() {
    let steps = [JumpStep::ZellijFocusPane {
        session: "main".into(),
        pane_id: "terminal_2".into(),
    }];
    assert_eq!(
        channel_for(&host("kitty", &[]), &steps, None, &never),
        Ok(Channel::Zellij {
            session: "main".into(),
            pane_id: "terminal_2".into()
        })
    );
}

#[test]
fn wezterm_takes_the_socket_from_the_env_or_the_runtime_dir() {
    let steps = [JumpStep::WeztermActivatePane {
        pane_id: "4".into(),
    }];
    let from_env = host(
        "wezterm",
        &[("WEZTERM_UNIX_SOCKET", "/run/user/1000/wezterm/gui-sock-1")],
    );
    assert_eq!(
        channel_for(&from_env, &steps, None, &never),
        Ok(Channel::Wezterm {
            socket: "/run/user/1000/wezterm/gui-sock-1".into(),
            pane_id: "4".into()
        })
    );
    let bare = host("wezterm", &[("WEZTERM_PANE", "4")]);
    assert_eq!(
        channel_for(&bare, &steps, Some(Path::new("/run/user/1000")), &always),
        Ok(Channel::Wezterm {
            socket: "/run/user/1000/wezterm/gui-sock-77".into(),
            pane_id: "4".into()
        })
    );
    assert_eq!(
        channel_for(&bare, &steps, Some(Path::new("/run/user/1000")), &never),
        Err(Blocked::WeztermSocketMissing)
    );
    assert_eq!(
        channel_for(&bare, &steps, None, &always),
        Err(Blocked::WeztermSocketMissing)
    );
}

#[test]
fn kitty_needs_the_listen_socket() {
    let steps = [JumpStep::KittyFocusWindow {
        window_id: "9".into(),
    }];
    assert_eq!(
        channel_for(
            &host("kitty", &[("KITTY_LISTEN_ON", "unix:/tmp/kitty-1")]),
            &steps,
            None,
            &never
        ),
        Ok(Channel::Kitty {
            socket: "unix:/tmp/kitty-1".into(),
            window_id: "9".into()
        })
    );
    assert_eq!(
        channel_for(&host("kitty", &[]), &steps, None, &never),
        Err(Blocked::KittyRemoteControlOff)
    );
    assert_eq!(
        channel_for(
            &host("kitty", &[]),
            &[JumpStep::RaiseWindow { pid: 77 }],
            None,
            &never
        ),
        Err(Blocked::KittyRemoteControlOff)
    );
}

#[test]
fn hosts_without_a_channel_are_unsupported() {
    for kind in ["ghostty", "alacritty", "unknown", "code"] {
        assert_eq!(
            channel_for(
                &host(kind, &[]),
                &[JumpStep::RaiseWindow { pid: 77 }],
                None,
                &never
            ),
            Err(Blocked::HostUnsupported),
            "{kind}"
        );
    }
}

#[test]
fn capability_reads_the_process_tree_and_env_only() {
    let plain_kitty = host("kitty", &[]);
    assert_eq!(
        capability(&plain_kitty),
        Err(Blocked::KittyRemoteControlOff)
    );
    assert_eq!(
        capability(&host("kitty", &[("KITTY_LISTEN_ON", "unix:/tmp/kitty-1")])),
        Ok("kitty")
    );
    assert_eq!(
        capability(&with_multiplexer(
            plain_kitty.clone(),
            MultiplexerKind::Tmux,
            Some("%1")
        )),
        Ok("tmux")
    );
    assert_eq!(
        capability(&with_multiplexer(
            plain_kitty,
            MultiplexerKind::Zellij,
            None
        )),
        Err(Blocked::KittyRemoteControlOff)
    );
    assert_eq!(
        capability(&host("wezterm", &[("WEZTERM_PANE", "2")])),
        Ok("wezterm")
    );
    assert_eq!(
        capability(&host("wezterm", &[])),
        Err(Blocked::WeztermSocketMissing)
    );
    assert_eq!(
        capability(&host("ghostty", &[])),
        Err(Blocked::HostUnsupported)
    );
    assert_eq!(Blocked::HostUnsupported.code(), "host_unsupported");
    assert_eq!(Blocked::PaneGone.code(), "pane_gone");
}

#[test]
fn normalisation_unifies_line_endings_and_drops_the_trailing_newline() {
    assert_eq!(normalize("oi\r\ntudo\rbem\n"), "oi\ntudo\nbem");
    assert_eq!(normalize("uma linha\n\n"), "uma linha");
}

#[test]
fn tmux_types_a_single_line_and_pastes_a_multi_line_through_a_named_buffer() {
    let channel = Channel::Tmux {
        socket: Some("/tmp/tmux-1000/default".into()),
        pane: "%3".into(),
    };
    let single = plan(&channel, "responda só ok");
    assert_eq!(
        single.iter().map(args).collect::<Vec<_>>(),
        vec![
            vec![
                "-S",
                "/tmp/tmux-1000/default",
                "send-keys",
                "-t",
                "%3",
                "-l",
                "responda só ok"
            ],
            vec![
                "-S",
                "/tmp/tmux-1000/default",
                "send-keys",
                "-t",
                "%3",
                "Enter"
            ],
        ]
    );
    let multi = plan(
        &Channel::Tmux {
            socket: None,
            pane: "%3".into(),
        },
        "linha um\nlinha dois\n",
    );
    assert_eq!(
        multi.iter().map(args).collect::<Vec<_>>(),
        vec![
            vec!["set-buffer", "-b", "open-island", "linha um\nlinha dois"],
            vec!["paste-buffer", "-p", "-d", "-b", "open-island", "-t", "%3"],
            vec!["send-keys", "-t", "%3", "Enter"],
        ]
    );
    assert!(multi.iter().all(|step| step.program == "tmux"));
}

#[test]
fn zellij_focuses_the_pane_writes_the_text_then_a_carriage_return() {
    let steps = plan(
        &Channel::Zellij {
            session: "main".into(),
            pane_id: "terminal_2".into(),
        },
        "oi\ntudo",
    );
    assert_eq!(
        steps.iter().map(args).collect::<Vec<_>>(),
        vec![
            vec!["--session", "main", "action", "focus-pane-id", "terminal_2"],
            vec!["--session", "main", "action", "write-chars", "oi\ntudo"],
            vec!["--session", "main", "action", "write", "13"],
        ]
    );
    assert!(steps.iter().all(|step| step.program == "zellij"));
}

#[test]
fn wezterm_goes_through_env_with_the_gui_socket() {
    let channel = Channel::Wezterm {
        socket: "/run/user/1000/wezterm/gui-sock-77".into(),
        pane_id: "4".into(),
    };
    let single = plan(&channel, "oi");
    assert_eq!(single[0].program, "env");
    assert_eq!(
        args(&single[0]),
        vec![
            "WEZTERM_UNIX_SOCKET=/run/user/1000/wezterm/gui-sock-77",
            "wezterm",
            "cli",
            "send-text",
            "--pane-id",
            "4",
            "--no-paste",
            "oi"
        ]
    );
    assert_eq!(
        args(&single[1]),
        vec![
            "WEZTERM_UNIX_SOCKET=/run/user/1000/wezterm/gui-sock-77",
            "wezterm",
            "cli",
            "send-text",
            "--pane-id",
            "4",
            "--no-paste",
            "\r"
        ]
    );
    let multi = plan(&channel, "a\nb");
    assert_eq!(
        args(&multi[0]),
        vec![
            "WEZTERM_UNIX_SOCKET=/run/user/1000/wezterm/gui-sock-77",
            "wezterm",
            "cli",
            "send-text",
            "--pane-id",
            "4",
            "a\nb"
        ]
    );
}

#[test]
fn kitty_targets_its_socket_doubles_backslashes_and_pastes_multi_line() {
    let channel = Channel::Kitty {
        socket: "unix:/tmp/kitty-1".into(),
        window_id: "9".into(),
    };
    let single = plan(&channel, r"a\b");
    assert_eq!(
        args(&single[0]),
        vec![
            "@",
            "--to",
            "unix:/tmp/kitty-1",
            "send-text",
            "--match",
            "id:9",
            r"a\\b"
        ]
    );
    assert_eq!(
        args(&single[1]),
        vec![
            "@",
            "--to",
            "unix:/tmp/kitty-1",
            "send-text",
            "--match",
            "id:9",
            r"\r"
        ]
    );
    let multi = plan(&channel, "a\nb");
    assert_eq!(
        args(&multi[0]),
        vec![
            "@",
            "--to",
            "unix:/tmp/kitty-1",
            "send-text",
            "--match",
            "id:9",
            "--bracketed-paste=enable",
            "a\nb"
        ]
    );
    assert!(single
        .iter()
        .chain(multi.iter())
        .all(|step| step.program == "kitty"));
}

#[test]
fn execute_runs_every_step_in_order_and_stops_at_the_first_failure() {
    let runner = FakeRunner::new();
    runner.push_ok("tmux", "");
    runner.push_status("tmux", 1, "", "can't find pane: %3");
    let steps = plan(
        &Channel::Tmux {
            socket: None,
            pane: "%3".into(),
        },
        "oi",
    );
    assert_eq!(
        execute(&steps, &runner),
        Err("tmux failed: can't find pane: %3".to_owned())
    );
    assert_eq!(runner.calls().len(), 2);

    let runner = FakeRunner::new();
    runner.push_ok("tmux", "");
    runner.push_ok("tmux", "");
    assert_eq!(execute(&steps, &runner), Ok(()));
    assert_eq!(
        runner.calls()[0],
        (
            "tmux".to_owned(),
            vec!["send-keys", "-t", "%3", "-l", "oi"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        )
    );
}
