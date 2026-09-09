use crate::daemon_config::hook_timeout;
use crate::server::socket_path;
use crate::{claude_hook, codex_hook, opencode_hook};
use std::env;
use std::io::{self, Read, Write};

pub fn run_hook() -> i32 {
    let socket = socket_path();
    let mut agent = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--agent" {
            agent = args.next();
            break;
        }
    }
    match agent.as_deref() {
        Some("claude") | Some("codex") | Some("opencode") => {}
        Some(other) => {
            eprintln!("open-islandd: unsupported agent '{other}'");
            return 2;
        }
        None => {
            eprintln!("open-islandd: hook requires --agent claude|codex|opencode");
            return 2;
        }
    }
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        eprintln!("open-islandd: failed to read hook input from stdin");
        return 1;
    }
    let result =
        match agent.as_deref() {
            Some("claude") => claude_hook::handle(&input, &socket, hook_timeout())
                .map_err(|error| error.to_string()),
            Some("codex") => {
                codex_hook::run(&input, &socket, hook_timeout()).map_err(|error| error.to_string())
            }
            Some("opencode") => opencode_hook::run(&input, &socket, hook_timeout())
                .map_err(|error| error.to_string()),
            Some(other) => Err(format!("unsupported agent '{other}'")),
            None => Err("hook requires --agent".to_owned()),
        };
    match result {
        Ok(output) => {
            if output.is_empty() {
                return 0;
            }
            let mut stdout = io::stdout().lock();
            if writeln!(stdout, "{output}").is_err() {
                return 1;
            }
            let _ = stdout.flush();
            0
        }
        Err(error) => {
            eprintln!("open-islandd: {error}");
            1
        }
    }
}
