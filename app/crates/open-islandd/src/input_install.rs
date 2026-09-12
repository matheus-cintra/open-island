//! Reversible shell integration for agents launched outside the island.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
const START: &str = "# >>> open-island input >>>";
const END: &str = "# <<< open-island input <<<";
const MARKER: &str = "# managed-by open-island input";

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn sh_script(executable: &Path) -> String {
    let executable = quote(&executable.to_string_lossy());
    let mut script = format!("{MARKER}\ncase $- in *i*) ;; *) return ;; esac\n");
    for agent in open_island_core::discovery::AGENTS
        .iter()
        .flat_map(|agent| agent.proc_names)
    {
        script.push_str(&format!(
            r#"
if ! typeset -f {agent} >/dev/null 2>&1 && ! alias {agent} >/dev/null 2>&1; then
  {agent}() {{
    if [ -t 0 ] && [ -t 1 ] && [ -x {executable} ]; then
      case "${{1-}}" in
        --help|-h|--version|-v|--print|-p|exec|run|--headless) command {agent} "$@" ;;
        *) {executable} run -- {agent} "$@" ;;
      esac
    else
      command {agent} "$@"
    fi
  }}
fi
"#
        ));
    }
    script
}

fn fish_script(executable: &Path) -> String {
    // Fish single quotes escape only backslash and quote.
    let executable = format!(
        "'{}'",
        executable
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('\'', "\\'")
    );
    let mut script = format!("{MARKER}\nstatus is-interactive; or return\n");
    for agent in open_island_core::discovery::AGENTS
        .iter()
        .flat_map(|agent| agent.proc_names)
    {
        script.push_str(&format!(
            r#"
if not functions -q {agent}
  function {agent} --wraps {agent}
    if isatty stdin; and isatty stdout; and test -x {executable}
      switch "$argv[1]"
        case --help -h --version -v --print -p exec run --headless
          command {agent} $argv
        case '*'
          {executable} run -- {agent} $argv
      end
    else
      command {agent} $argv
    end
  end
end
"#
        ));
    }
    script
}

fn rc_content(old: &str, source: Option<&str>) -> Result<String, String> {
    let mut content = old.to_owned();
    if let Some(start) = content.find(START) {
        let end = content[start..].find(END).ok_or(
            "Bloco de entrada da ilha incompleto; preserve o arquivo e corrija os marcadores.",
        )? + start
            + END.len();
        let end = if content.as_bytes().get(end) == Some(&b'\n') {
            end + 1
        } else {
            end
        };
        content.replace_range(start..end, "");
        if content.contains(START) || content.contains(END) {
            return Err("Marcadores duplicados da entrada da ilha.".into());
        }
    } else if content.contains(END) {
        return Err("Bloco de entrada da ilha incompleto.".into());
    }
    if let Some(source) = source {
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&format!("{START}\nif [ -n \"${{BASH_VERSION-}}${{ZSH_VERSION-}}\" ] && [ -f {source} ]; then . {source}; fi\n{END}\n"));
    }
    Ok(content)
}

pub fn configure(
    home: &Path,
    executable: &Path,
    install: bool,
    dry_run: bool,
) -> Result<Vec<PathBuf>, String> {
    let script = home.join(".config/open-island/input.sh");
    let fish = home.join(".config/fish/conf.d/open-island-input.fish");
    let source = quote(&script.to_string_lossy());
    let mut plan = Vec::new();
    // Plan all writes first, so malformed markers cannot leave a partial setup.
    for (path, content) in [
        (&script, sh_script(executable)),
        (&fish, fish_script(executable)),
    ] {
        let old = match fs::read_to_string(path) {
            Ok(text) => Some(text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.to_string()),
        };
        if old.as_ref().is_some_and(|text| !text.starts_with(MARKER)) {
            return Err(format!(
                "Arquivo não gerenciado pela ilha: {}",
                path.display()
            ));
        }
        let new = install.then_some(content);
        if old != new {
            plan.push((path.clone(), new));
        }
    }
    // Creating .bash_profile would shadow an existing .profile/.bash_login.
    let login = [".bash_profile", ".bash_login", ".profile"]
        .into_iter()
        .find(|name| home.join(name).exists())
        .unwrap_or(".profile");
    for file in [
        ".bashrc",
        ".bash_profile",
        ".bash_login",
        ".profile",
        ".zshrc",
    ] {
        let original = home.join(file);
        let path = if original.exists() {
            fs::canonicalize(&original).map_err(|e| e.to_string())?
        } else {
            original
        };
        let old = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e.to_string()),
        };
        let active = file == ".bashrc" || file == ".zshrc" || file == login;
        let new = rc_content(&old, (install && active).then_some(source.as_str()))?;
        if old != new {
            plan.push((path, Some(new)));
        }
    }
    let paths = plan.iter().map(|(path, _)| path.clone()).collect();
    if !dry_run {
        for (path, content) in plan {
            if let Some(content) = content {
                let parent = path.parent().ok_or("Pasta de configuração indisponível")?;
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                let mut temp =
                    tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
                if let Ok(metadata) = path.metadata() {
                    temp.as_file()
                        .set_permissions(metadata.permissions())
                        .map_err(|e| e.to_string())?;
                }
                temp.write_all(content.as_bytes())
                    .map_err(|e| e.to_string())?;
                temp.as_file().sync_all().map_err(|e| e.to_string())?;
                temp.persist(&path).map_err(|e| e.to_string())?;
            } else {
                fs::remove_file(path).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(paths)
}

pub fn cli() -> Result<(), String> {
    let action = std::env::args()
        .nth(2)
        .ok_or("Uso: open-islandd input install|uninstall|status")?;
    let home = crate::installer::home_dir()?;
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    match action.as_str() {
        "status" => {
            let installed = configure(&home, &executable, true, true)?.is_empty();
            println!("{}", serde_json::json!({"installed": installed}));
        }
        "install" | "uninstall" => {
            for path in configure(
                &home,
                &executable,
                action == "install",
                std::env::args().any(|arg| arg == "--dry-run"),
            )? {
                println!("{}", path.display());
            }
        }
        _ => return Err("Uso: open-islandd input install|uninstall|status".into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installation_is_idempotent_and_removal_preserves_user_settings() {
        let home = tempfile::tempdir().unwrap();
        fs::write(home.path().join(".zshrc"), "export USER_SETTING=yes\n").unwrap();
        let exe = Path::new("/Applications/Open 'Island.app/Contents/MacOS/open-islandd");
        assert_eq!(configure(home.path(), exe, true, true).unwrap().len(), 5);
        assert!(!home.path().join(".bashrc").exists());
        configure(home.path(), exe, true, false).unwrap();
        assert!(configure(home.path(), exe, true, true).unwrap().is_empty());
        configure(home.path(), exe, false, false).unwrap();
        assert_eq!(
            fs::read_to_string(home.path().join(".zshrc")).unwrap(),
            "export USER_SETTING=yes\n"
        );
        assert!(configure(home.path(), exe, false, true).unwrap().is_empty());
    }
    #[test]
    fn malformed_markers_do_not_modify_any_file() {
        let home = tempfile::tempdir().unwrap();
        fs::write(home.path().join(".zshrc"), START).unwrap();
        assert!(configure(home.path(), Path::new("/bin/island"), true, false).is_err());
        assert!(!home.path().join(".config").exists());
    }

    #[test]
    fn shell_scripts_parse_and_preserve_existing_functions() {
        use std::process::{Command, Stdio};
        let home = tempfile::tempdir().unwrap();
        let script = home.path().join("input.sh");
        fs::write(&script, sh_script(Path::new("/tmp/a 'quoted' daemon"))).unwrap();
        for shell in ["bash", "zsh"] {
            let Some(executable) = open_island_core::paths::executable(shell) else {
                continue;
            };
            assert!(Command::new(&executable)
                .arg("-n")
                .arg(&script)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success());
            let result = Command::new(executable)
                .args([
                    "-fic",
                    "claude() { printf 'preserved:%s' \"$1\"; }; . \"$1\"; claude 'a b'",
                    "test",
                ])
                .arg(&script)
                .env("HOME", home.path())
                .stdin(Stdio::null())
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(String::from_utf8_lossy(&result.stdout), "preserved:a b");
        }
    }

    #[test]
    fn existing_profile_is_not_shadowed_and_uninstall_cleans_previous_login_files() {
        let home = tempfile::tempdir().unwrap();
        fs::write(home.path().join(".profile"), "export MY_PATH=yes\n").unwrap();
        let exe = Path::new("/bin/island");
        configure(home.path(), exe, true, false).unwrap();
        assert!(!home.path().join(".bash_profile").exists());
        fs::write(home.path().join(".bash_profile"), "# new user file\n").unwrap();
        configure(home.path(), exe, false, false).unwrap();
        assert_eq!(
            fs::read_to_string(home.path().join(".profile")).unwrap(),
            "export MY_PATH=yes\n"
        );
        assert_eq!(
            fs::read_to_string(home.path().join(".bash_profile")).unwrap(),
            "# new user file\n"
        );
    }
}
