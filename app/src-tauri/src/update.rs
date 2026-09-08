use std::{
    env,
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Command,
};

const INSTALL_SCRIPT: &str = "curl -fsSL https://raw.githubusercontent.com/matheus-cintra/open-island/master/install.sh | sh && systemctl --user restart open-islandd.service open-island.service; printf '\\n%s' \"$1\"; read dummy";
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

pub fn run(prompt: &str) -> Result<(), String> {
    let preferred = env::var("TERMINAL").ok();
    let program = pick_terminal(preferred.as_deref(), on_path)
        .ok_or_else(|| "no terminal emulator found on PATH".to_owned())?;
    let status = Command::new("systemd-run")
        .args(["--user", "--collect", "--quiet", "--"])
        .args(terminal_argv(&program, INSTALL_SCRIPT, prompt))
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

fn on_path(name: &str) -> Option<PathBuf> {
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

fn terminal_argv(program: &Path, script: &str, prompt: &str) -> Vec<String> {
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
    argv.extend(["sh", "-c", script, "sh", prompt].map(str::to_owned));
    argv
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;
