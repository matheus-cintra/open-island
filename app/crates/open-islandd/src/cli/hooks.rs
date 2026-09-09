use crate::installer;
use serde_json::{json, Value};
use std::env;
use std::path::Path;

pub fn report_hooks_status(home: &Path, agents: &[&str], executable: &Path) -> i32 {
    let mut report = serde_json::Map::new();
    for agent in agents {
        let detected = installer::detected(home, agent);
        let installed = detected && installer::installed(home, agent, executable).unwrap_or(false);
        report.insert(
            (*agent).to_owned(),
            json!({"detected": detected, "installed": installed}),
        );
    }
    println!("{}", Value::Object(report));
    0
}

pub fn run_hooks() -> i32 {
    let mut args = env::args().skip(2);
    let action = args.next();
    let mut agent = "all".to_owned();
    let mut dry_run = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--agent" => {
                let Some(value) = args.next() else {
                    eprintln!("open-islandd: --agent requires a value");
                    return 2;
                };
                agent = value;
            }
            "--dry-run" => dry_run = true,
            "--socket" => {
                let _ = args.next();
            }
            other => {
                eprintln!("open-islandd: unknown hooks option '{other}'");
                return 2;
            }
        }
    }
    let agents = match installer::selected_agents(&agent) {
        Ok(agents) => agents,
        Err(error) => {
            eprintln!("open-islandd: {error}");
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
    if action.as_deref() == Some("status") {
        return report_hooks_status(&home, &agents, &executable);
    }
    let result = match action.as_deref() {
        Some("install") => installer::install(&home, &agents, &executable, dry_run),
        Some("uninstall") => installer::uninstall(&home, &agents, &executable, dry_run),
        Some(other) => Err(format!("unknown hooks action '{other}'")),
        None => Err("hooks requires install, uninstall or status".to_owned()),
    };
    match result {
        Ok(paths) => {
            for path in paths {
                println!(
                    "{} {}",
                    if dry_run { "would-change" } else { "changed" },
                    path.display()
                );
            }
            if action.as_deref() == Some("install") {
                for note in installer::install_notes(&agents) {
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
