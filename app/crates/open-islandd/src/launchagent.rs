//! launchd files are managed with the same ownership marker as Linux autostart.
use std::{
    path::{Path, PathBuf},
    process::Command,
};
const MARKER: &str = "<!-- Managed by Open Island -->";
pub const LABELS: [&str; 2] = ["app.open-island.daemon", "app.open-island.panel"];
pub fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
pub fn plist(label: &str, executable: &Path) -> String {
    let executable = escape(&executable.to_string_lossy());
    let keep_alive = if label == LABELS[0] {
        "<key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>"
    } else {
        ""
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
{MARKER}
<key>Label</key><string>{label}</string>
<key>ProgramArguments</key><array><string>{executable}</string></array>
<key>RunAtLoad</key><true/>
{keep_alive}
<key>LimitLoadToSessionType</key><string>Aqua</string>
<key>ProcessType</key><string>Interactive</string>
</dict></plist>
"#
    )
}
pub fn files(home: &Path) -> Vec<PathBuf> {
    LABELS
        .iter()
        .map(|label| {
            home.join("Library/LaunchAgents")
                .join(format!("{label}.plist"))
        })
        .collect()
}
pub fn install(
    home: &Path,
    daemon: &Path,
    island: &Path,
    enabled: bool,
    dry: bool,
) -> Result<Vec<PathBuf>, String> {
    let mut changed = Vec::new();
    for ((path, label), executable) in files(home).into_iter().zip(LABELS).zip([daemon, island]) {
        if super::installer::merge_managed_file(
            &path,
            &plist(label, executable),
            MARKER,
            enabled,
            dry,
        )? {
            changed.push(path);
        }
    }
    Ok(changed)
}
fn domain() -> String {
    format!("gui/{}", unsafe { libc::geteuid() })
}
pub fn loaded() -> bool {
    LABELS.iter().all(|label| {
        Command::new("/bin/launchctl")
            .args(["print", &format!("{}/{label}", domain())])
            .output()
            .is_ok_and(|output| output.status.success())
    })
}
pub fn stop(label: &str) -> Result<(), String> {
    let target = format!("{}/{label}", domain());
    // bootout is idempotent: an unloaded job requires no action.
    if !Command::new("/bin/launchctl")
        .args(["print", &target])
        .output()
        .map_err(|e| e.to_string())?
        .status
        .success()
    {
        return Ok(());
    }
    let output = Command::new("/bin/launchctl")
        .args(["bootout", &target])
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().into())
    }
}
pub fn apply(home: &Path, enabled: bool) -> Result<(), String> {
    for (path, label) in files(home).into_iter().zip(LABELS) {
        if !enabled {
            stop(label)?;
            continue;
        }
        let target = format!("{}/{label}", domain());
        if Command::new("/bin/launchctl")
            .args(["print", &target])
            .output()
            .is_ok_and(|out| out.status.success())
        {
            continue;
        }
        if enabled {
            let output = Command::new("/bin/launchctl")
                .arg("bootstrap")
                .arg(domain())
                .arg(path)
                .output()
                .map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err(format!(
                    "launchctl: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plist_quotes_paths_and_uses_argument_array() {
        let text = plist(
            LABELS[0],
            Path::new("/Applications/A & B.app/Contents/MacOS/open-islandd"),
        );
        assert!(text.contains("A &amp; B.app"));
        assert!(text.contains("<array><string>/Applications/"));
        assert!(text.contains("<key>SuccessfulExit</key><false/>"));
        assert!(!plist(LABELS[1], Path::new("/app")).contains("KeepAlive"));
    }
    #[test]
    fn files_are_idempotent_removable_and_protect_unrelated_files() {
        let root = std::env::temp_dir().join(format!("oi-launchagent-test-{}", std::process::id()));
        struct Guard(PathBuf);
        impl Drop for Guard {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _guard = Guard(root.clone());
        let daemon = Path::new("/Applications/Open Island.app/Contents/MacOS/open-islandd");
        let panel = daemon.with_file_name("open-island");
        assert_eq!(install(&root, daemon, &panel, true, true).unwrap().len(), 2);
        assert!(!root.exists());
        assert_eq!(
            install(&root, daemon, &panel, true, false).unwrap().len(),
            2
        );
        assert!(install(&root, daemon, &panel, true, false)
            .unwrap()
            .is_empty());
        assert_eq!(
            install(&root, daemon, &panel, false, false).unwrap().len(),
            2
        );
        std::fs::write(&files(&root)[0], "unrelated").unwrap();
        assert!(install(&root, daemon, &panel, false, false).is_err());
    }
}
