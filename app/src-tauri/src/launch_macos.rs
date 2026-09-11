//! Launch into a new terminal surface; never inject into the active user's tab.
#[cfg(target_os = "macos")]
use std::path::Path;

pub fn quote_shell(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\"'\"'"))
}
pub fn applescript(folder: &str, executable: &str, iterm: bool) -> String {
    let command = format!("cd {} && {}", quote_shell(folder), quote_shell(executable));
    // Profile commands are executables, while Terminal do-script accepts shell input.
    let command = if iterm {
        format!("/bin/sh -lc {}", quote_shell(&command))
    } else {
        command
    };
    let literal = command
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    if iterm {
        format!("tell application id \"com.googlecode.iterm2\"\nactivate\ncreate window with default profile command \"{literal}\"\nend tell")
    } else {
        format!("tell application \"Terminal\"\nactivate\ndo script \"{literal}\"\nend tell")
    }
}
pub fn warp_configuration(folder: &str, executable: &str) -> serde_json::Value {
    serde_json::json!({"name":"Open Island", "windows":[{"tabs":[{
        "title":"Open Island", "layout":{"cwd":folder, "commands":[{"exec":quote_shell(executable)}]}
    }]}]})
}
pub fn warp_uri(name: &str) -> String {
    let encoded: String = name
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect();
    format!("warp://launch/{encoded}")
}

#[cfg(target_os = "macos")]
pub fn open(folder: &str, agent: &str, kind: &str) -> Result<(), String> {
    use std::{
        fs,
        io::Write,
        os::unix::fs::{DirBuilderExt, OpenOptionsExt},
        process::{Command, Stdio},
        time::{SystemTime, UNIX_EPOCH},
    };
    let agent = crate::launch::known_agent(agent)?;
    if !Path::new(folder).is_absolute() || !Path::new(folder).is_dir() {
        return Err("Selecione uma pasta válida.".into());
    }
    let executable = crate::terminal::on_path(agent)
        .ok_or_else(|| format!("Instale {agent} para abrir uma sessão."))?;
    let (bundle, name) = match kind {
        "terminal" => ("com.apple.Terminal", "Terminal"),
        "iterm2" => ("com.googlecode.iterm2", "iTerm2"),
        "warp" => ("dev.warp.Warp-Stable", "Warp"),
        "wezterm" => ("com.github.wez.wezterm", "WezTerm"),
        "kitty" => ("net.kovidgoyal.kitty", "Kitty"),
        _ => return Err("Terminal não suportado. Escolha outro em Ajustes → Geral.".into()),
    };
    let application = crate::platform::application_path(bundle).ok_or_else(|| {
        format!("{name} não está instalado. Instale-o ou escolha outro em Ajustes → Geral.")
    })?;
    if kind == "warp" {
        let home = std::env::var_os("HOME").ok_or("Pasta pessoal indisponível.")?;
        let directory = Path::new(&home).join(".warp/launch_configurations");
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&directory)
            .map_err(|e| e.to_string())?;
        // Keep each file independent so two rapid launches cannot replace each other's command.
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let file_name = format!("open-island-{}-{nonce}.yaml", std::process::id());
        let path = directory.join(&file_name);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| e.to_string())?;
        // JSON is a YAML subset and quotes every user-supplied scalar.
        file.write_all(
            serde_json::to_string(&warp_configuration(folder, &executable.to_string_lossy()))
                .map_err(|e| e.to_string())?
                .as_bytes(),
        )
        .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        let status = Command::new("/usr/bin/open")
            .args(["-b", bundle, &warp_uri(&file_name)])
            .status()
            .map_err(|e| e.to_string());
        if !status.as_ref().is_ok_and(|s| s.success()) {
            let _ = fs::remove_file(path);
            return Err("Não foi possível abrir a configuração de sessão no Warp.".into());
        }
        // Warp reads the file asynchronously, so it must outlive this request.
        return Ok(());
    }
    if matches!(kind, "wezterm" | "kitty") {
        let program = crate::terminal::on_path(kind)
            .unwrap_or_else(|| application.join("Contents/MacOS").join(kind));
        let mut argv = if kind == "wezterm" {
            vec!["start", "--always-new-process", "--"]
        } else {
            vec![]
        };
        argv.extend(["/bin/sh", "-c", "cd \"$1\" && exec \"$2\"", "sh", folder]);
        let mut child = Command::new(program)
            .args(argv)
            .arg(executable)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Não foi possível iniciar {name}: {e}"))?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        return Ok(());
    }
    let output = Command::new("/usr/bin/osascript")
        .args([
            "-e",
            &applescript(folder, &executable.to_string_lossy(), kind == "iterm2"),
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!("Não foi possível abrir {name}. Em Ajustes do Sistema → Privacidade e Segurança → Automação, permita que Open Island controle {name}. {}", String::from_utf8_lossy(&output.stderr).trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn warp_values_round_trip_without_becoming_yaml_structure_or_url_parameters() {
        let folder = "/tmp/ç a'\"\ncommands: bad";
        let config = warp_configuration(folder, "/tmp/bin/agent '$(touch bad)");
        let decoded: serde_json::Value = serde_json::from_str(&config.to_string()).unwrap();
        assert_eq!(decoded["windows"][0]["tabs"][0]["layout"]["cwd"], folder);
        assert_eq!(
            decoded["windows"][0]["tabs"][0]["layout"]["commands"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(warp_uri("a b&x.yaml"), "warp://launch/a%20b%26x.yaml");
    }
    #[test]
    fn iterm_creates_a_new_window_instead_of_writing_to_the_current_session() {
        let script = applescript("/tmp/a'\"\n", "/bin/claude", true);
        assert!(script.contains("create window with default profile command \"/bin/sh -lc "));
        assert!(!script.contains("current session"));
        assert_eq!(script.lines().count(), 4);
    }
}
