use super::run_tool;
use crate::installer;
use serde_json::json;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub const PACKAGED_DAEMON: &str = "/usr/bin/open-islandd";
pub const PACKAGED_ISLAND: &str = "/usr/bin/open-island";

pub fn island_candidates_beside(executable: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(directory) = executable.parent() {
        candidates.push(directory.join("open-island"));
    }
    candidates.push(PathBuf::from("open-island"));
    candidates
}

pub fn island_candidates() -> Vec<PathBuf> {
    match env::current_exe() {
        Ok(executable) => island_candidates_beside(&executable),
        Err(_) => vec![PathBuf::from("open-island")],
    }
}

pub fn run_autostart() -> i32 {
    let mut args = env::args().skip(2);
    let action = args.next();
    let mut dry_run = false;
    for arg in args {
        match arg.as_str() {
            "--dry-run" => dry_run = true,
            other => {
                eprintln!("open-islandd: unknown autostart option '{other}'");
                return 2;
            }
        }
    }
    let install = match action.as_deref() {
        Some("install") => true,
        Some("uninstall") => false,
        Some("status") => return report_autostart_status(),
        Some(other) => {
            eprintln!("open-islandd: unknown autostart action '{other}'");
            return 2;
        }
        None => {
            eprintln!("open-islandd: autostart requires install, uninstall or status");
            return 2;
        }
    };
    let Some((home, daemon, island)) = autostart_paths() else {
        return 1;
    };
    if !install && !dry_run {
        run_systemd_steps(false);
    }
    match installer::install_autostart(&home, &daemon, &island, install, dry_run) {
        Ok(paths) => {
            for path in paths {
                println!(
                    "{} {}",
                    if dry_run { "would-change" } else { "changed" },
                    path.display()
                );
            }
            if !dry_run {
                refresh_icon_cache(&home);
            }
            if install && !dry_run {
                run_systemd_steps(true);
            }
            for note in installer::autostart_notes(install) {
                println!("note {note}");
            }
            0
        }
        Err(error) => {
            eprintln!("open-islandd: {error}");
            1
        }
    }
}

// The three files existing is not the same as systemd being told to start them: the units
// were present and disabled after a reboot, and the pane still reported Ativo.
pub fn units_enabled() -> bool {
    ["open-islandd.service", "open-island.service"]
        .iter()
        .all(|unit| {
            std::process::Command::new("systemctl")
                .args(["--user", "is-enabled", unit])
                .output()
                .is_ok_and(|output| output.status.success())
        })
}

/// GTK trusts `icon-theme.cache` over the directory it sits in; without this the icon is invisible.
pub fn refresh_icon_cache(home: &Path) {
    let theme = home.join(".local/share/icons/hicolor");
    let Some(directory) = theme.to_str() else {
        return;
    };
    println!(
        "ran gtk-update-icon-cache {directory} -> {}",
        run_tool(
            "gtk-update-icon-cache",
            &["-f", "-t", "--ignore-theme-index", directory],
        )
    );
}

pub fn run_systemd_steps(install: bool) {
    for (program, arguments) in systemd_steps(install) {
        let rendered = arguments.join(" ");
        println!(
            "ran {program} {rendered} -> {}",
            run_tool(program, &arguments)
        );
    }
}

pub fn systemd_steps(install: bool) -> Vec<(&'static str, Vec<&'static str>)> {
    let units = ["open-islandd.service", "open-island.service"];
    let toggle = if install { "enable" } else { "disable" };
    let mut arguments = vec!["--user", toggle, "--now"];
    arguments.extend(units);
    if install {
        return vec![
            ("systemctl", vec!["--user", "daemon-reload"]),
            ("systemctl", arguments),
        ];
    }
    vec![
        ("systemctl", arguments),
        ("systemctl", vec!["--user", "daemon-reload"]),
    ]
}

pub fn autostart_paths() -> Option<(PathBuf, PathBuf, PathBuf)> {
    let home = match installer::home_dir() {
        Ok(home) => home,
        Err(error) => {
            eprintln!("open-islandd: {error}");
            return None;
        }
    };
    let daemon = match env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("open-islandd: resolve executable: {error}");
            return None;
        }
    };
    let daemon = fs::canonicalize(&daemon).unwrap_or(daemon);
    let (daemon, island) = autostart_executables(daemon);
    Some((home, daemon, island))
}

pub fn autostart_executables(daemon: PathBuf) -> (PathBuf, PathBuf) {
    if installer::is_packaged_executable(&daemon) {
        return (
            PathBuf::from(PACKAGED_DAEMON),
            PathBuf::from(PACKAGED_ISLAND),
        );
    }
    let island = island_candidates_beside(&daemon)
        .into_iter()
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| PathBuf::from("open-island"));
    (daemon, island)
}

pub fn report_autostart_status() -> i32 {
    let Some((home, daemon, island)) = autostart_paths() else {
        return 1;
    };
    match installer::install_autostart(&home, &daemon, &island, true, true) {
        Ok(pending) => {
            println!(
                "{}",
                json!({"installed": pending.is_empty() && units_enabled()})
            );
            0
        }
        Err(error) => {
            eprintln!("open-islandd: {error}");
            1
        }
    }
}

#[cfg(test)]
#[path = "autostart_tests.rs"]
mod tests;
