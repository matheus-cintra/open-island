use super::run_tool;
use crate::installer;
use serde_json::json;
use std::env;

pub fn run_hotkey() -> i32 {
    let mut args = env::args().skip(2);
    let action = args.next();
    let mut combo = installer::DEFAULT_HOTKEY.to_owned();
    let mut dry_run = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--combo" => {
                let Some(value) = args.next() else {
                    eprintln!("open-islandd: --combo requires a value");
                    return 2;
                };
                combo = value;
            }
            "--dry-run" => dry_run = true,
            "--socket" => {
                let _ = args.next();
            }
            other => {
                eprintln!("open-islandd: unknown hotkey option '{other}'");
                return 2;
            }
        }
    }
    let install = match action.as_deref() {
        Some("install") => true,
        Some("uninstall") => false,
        Some("status") => return report_hotkey_status(&combo),
        Some(other) => {
            eprintln!("open-islandd: unknown hotkey action '{other}'");
            return 2;
        }
        None => {
            eprintln!("open-islandd: hotkey requires install, uninstall or status");
            return 2;
        }
    };
    let home = match installer::home_dir() {
        Ok(home) => home,
        Err(error) => {
            eprintln!("open-islandd: {error}");
            return 1;
        }
    };
    let executable = match env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("open-islandd: resolve executable: {error}");
            return 1;
        }
    };
    match installer::install_hotkey(&home, &executable, &combo, install, dry_run) {
        Ok(paths) => {
            for path in paths {
                println!(
                    "{} {}",
                    if dry_run { "would-change" } else { "changed" },
                    path.display()
                );
            }
            if !dry_run {
                println!("ran hyprctl reload -> {}", run_tool("hyprctl", &["reload"]));
            }
            if install {
                for note in installer::hotkey_notes(&combo) {
                    println!("note {note}");
                }
            }
            0
        }
        Err(error) => {
            eprintln!("open-islandd: {error}");
            1
        }
    }
}

pub fn report_hotkey_status(combo: &str) -> i32 {
    let home = match installer::home_dir() {
        Ok(home) => home,
        Err(error) => {
            eprintln!("open-islandd: {error}");
            return 1;
        }
    };
    let executable = match env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("open-islandd: resolve executable: {error}");
            return 1;
        }
    };
    match installer::install_hotkey(&home, &executable, combo, true, true) {
        Ok(pending) => {
            println!("{}", json!({"installed": pending.is_empty()}));
            0
        }
        Err(error) => {
            eprintln!("open-islandd: {error}");
            1
        }
    }
}
