use std::{
    env,
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Command,
};

const TERMINALS: [&str; 8] = [
    "kitty",
    "alacritty",
    "foot",
    "wezterm",
    "ghostty",
    "gnome-terminal",
    "konsole",
    "xterm",
];

pub fn pick() -> Option<PathBuf> {
    let preferred = env::var("TERMINAL").ok();
    pick_terminal(preferred.as_deref(), on_path)
}

pub fn spawn_detached(argv: Vec<String>) -> Result<(), String> {
    let status = Command::new("systemd-run")
        .args(["--user", "--collect", "--quiet", "--"])
        .args(argv)
        .status()
        .map_err(|error| format!("systemd-run: {error}"))?;
    if status.success() {
        return Ok(());
    }
    Err(format!("systemd-run exited with {status}"))
}

fn pick_terminal(
    preferred: Option<&str>,
    lookup: impl Fn(&str) -> Option<PathBuf>,
) -> Option<PathBuf> {
    preferred
        .filter(|name| !name.is_empty())
        .and_then(&lookup)
        .or_else(|| TERMINALS.iter().find_map(|name| lookup(name)))
}

pub(crate) fn on_path(name: &str) -> Option<PathBuf> {
    let candidate = Path::new(name);
    if candidate.is_absolute() {
        return candidate.is_file().then(|| candidate.to_path_buf());
    }
    env::var_os("PATH").and_then(|path| {
        env::split_paths(&path)
            .map(|directory| directory.join(name))
            .find(|full| full.is_file())
    })
}

pub fn terminal_argv(program: &Path, script: &str, args: &[&str]) -> Vec<String> {
    let kind = program
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default();
    let mut argv = vec![program.to_string_lossy().into_owned()];
    match kind {
        "kitty" | "foot" => {}
        "wezterm" => argv.extend(["start", "--"].map(str::to_owned)),
        "gnome-terminal" => argv.push("--".to_owned()),
        _ => argv.push("-e".to_owned()),
    }
    argv.extend(["sh", "-c", script, "sh"].map(str::to_owned));
    argv.extend(args.iter().map(|arg| (*arg).to_owned()));
    argv
}

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;
