use serde_json::{json, Value};
use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

const MANAGED_MARKER: &str = "--managed-by open-island";
const OPENCODE_START: &str = "// open-island-managed";
const OPENCODE_END: &str = "// end-open-island-managed";
const HYPR_START: &str = "-- open-island-managed";
const HYPR_END: &str = "-- end-open-island-managed";
const HYPR_REQUIRE: &str = "require(\"conf/open-island\")";
const HYPR_CONF_START: &str = "# open-island-managed";
const HYPR_CONF_END: &str = "# end-open-island-managed";
const HYPR_CONF_SOURCE: &str = "source = conf/open-island.conf";
const HYPR_CONF_MARKER: &str = "# managed-by open-island";
const SYSTEMD_MANAGED_MARKER: &str = "X-OpenIsland-Managed=1";
const DESKTOP_MANAGED_MARKER: &str = "X-OpenIsland-Managed=true";
const DAEMON_UNIT_NAME: &str = "open-islandd.service";
const PACKAGED_BIN_DIR: &str = "/usr/bin";
pub const DEFAULT_HOTKEY: &str = "SUPER + I";
const CODEX_EVENTS: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "Stop",
    "PermissionRequest",
];
const CLAUDE_EVENTS: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "Stop",
    "SubagentStop",
    "PermissionRequest",
];

/// Claude kills a hook after 60s by default, which is shorter than the 90s the daemon
/// waits on a permission or a question, so the blocking event carries its own ceiling.
const CLAUDE_BLOCKING_TIMEOUT_SECS: u64 = 120;

pub fn home_dir() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set".to_owned())
}

pub const AGENTS: &[&str] = &["claude", "codex", "opencode"];

pub fn selected_agents(value: &str) -> Result<Vec<&'static str>, String> {
    if value == "all" {
        return Ok(AGENTS.to_vec());
    }
    AGENTS
        .iter()
        .copied()
        .find(|agent| *agent == value)
        .map(|agent| vec![agent])
        .ok_or_else(|| format!("unsupported agent '{value}'"))
}

pub fn agent_path(home: &Path, agent: &str) -> Result<PathBuf, String> {
    match agent {
        "claude" => Ok(home.join(".claude/settings.json")),
        "codex" => Ok(home.join(".codex/hooks.json")),
        "opencode" => Ok(home.join(".config/opencode/plugins/open-island.ts")),
        other => Err(format!("unsupported agent '{other}'")),
    }
}

pub fn detected(home: &Path, agent: &str) -> bool {
    let root = match agent {
        "claude" => home.join(".claude"),
        "codex" => home.join(".codex"),
        "opencode" => home.join(".config/opencode"),
        _ => return false,
    };
    root.is_dir()
}

pub fn installed(home: &Path, agent: &str, executable: &Path) -> Result<bool, String> {
    install(home, &[agent], executable, true).map(|pending| pending.is_empty())
}

pub fn auto_configure(
    home: &Path,
    executable: &Path,
    known: &[String],
) -> (Vec<String>, Vec<String>) {
    let mut configured = Vec::new();
    let mut errors = Vec::new();
    for agent in AGENTS {
        if known.iter().any(|name| name == agent) || !detected(home, agent) {
            continue;
        }
        match install(home, &[agent], executable, false) {
            Ok(_) => configured.push((*agent).to_owned()),
            Err(error) => errors.push(format!("{agent}: {error}")),
        }
    }
    (configured, errors)
}

pub fn install(
    home: &Path,
    agents: &[&str],
    executable: &Path,
    dry_run: bool,
) -> Result<Vec<PathBuf>, String> {
    let mut changed = Vec::new();
    for agent in agents {
        let path = agent_path(home, agent)?;
        let did_change = match *agent {
            "claude" => {
                merge_agent_json(&path, CLAUDE_EVENTS, "claude", executable, true, dry_run)?
            }
            "codex" => merge_agent_json(&path, CODEX_EVENTS, "codex", executable, true, dry_run)?,
            "opencode" => merge_opencode_plugin(&path, executable, true, dry_run)?,
            _ => false,
        };
        if did_change {
            changed.push(path);
        }
    }
    Ok(changed)
}

pub fn uninstall(
    home: &Path,
    agents: &[&str],
    executable: &Path,
    dry_run: bool,
) -> Result<Vec<PathBuf>, String> {
    let mut changed = Vec::new();
    for agent in agents {
        let path = agent_path(home, agent)?;
        let did_change = match *agent {
            "claude" => {
                merge_agent_json(&path, CLAUDE_EVENTS, "claude", executable, false, dry_run)?
            }
            "codex" => merge_agent_json(&path, CODEX_EVENTS, "codex", executable, false, dry_run)?,
            "opencode" => merge_opencode_plugin(&path, executable, false, dry_run)?,
            _ => false,
        };
        if did_change {
            changed.push(path);
        }
    }
    Ok(changed)
}

/// The hotkey lives in a file of our own plus one `require` line, because `conf/binds.lua`
/// is hand-maintained and a bind appended there would be indistinguishable from the user's.
pub fn install_hotkey(
    home: &Path,
    executable: &Path,
    combo: &str,
    install: bool,
    dry_run: bool,
) -> Result<Vec<PathBuf>, String> {
    let plan = hypr_hotkey_plan(home, executable, combo)?;
    let mut changed = Vec::new();
    if merge_hypr_module(
        &plan.module,
        &plan.content,
        plan.start,
        plan.end,
        install,
        dry_run,
    )? {
        changed.push(plan.module);
    }
    if merge_hypr_require(&plan.entry, &plan.entry_line, install, dry_run)? {
        changed.push(plan.entry);
    }
    Ok(changed)
}

struct HyprHotkeyPlan {
    entry: PathBuf,
    module: PathBuf,
    content: String,
    start: &'static str,
    end: &'static str,
    entry_line: String,
}

fn hypr_hotkey_plan(home: &Path, executable: &Path, combo: &str) -> Result<HyprHotkeyPlan, String> {
    let lua_entry = home.join(".config/hypr/hyprland.lua");
    let conf_entry = home.join(".config/hypr/hyprland.conf");
    if lua_entry.exists() {
        return Ok(HyprHotkeyPlan {
            entry: lua_entry,
            module: home.join(".config/hypr/conf/open-island.lua"),
            content: hypr_lua_module(executable, combo)?,
            start: HYPR_START,
            end: HYPR_END,
            entry_line: format!("{HYPR_REQUIRE}  {MANAGED_MARKER}"),
        });
    }
    if conf_entry.exists() {
        return Ok(HyprHotkeyPlan {
            entry: conf_entry,
            module: home.join(".config/hypr/conf/open-island.conf"),
            content: hypr_conf_module(executable, combo)?,
            start: HYPR_CONF_START,
            end: HYPR_CONF_END,
            entry_line: format!("{HYPR_CONF_SOURCE}  {HYPR_CONF_MARKER}"),
        });
    }
    Err(format!(
        "neither {} nor {} was found: the hotkey installer only knows Hyprland's own config",
        lua_entry.display(),
        conf_entry.display()
    ))
}

fn merge_hypr_module(
    path: &Path,
    content: &str,
    start: &str,
    end: &str,
    install: bool,
    dry_run: bool,
) -> Result<bool, String> {
    let existing = path
        .exists()
        .then(|| fs::read_to_string(path))
        .transpose()
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    if !install {
        let Some(existing) = existing else {
            return Ok(false);
        };
        if !existing.contains(start) {
            return Err(format!("refusing to remove unrelated {}", path.display()));
        }
        if !dry_run {
            fs::remove_file(path).map_err(|error| format!("remove {}: {error}", path.display()))?;
        }
        return Ok(true);
    }
    if let Some(existing) = existing {
        if !existing.contains(start) || !existing.contains(end) {
            return Err(format!("refusing to replace unrelated {}", path.display()));
        }
        if existing == content {
            return Ok(false);
        }
    }
    if !dry_run {
        write_text_atomic(path, content)?;
    }
    Ok(true)
}

fn merge_hypr_require(
    path: &Path,
    line: &str,
    install: bool,
    dry_run: bool,
) -> Result<bool, String> {
    let existing =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let present = existing.lines().any(|entry| entry.trim() == line);
    if install == present {
        return Ok(false);
    }
    let content = if install {
        append_entry_line(&existing, line)
    } else {
        remove_entry_line(&existing, line)
    };
    if !dry_run {
        write_text_atomic(path, &content)?;
    }
    Ok(true)
}

fn append_entry_line(existing: &str, line: &str) -> String {
    if existing.is_empty() || existing.ends_with('\n') {
        format!("{existing}{line}\n")
    } else {
        format!("{existing}\n{line}")
    }
}

fn remove_entry_line(existing: &str, line: &str) -> String {
    let mut kept = String::with_capacity(existing.len());
    let mut rest = existing;
    while !rest.is_empty() {
        let (entry, tail) = match rest.find('\n') {
            Some(index) => rest.split_at(index + 1),
            None => (rest, ""),
        };
        if entry.trim() != line {
            kept.push_str(entry);
        } else if !entry.ends_with('\n') && kept.ends_with('\n') {
            kept.truncate(kept.len() - 1);
        }
        rest = tail;
    }
    kept
}

/// Two layers of quoting: the compositor hands the string to a shell, and the whole thing
/// then has to survive as a Lua literal.
fn hypr_lua_module(executable: &Path, combo: &str) -> Result<String, String> {
    let command = format!("{} toggle", shell_quote(&executable.to_string_lossy()));
    let command =
        serde_json::to_string(&command).map_err(|error| format!("encode command: {error}"))?;
    let combo = serde_json::to_string(combo).map_err(|error| format!("encode combo: {error}"))?;
    let template = include_str!("../templates/open-island-hypr.lua.template");
    Ok(template
        .replace("__OPEN_ISLAND_COMMAND__", &command)
        .replace("__OPEN_ISLAND_COMBO__", &combo))
}

fn hypr_conf_bind(combo: &str) -> Result<(String, String), String> {
    let tokens = combo
        .split('+')
        .flat_map(str::split_whitespace)
        .collect::<Vec<_>>();
    let Some((key, modifiers)) = tokens.split_last() else {
        return Err(format!("hotkey combo '{combo}' names no key"));
    };
    Ok((modifiers.join(" "), (*key).to_owned()))
}

fn hypr_conf_module(executable: &Path, combo: &str) -> Result<String, String> {
    let (modifiers, key) = hypr_conf_bind(combo)?;
    let command = format!("{} toggle", shell_quote(&executable.to_string_lossy()));
    let template = include_str!("../templates/open-island-hypr.conf.template");
    Ok(template
        .replace("__OPEN_ISLAND_MODIFIERS__", &modifiers)
        .replace("__OPEN_ISLAND_KEY__", &key)
        .replace("__OPEN_ISLAND_COMMAND__", &command))
}

pub fn hotkey_notes(combo: &str) -> Vec<String> {
    vec![
        format!("hyprland: {combo} toggles the island once the config is reloaded"),
        "hyprland: a settings window already on screen stays pinned until it is closed and \
         reopened, because Hyprland applies a window rule at creation"
            .to_owned(),
    ]
}

pub fn is_packaged_executable(executable: &Path) -> bool {
    executable.parent() == Some(Path::new(PACKAGED_BIN_DIR))
}

pub fn install_autostart(
    home: &Path,
    daemon_executable: &Path,
    island_executable: &Path,
    install: bool,
    dry_run: bool,
) -> Result<Vec<PathBuf>, String> {
    let units = home.join(".config/systemd/user");
    let mut files = vec![
        (
            units.join(DAEMON_UNIT_NAME),
            systemd_unit("Open Island daemon", daemon_executable, false),
            SYSTEMD_MANAGED_MARKER,
        ),
        (
            units.join("open-island.service"),
            systemd_unit("Open Island panel", island_executable, true),
            SYSTEMD_MANAGED_MARKER,
        ),
    ];
    if !(install && is_packaged_executable(daemon_executable)) {
        files.push((
            home.join(".local/share/applications/open-island-settings.desktop"),
            settings_desktop_entry(daemon_executable),
            DESKTOP_MANAGED_MARKER,
        ));
    }
    let mut changed = Vec::new();
    for (path, content, marker) in files {
        if merge_managed_file(&path, &content, marker, install, dry_run)? {
            changed.push(path);
        }
    }
    for (size, bytes) in ICONS {
        let path = icon_path(home, *size);
        if merge_managed_icon(&path, bytes, install, dry_run)? {
            changed.push(path);
        }
    }
    Ok(changed)
}

fn merge_managed_file(
    path: &Path,
    content: &str,
    marker: &str,
    install: bool,
    dry_run: bool,
) -> Result<bool, String> {
    let existing = path
        .exists()
        .then(|| fs::read_to_string(path))
        .transpose()
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    if !install {
        let Some(existing) = existing else {
            return Ok(false);
        };
        if !existing.contains(marker) {
            return Err(format!("refusing to remove unrelated {}", path.display()));
        }
        if !dry_run {
            fs::remove_file(path).map_err(|error| format!("remove {}: {error}", path.display()))?;
        }
        return Ok(true);
    }
    if let Some(existing) = existing {
        if !existing.contains(marker) {
            return Err(format!("refusing to replace unrelated {}", path.display()));
        }
        if existing == content {
            return Ok(false);
        }
    }
    if !dry_run {
        write_text_atomic(path, content)?;
    }
    Ok(true)
}

const ICONS: &[(u32, &[u8])] = &[
    (32, include_bytes!("../assets/open-island-32.png")),
    (48, include_bytes!("../assets/open-island-48.png")),
    (64, include_bytes!("../assets/open-island-64.png")),
    (128, include_bytes!("../assets/open-island-128.png")),
    (256, include_bytes!("../assets/open-island-256.png")),
];

fn icon_path(home: &Path, size: u32) -> PathBuf {
    home.join(format!(
        ".local/share/icons/hicolor/{size}x{size}/apps/open-island.png"
    ))
}

fn merge_managed_icon(
    path: &Path,
    bytes: &[u8],
    install: bool,
    dry_run: bool,
) -> Result<bool, String> {
    let existing = path
        .exists()
        .then(|| fs::read(path))
        .transpose()
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    let ours = existing
        .as_deref()
        .is_some_and(|found| ICONS.iter().any(|(_, icon)| found == *icon));
    if !install {
        let Some(_) = existing else {
            return Ok(false);
        };
        if !ours {
            return Err(format!("refusing to remove unrelated {}", path.display()));
        }
        if !dry_run {
            fs::remove_file(path).map_err(|error| format!("remove {}: {error}", path.display()))?;
        }
        return Ok(true);
    }
    if let Some(found) = existing {
        if !ours {
            return Err(format!("refusing to replace unrelated {}", path.display()));
        }
        if found == bytes {
            return Ok(false);
        }
    }
    if !dry_run {
        write_bytes_atomic(path, bytes)?;
    }
    Ok(true)
}

fn systemd_unit(description: &str, executable: &Path, after_daemon: bool) -> String {
    let daemon_dependency = if after_daemon {
        format!(" {DAEMON_UNIT_NAME}\nWants={DAEMON_UNIT_NAME}")
    } else {
        String::new()
    };
    let command = shell_quote(&executable.to_string_lossy());
    format!(
        "[Unit]\n\
         Description={description}\n\
         PartOf=graphical-session.target\n\
         After=graphical-session.target{daemon_dependency}\n\
         {SYSTEMD_MANAGED_MARKER}\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={command}\n\
         Restart=on-failure\n\
         RestartSec=2\n\
         Slice=app.slice\n\
         \n\
         [Install]\n\
         WantedBy=graphical-session.target\n"
    )
}

fn settings_desktop_entry(executable: &Path) -> String {
    let command = shell_quote(&executable.to_string_lossy());
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Open Island\n\
         Comment=Open the Open Island settings panel\n\
         Exec={command} settings\n\
         Icon=open-island\n\
         Terminal=false\n\
         Categories=Utility;\n\
         {DESKTOP_MANAGED_MARKER}\n"
    )
}

pub fn autostart_notes(install: bool) -> Vec<String> {
    if install {
        return vec![
            format!(
                "systemd: {DAEMON_UNIT_NAME} and open-island.service point at the executables \
                 named in their ExecStart lines; moving or deleting those files breaks login"
            ),
            "systemd: read the journal with 'journalctl --user -u open-islandd -u open-island'"
                .to_owned(),
        ];
    }
    vec![format!(
        "systemd: {DAEMON_UNIT_NAME} and open-island.service are gone; anything already running \
         keeps running until you stop it"
    )]
}

/// Manual steps the installer cannot perform, printed after a successful install.
pub fn install_notes(agents: &[&str]) -> Vec<String> {
    if !agents.contains(&"codex") {
        return Vec::new();
    }
    vec![
        "codex: the next Codex start shows \"Hooks need review\" - press t to trust the \
         open-island hooks, otherwise they stay installed but never run"
            .to_owned(),
        "codex: agent questions need experimental_request_user_input=true and \
         features.default_mode_request_user_input=true in ~/.codex/config.toml"
            .to_owned(),
    ]
}

fn command(executable: &Path, agent: &str) -> String {
    format!(
        "{} hook --agent {agent} {MANAGED_MARKER}",
        shell_quote(&executable.to_string_lossy())
    )
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn merge_agent_json(
    path: &Path,
    events: &[&str],
    agent: &str,
    executable: &Path,
    install: bool,
    dry_run: bool,
) -> Result<bool, String> {
    let mut root = if path.exists() {
        let text = fs::read_to_string(path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        serde_json::from_str::<Value>(&text)
            .map_err(|error| format!("parse {}: {error}", path.display()))?
    } else {
        json!({})
    };
    let object = root
        .as_object_mut()
        .ok_or_else(|| format!("{} must contain a JSON object", path.display()))?;
    let hooks = object
        .entry("hooks".to_owned())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| format!("{} hooks must contain a JSON object", path.display()))?;
    let managed = command(executable, agent);
    let mut changed = false;
    for event in events {
        if install {
            let groups = hooks
                .entry((*event).to_owned())
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or_else(|| {
                    format!(
                        "{} hook event {event} must contain an array",
                        path.display()
                    )
                })?;
            let present = groups
                .iter()
                .any(|group| group_has_command(group, &managed));
            if !present {
                let mut handler = json!({"type": "command", "command": managed});
                if agent == "claude" && *event == "PermissionRequest" {
                    handler["timeout"] = json!(CLAUDE_BLOCKING_TIMEOUT_SECS);
                }
                groups.push(json!({"matcher": "*", "hooks": [handler]}));
                changed = true;
            }
        } else if let Some(groups_value) = hooks.get_mut(*event) {
            let groups = groups_value.as_array_mut().ok_or_else(|| {
                format!(
                    "{} hook event {event} must contain an array",
                    path.display()
                )
            })?;
            let before = groups.len();
            let mut removed_handler = false;
            for group in groups.iter_mut() {
                if let Some(items) = group.get_mut("hooks").and_then(Value::as_array_mut) {
                    let item_count = items.len();
                    items.retain(|item| !is_managed_handler(item, agent));
                    removed_handler |= item_count != items.len();
                }
            }
            groups.retain(|group| {
                group
                    .get("hooks")
                    .and_then(Value::as_array)
                    .is_none_or(|items| !items.is_empty())
            });
            changed |= before != groups.len() || removed_handler;
            if groups.is_empty() {
                hooks.remove(*event);
            }
        }
    }
    if !changed {
        return Ok(false);
    }
    if !dry_run {
        write_json_atomic(path, &root)?;
    }
    Ok(true)
}

fn group_has_command(group: &Value, command: &str) -> bool {
    group
        .get("hooks")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items.iter().any(|item| {
                item.get("type").and_then(Value::as_str) == Some("command")
                    && item.get("command").and_then(Value::as_str) == Some(command)
            })
        })
}

fn is_managed_handler(item: &Value, agent: &str) -> bool {
    item.get("type").and_then(Value::as_str) == Some("command")
        && item
            .get("command")
            .and_then(Value::as_str)
            .is_some_and(|value| {
                value.contains(MANAGED_MARKER) && value.contains(&format!("--agent {agent}"))
            })
}

fn merge_opencode_plugin(
    path: &Path,
    executable: &Path,
    install: bool,
    dry_run: bool,
) -> Result<bool, String> {
    let existing = path
        .exists()
        .then(|| fs::read_to_string(path))
        .transpose()
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    if install {
        let content = opencode_plugin(executable)?;
        if let Some(existing) = existing {
            if !existing.contains(OPENCODE_START) || !existing.contains(OPENCODE_END) {
                return Err(format!("refusing to replace unrelated {}", path.display()));
            }
            if existing == content {
                return Ok(false);
            }
        }
        if !dry_run {
            write_text_atomic(path, &content)?;
        }
        return Ok(true);
    }
    let Some(existing) = existing else {
        return Ok(false);
    };
    let Some(start) = existing.find(OPENCODE_START) else {
        return Ok(false);
    };
    let Some(end_relative) = existing[start..].find(OPENCODE_END) else {
        return Err(format!("incomplete managed plugin {}", path.display()));
    };
    let end = start + end_relative + OPENCODE_END.len();
    let remaining = format!("{}{}", &existing[..start], &existing[end..]);
    if !dry_run {
        if remaining.trim().is_empty() {
            fs::remove_file(path).map_err(|error| format!("remove {}: {error}", path.display()))?;
        } else {
            write_text_atomic(path, &remaining)?;
        }
    }
    Ok(true)
}

fn opencode_plugin(executable: &Path) -> Result<String, String> {
    let path = serde_json::to_string(&executable.to_string_lossy().to_string())
        .map_err(|error| format!("encode daemon path: {error}"))?;
    let template = include_str!("../templates/open-island-opencode.ts.template");
    Ok(template.replace("__OPEN_ISLANDD_PATH__", &path))
}

fn write_json_atomic(path: &Path, value: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| format!("serialize {}: {error}", path.display()))?;
    write_text_atomic(path, &format!("{text}\n"))
}

fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent", path.display()))?;
    fs::create_dir_all(parent).map_err(|error| format!("create {}: {error}", parent.display()))?;
    let temp = parent.join(format!(".open-island-{}.tmp", std::process::id()));
    let result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(|error| format!("write {}: {error}", path.display()))
}

fn write_text_atomic(path: &Path, text: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent", path.display()))?;
    fs::create_dir_all(parent).map_err(|error| format!("create {}: {error}", parent.display()))?;
    let temp = parent.join(format!(".open-island-{}.tmp", std::process::id()));
    let result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&temp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(|error| format!("write {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn home() -> PathBuf {
        env::temp_dir().join(format!(
            "open-island-installer-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    #[test]
    fn json_install_is_idempotent_and_preserves_unrelated_hooks() {
        let root = home();
        let path = root.join(".claude/settings.json");
        fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        fs::write(
            &path,
            r#"{"name":"keep","hooks":{"SessionStart":[{"matcher":"*","hooks":[{"type":"command","command":"keep-me"}]}]}}"#,
        )
        .expect("seed");
        let executable = Path::new("/opt/open-islandd");
        assert!(
            merge_agent_json(&path, CLAUDE_EVENTS, "claude", executable, true, false)
                .expect("install")
        );
        let first = fs::read(&path).expect("read");
        assert!(
            !merge_agent_json(&path, CLAUDE_EVENTS, "claude", executable, true, false)
                .expect("repeat")
        );
        assert_eq!(first, fs::read(&path).expect("read repeat"));
        let text = String::from_utf8(first).expect("utf8");
        assert!(text.contains("keep-me"));
        assert!(
            merge_agent_json(&path, CLAUDE_EVENTS, "claude", executable, false, false)
                .expect("uninstall")
        );
        let final_text = fs::read_to_string(&path).expect("read final");
        assert!(final_text.contains("keep-me"));
        assert!(!final_text.contains(MANAGED_MARKER));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn malformed_json_is_not_truncated() {
        let root = home();
        let path = root.join(".codex/hooks.json");
        fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        fs::write(&path, "{").expect("seed");
        let result = merge_agent_json(
            &path,
            CODEX_EVENTS,
            "codex",
            Path::new("/opt/open-islandd"),
            true,
            false,
        );
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&path).expect("read"), "{");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn opencode_plugin_install_repeat_and_uninstall_are_safe() {
        let root = home();
        let path = root.join(".config/opencode/plugins/open-island.ts");
        let executable = Path::new("/opt/open-islandd");
        assert!(merge_opencode_plugin(&path, executable, true, false).expect("install"));
        let first = fs::read(&path).expect("read");
        assert!(!merge_opencode_plugin(&path, executable, true, false).expect("repeat"));
        assert_eq!(first, fs::read(&path).expect("read repeat"));
        assert!(merge_opencode_plugin(&path, executable, false, false).expect("uninstall"));
        assert!(!path.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn the_blocking_claude_hook_outlives_claude_default_60s_ceiling() {
        let root = home();
        let path = root.join(".claude/settings.json");
        merge_agent_json(
            &path,
            CLAUDE_EVENTS,
            "claude",
            Path::new("/opt/open-islandd"),
            true,
            false,
        )
        .expect("install");
        let settings: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("JSON");
        let hooks = &settings["hooks"];

        assert_eq!(
            hooks["PermissionRequest"][0]["hooks"][0]["timeout"],
            json!(CLAUDE_BLOCKING_TIMEOUT_SECS),
            "a question or a permission can wait 90s on the island"
        );
        for event in ["SessionStart", "PreToolUse", "PostToolUse"] {
            assert!(
                hooks[event][0]["hooks"][0].get("timeout").is_none(),
                "{event} never blocks, so it keeps Claude's default"
            );
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn the_stop_hook_is_installed_for_both_blocking_agents() {
        let root = home();
        for (agent, relative) in [
            ("claude", ".claude/settings.json"),
            ("codex", ".codex/hooks.json"),
        ] {
            let path = root.join(relative);
            merge_agent_json(
                &path,
                CLAUDE_EVENTS,
                agent,
                Path::new("/opt/open-islandd"),
                true,
                false,
            )
            .expect("install");
            let settings: Value =
                serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("JSON");

            assert!(
                settings["hooks"]["Stop"][0]["hooks"][0]["command"]
                    .as_str()
                    .is_some_and(|command| command.contains("open-islandd")),
                "{agent} needs the Stop hook: it is the only edge that says the agent finished"
            );
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn an_agent_is_detected_by_its_own_directory_and_not_by_our_hook_file() {
        let root = home();
        assert!(!detected(&root, "claude"));
        assert!(!detected(&root, "codex"));
        assert!(!detected(&root, "opencode"));
        assert!(!detected(&root, "cursor"));

        fs::create_dir_all(root.join(".claude")).expect("claude");
        fs::create_dir_all(root.join(".config/opencode")).expect("opencode");

        assert!(detected(&root, "claude"));
        assert!(!detected(&root, "codex"));
        assert!(detected(&root, "opencode"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn auto_configure_installs_every_detected_agent_once_and_never_returns_to_it() {
        let root = home();
        let executable = Path::new("/opt/open-islandd");
        fs::create_dir_all(root.join(".claude")).expect("claude");
        fs::create_dir_all(root.join(".codex")).expect("codex");

        let (configured, errors) = auto_configure(&root, executable, &[]);
        assert_eq!(configured, vec!["claude".to_owned(), "codex".to_owned()]);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(installed(&root, "claude", executable).expect("status"));
        assert!(installed(&root, "codex", executable).expect("status"));
        assert!(!detected(&root, "opencode"));

        let (again, errors) = auto_configure(&root, executable, &configured);
        assert!(again.is_empty(), "a known agent is never configured twice");
        assert!(errors.is_empty(), "{errors:?}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn an_agent_the_user_turned_off_stays_off_across_a_restart() {
        let root = home();
        let executable = Path::new("/opt/open-islandd");
        fs::create_dir_all(root.join(".claude")).expect("claude");
        let known = auto_configure(&root, executable, &[]).0;
        uninstall(&root, &["claude"], executable, false).expect("uninstall");
        assert!(!installed(&root, "claude", executable).expect("status"));

        let (configured, _) = auto_configure(&root, executable, &known);

        assert!(configured.is_empty());
        assert!(
            !installed(&root, "claude", executable).expect("status"),
            "the next start put back a hook the user had removed"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn installing_codex_reports_the_two_steps_the_installer_cannot_do() {
        let notes = install_notes(&["codex"]);
        assert_eq!(notes.len(), 2);
        assert!(notes[0].contains("press t"), "{}", notes[0]);
        assert!(notes[0].contains("Hooks need review"), "{}", notes[0]);
        assert!(
            notes[1].contains("experimental_request_user_input=true"),
            "{}",
            notes[1]
        );
        assert!(install_notes(&["claude", "opencode"]).is_empty());
        assert_eq!(install_notes(&["claude", "codex", "opencode"]).len(), 2);
    }

    #[test]
    fn dry_run_does_not_create_user_config() {
        let root = home();
        let paths = install(
            &root,
            &["claude", "codex", "opencode"],
            Path::new("/opt/open-islandd"),
            true,
        )
        .expect("dry run");
        assert_eq!(paths.len(), 3);
        assert!(!root.exists());
    }

    #[test]
    fn installing_the_hotkey_writes_its_own_file_and_one_require_line() {
        let root = home();
        let entry = root.join(".config/hypr/hyprland.lua");
        fs::create_dir_all(entry.parent().expect("parent")).expect("directory");
        let original = "require(\"conf/binds\")\nrequire(\"conf/rules\")\n";
        fs::write(&entry, original).expect("seed");
        let executable = Path::new("/opt/open-islandd");

        let changed =
            install_hotkey(&root, executable, DEFAULT_HOTKEY, true, false).expect("install");
        assert_eq!(changed.len(), 2);
        let module = fs::read_to_string(root.join(".config/hypr/conf/open-island.lua"))
            .expect("read module");
        assert!(module.contains(HYPR_START) && module.contains(HYPR_END));
        assert!(module.contains("\"SUPER + I\""));
        assert!(module.contains("'/opt/open-islandd' toggle"));
        assert!(
            module.contains("^Open Island Settings$") && module.contains("pin = false"),
            "the settings window has to escape the class rule that pins the island: {module}"
        );
        let after = fs::read_to_string(&entry).expect("read entry");
        assert!(after.starts_with(original));
        assert_eq!(after.matches(HYPR_REQUIRE).count(), 1);

        assert!(
            install_hotkey(&root, executable, DEFAULT_HOTKEY, true, false)
                .expect("repeat")
                .is_empty()
        );
        assert_eq!(fs::read_to_string(&entry).expect("read repeat"), after);

        assert_eq!(
            install_hotkey(&root, executable, DEFAULT_HOTKEY, false, false)
                .expect("uninstall")
                .len(),
            2
        );
        assert!(!root.join(".config/hypr/conf/open-island.lua").exists());
        assert_eq!(fs::read_to_string(&entry).expect("read final"), original);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn installing_the_hotkey_refuses_a_config_it_does_not_recognise() {
        let root = home();
        fs::create_dir_all(root.join(".config/hypr")).expect("directory");
        let error = install_hotkey(
            &root,
            Path::new("/opt/open-islandd"),
            DEFAULT_HOTKEY,
            true,
            false,
        )
        .expect_err("must refuse");
        assert!(error.contains("hyprland.lua"), "{error}");
        assert!(error.contains("hyprland.conf"), "{error}");
        assert!(!root.join(".config/hypr/conf").exists());
        assert_eq!(
            fs::read_dir(root.join(".config/hypr"))
                .expect("read dir")
                .count(),
            0
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn installing_the_hotkey_writes_its_own_conf_file_and_one_source_line() {
        let root = home();
        let entry = root.join(".config/hypr/hyprland.conf");
        fs::create_dir_all(entry.parent().expect("parent")).expect("directory");
        let original = "source = conf/binds.conf\nsource = conf/rules.conf\n";
        fs::write(&entry, original).expect("seed");
        let executable = Path::new("/opt/open-islandd");

        let changed =
            install_hotkey(&root, executable, DEFAULT_HOTKEY, true, false).expect("install");
        assert_eq!(changed.len(), 2);
        let module = fs::read_to_string(root.join(".config/hypr/conf/open-island.conf"))
            .expect("read module");
        assert!(module.starts_with(HYPR_CONF_START));
        assert!(module.trim_end().ends_with(HYPR_CONF_END));
        assert!(
            module.contains("bind = SUPER, I, exec, '/opt/open-islandd' toggle"),
            "{module}"
        );
        assert!(
            module.contains("windowrule = match:title ^Open Island Settings$")
                && module.contains("pin false"),
            "the settings window has to escape the class rule that pins the island: {module}"
        );
        assert!(!module.contains("windowrulev2"), "{module}");
        assert!(!root.join(".config/hypr/conf/open-island.lua").exists());
        let after = fs::read_to_string(&entry).expect("read entry");
        assert!(after.starts_with(original));
        assert_eq!(after.matches(HYPR_CONF_SOURCE).count(), 1);

        assert!(
            install_hotkey(&root, executable, DEFAULT_HOTKEY, true, false)
                .expect("repeat")
                .is_empty()
        );
        assert_eq!(fs::read_to_string(&entry).expect("read repeat"), after);

        assert_eq!(
            install_hotkey(&root, executable, DEFAULT_HOTKEY, false, false)
                .expect("uninstall")
                .len(),
            2
        );
        assert!(!root.join(".config/hypr/conf/open-island.conf").exists());
        assert_eq!(fs::read_to_string(&entry).expect("read final"), original);
        let _ = fs::remove_dir_all(root);
    }

    fn entry_round_trip(entry_name: &str, original: &str) -> String {
        let root = home();
        let entry = root.join(".config/hypr").join(entry_name);
        fs::create_dir_all(entry.parent().expect("parent")).expect("directory");
        fs::write(&entry, original).expect("seed");
        let executable = Path::new("/opt/open-islandd");

        assert_eq!(
            install_hotkey(&root, executable, DEFAULT_HOTKEY, true, false)
                .expect("install")
                .len(),
            2
        );
        let installed = fs::read_to_string(&entry).expect("read installed");
        assert!(
            installed.starts_with(original),
            "{entry_name}: the install rewrote bytes it did not own: {installed:?}"
        );
        assert_eq!(
            install_hotkey(&root, executable, DEFAULT_HOTKEY, false, false)
                .expect("uninstall")
                .len(),
            2
        );
        assert_eq!(
            fs::read_to_string(&entry).expect("read final"),
            original,
            "{entry_name} did not come back byte for byte"
        );
        let _ = fs::remove_dir_all(root);
        installed
    }

    #[test]
    fn an_entry_file_without_a_trailing_newline_comes_back_byte_for_byte() {
        let lua = entry_round_trip("hyprland.lua", "require(\"conf/binds\")");
        assert!(lua.ends_with(MANAGED_MARKER), "{lua:?}");
        let conf = entry_round_trip("hyprland.conf", "source = conf/binds.conf");
        assert!(conf.ends_with(HYPR_CONF_MARKER), "{conf:?}");
    }

    #[test]
    fn an_entry_file_ending_in_two_newlines_comes_back_byte_for_byte() {
        let lua = entry_round_trip("hyprland.lua", "require(\"conf/binds\")\n\n");
        assert!(lua.starts_with("require(\"conf/binds\")\n\n"), "{lua:?}");
        entry_round_trip("hyprland.conf", "source = conf/binds.conf\n\n");
    }

    #[test]
    fn an_entry_file_ending_in_a_bare_carriage_return_comes_back_byte_for_byte() {
        let lua = entry_round_trip("hyprland.lua", "require(\"conf/binds\")\r");
        assert!(lua.starts_with("require(\"conf/binds\")\r\n"), "{lua:?}");
        entry_round_trip("hyprland.conf", "source = conf/binds.conf\r");
    }

    #[test]
    fn an_entry_file_whose_only_line_is_ours_comes_back_empty() {
        let lua = entry_round_trip("hyprland.lua", "");
        assert_eq!(lua, format!("{HYPR_REQUIRE}  {MANAGED_MARKER}\n"));
        let conf = entry_round_trip("hyprland.conf", "");
        assert_eq!(conf, format!("{HYPR_CONF_SOURCE}  {HYPR_CONF_MARKER}\n"));
    }

    #[test]
    fn the_conf_hotkey_splits_every_modifier_off_the_key() {
        assert_eq!(
            hypr_conf_bind("SUPER + I").expect("default"),
            ("SUPER".to_owned(), "I".to_owned())
        );
        assert_eq!(
            hypr_conf_bind("SUPER + SHIFT + K").expect("lua shaped"),
            ("SUPER SHIFT".to_owned(), "K".to_owned())
        );
        assert_eq!(
            hypr_conf_bind("SUPER SHIFT + K").expect("space shaped"),
            ("SUPER SHIFT".to_owned(), "K".to_owned())
        );
        assert_eq!(
            hypr_conf_bind("F12").expect("bare key"),
            (String::new(), "F12".to_owned())
        );
        assert!(hypr_conf_bind("  ").is_err());
    }

    #[test]
    fn installing_the_hotkey_prefers_the_lua_entry_when_both_exist() {
        let root = home();
        fs::create_dir_all(root.join(".config/hypr")).expect("directory");
        fs::write(root.join(".config/hypr/hyprland.lua"), "").expect("seed lua");
        let conf = root.join(".config/hypr/hyprland.conf");
        fs::write(&conf, "source = conf/binds.conf\n").expect("seed conf");

        install_hotkey(
            &root,
            Path::new("/opt/open-islandd"),
            DEFAULT_HOTKEY,
            true,
            false,
        )
        .expect("install");
        assert!(root.join(".config/hypr/conf/open-island.lua").exists());
        assert!(!root.join(".config/hypr/conf/open-island.conf").exists());
        assert_eq!(
            fs::read_to_string(&conf).expect("read conf"),
            "source = conf/binds.conf\n"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn installing_the_hotkey_refuses_a_lua_module_that_is_not_ours() {
        let root = home();
        fs::create_dir_all(root.join(".config/hypr/conf")).expect("directory");
        fs::write(root.join(".config/hypr/hyprland.lua"), "").expect("seed entry");
        let module = root.join(".config/hypr/conf/open-island.lua");
        fs::write(&module, "hl.bind(\"SUPER + P\", nil)\n").expect("seed module");

        let error = install_hotkey(
            &root,
            Path::new("/opt/open-islandd"),
            DEFAULT_HOTKEY,
            true,
            false,
        )
        .expect_err("must refuse");
        assert!(error.contains("refusing to replace unrelated"), "{error}");
        assert_eq!(
            fs::read_to_string(&module).expect("read module"),
            "hl.bind(\"SUPER + P\", nil)\n"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn installing_the_hotkey_refuses_a_conf_module_that_is_not_ours() {
        let root = home();
        fs::create_dir_all(root.join(".config/hypr/conf")).expect("directory");
        fs::write(root.join(".config/hypr/hyprland.conf"), "").expect("seed entry");
        let module = root.join(".config/hypr/conf/open-island.conf");
        fs::write(&module, "bind = SUPER, P, exec, mine\n").expect("seed module");

        let error = install_hotkey(
            &root,
            Path::new("/opt/open-islandd"),
            DEFAULT_HOTKEY,
            true,
            false,
        )
        .expect_err("must refuse");
        assert!(error.contains("refusing to replace unrelated"), "{error}");
        assert_eq!(
            fs::read_to_string(&module).expect("read module"),
            "bind = SUPER, P, exec, mine\n"
        );
        let _ = fs::remove_dir_all(root);
    }

    fn autostart_paths(root: &Path) -> Vec<PathBuf> {
        let mut paths = vec![
            root.join(".config/systemd/user/open-islandd.service"),
            root.join(".config/systemd/user/open-island.service"),
            root.join(".local/share/applications/open-island-settings.desktop"),
        ];
        paths.extend(ICONS.iter().map(|(size, _)| icon_path(root, *size)));
        paths
    }

    #[test]
    fn the_desktop_entry_names_an_icon_the_install_actually_writes() {
        let root = home();
        install_autostart(
            &root,
            Path::new("/opt/open-islandd"),
            Path::new("/opt/open-island"),
            true,
            false,
        )
        .expect("install");
        let entry =
            fs::read_to_string(root.join(".local/share/applications/open-island-settings.desktop"))
                .expect("read desktop entry");
        assert!(entry.contains("Icon=open-island"), "{entry}");
        for (size, bytes) in ICONS {
            let path = icon_path(&root, *size);
            assert_eq!(
                fs::read(&path).expect("read icon"),
                *bytes,
                "the theme lookup for Icon=open-island has to find {}",
                path.display()
            );
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn an_icon_that_is_not_ours_is_never_replaced_or_removed() {
        let root = home();
        let path = icon_path(&root, 256);
        fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        fs::write(&path, b"somebody else's icon").expect("seed");

        for install in [true, false] {
            let error = install_autostart(
                &root,
                Path::new("/opt/open-islandd"),
                Path::new("/opt/open-island"),
                install,
                false,
            )
            .expect_err("must refuse");
            assert!(error.contains("open-island.png"), "{error}");
        }
        assert_eq!(fs::read(&path).expect("read"), b"somebody else's icon");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn installing_autostart_writes_three_managed_files_naming_their_executables() {
        let root = home();
        let changed = install_autostart(
            &root,
            Path::new("/opt/open-islandd"),
            Path::new("/opt/open-island"),
            true,
            false,
        )
        .expect("install");
        assert_eq!(changed, autostart_paths(&root));

        let daemon = fs::read_to_string(&changed[0]).expect("read daemon unit");
        assert!(daemon.contains(SYSTEMD_MANAGED_MARKER), "{daemon}");
        assert!(
            daemon.contains("ExecStart='/opt/open-islandd'\n"),
            "{daemon}"
        );
        assert!(
            daemon.contains("WantedBy=graphical-session.target"),
            "{daemon}"
        );

        let island = fs::read_to_string(&changed[1]).expect("read island unit");
        assert!(island.contains(SYSTEMD_MANAGED_MARKER), "{island}");
        assert!(
            island.contains("ExecStart='/opt/open-island'\n"),
            "{island}"
        );

        let desktop = fs::read_to_string(&changed[2]).expect("read desktop entry");
        assert!(desktop.contains(DESKTOP_MANAGED_MARKER), "{desktop}");
        assert!(
            desktop.contains("Exec='/opt/open-islandd' settings\n"),
            "{desktop}"
        );
        assert!(desktop.contains("Type=Application"), "{desktop}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn the_island_unit_orders_itself_after_the_daemon_unit() {
        let root = home();
        install_autostart(
            &root,
            Path::new("/opt/open-islandd"),
            Path::new("/opt/open-island"),
            true,
            false,
        )
        .expect("install");
        let island = fs::read_to_string(root.join(".config/systemd/user/open-island.service"))
            .expect("read");

        assert!(
            island.contains("After=graphical-session.target open-islandd.service\n"),
            "the island talks to the daemon over a socket: {island}"
        );
        assert!(island.contains("Wants=open-islandd.service\n"), "{island}");
        let daemon = fs::read_to_string(root.join(".config/systemd/user/open-islandd.service"))
            .expect("read daemon");
        assert!(!daemon.contains("Wants="), "{daemon}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn installing_autostart_a_second_time_reports_no_change() {
        let root = home();
        let daemon_executable = Path::new("/opt/open-islandd");
        let island_executable = Path::new("/opt/open-island");
        install_autostart(&root, daemon_executable, island_executable, true, false)
            .expect("install");
        let first = autostart_paths(&root)
            .into_iter()
            .map(|path| fs::read(path).expect("read"))
            .collect::<Vec<_>>();

        assert!(
            install_autostart(&root, daemon_executable, island_executable, true, false)
                .expect("repeat")
                .is_empty()
        );
        assert_eq!(
            first,
            autostart_paths(&root)
                .into_iter()
                .map(|path| fs::read(path).expect("read repeat"))
                .collect::<Vec<_>>()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn uninstalling_autostart_removes_exactly_the_three_files_it_wrote() {
        let root = home();
        let daemon_executable = Path::new("/opt/open-islandd");
        let island_executable = Path::new("/opt/open-island");
        let unrelated = root.join(".config/systemd/user/swaync.service");
        install_autostart(&root, daemon_executable, island_executable, true, false)
            .expect("install");
        fs::write(&unrelated, "[Unit]\nDescription=someone else\n").expect("seed");

        let removed = install_autostart(&root, daemon_executable, island_executable, false, false)
            .expect("uninstall");
        assert_eq!(removed, autostart_paths(&root));
        for path in autostart_paths(&root) {
            assert!(!path.exists(), "{}", path.display());
        }
        assert!(unrelated.exists());
        assert!(
            install_autostart(&root, daemon_executable, island_executable, false, false)
                .expect("repeat")
                .is_empty()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn uninstalling_autostart_refuses_a_unit_it_does_not_recognise() {
        let root = home();
        let unit = root.join(".config/systemd/user/open-islandd.service");
        fs::create_dir_all(unit.parent().expect("parent")).expect("directory");
        let original = "[Unit]\nDescription=handwritten\n\n[Service]\nExecStart=/bin/true\n";
        fs::write(&unit, original).expect("seed");

        let error = install_autostart(
            &root,
            Path::new("/opt/open-islandd"),
            Path::new("/opt/open-island"),
            false,
            false,
        )
        .expect_err("must refuse");
        assert!(error.contains("open-islandd.service"), "{error}");
        assert_eq!(fs::read_to_string(&unit).expect("read"), original);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_dry_run_autostart_install_reports_every_path_and_writes_nothing() {
        let root = home();
        let daemon_executable = Path::new("/opt/open-islandd");
        let island_executable = Path::new("/opt/open-island");

        let planned = install_autostart(&root, daemon_executable, island_executable, true, true)
            .expect("dry run");
        assert_eq!(planned, autostart_paths(&root));
        assert!(!root.exists());

        let written = install_autostart(&root, daemon_executable, island_executable, true, false)
            .expect("install");
        assert_eq!(planned, written);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn only_a_direct_child_of_usr_bin_counts_as_the_packaged_layout() {
        assert!(is_packaged_executable(Path::new("/usr/bin/open-islandd")));
        assert!(!is_packaged_executable(Path::new(
            "/usr/bin-something/open-islandd"
        )));
        assert!(!is_packaged_executable(Path::new(
            "/usr/local/bin/open-islandd"
        )));
        assert!(!is_packaged_executable(Path::new(
            "/usr/bin/nested/open-islandd"
        )));
    }

    #[test]
    fn a_packaged_install_writes_the_units_but_no_settings_desktop_entry() {
        let root = home();
        let changed = install_autostart(
            &root,
            Path::new("/usr/bin/open-islandd"),
            Path::new("/usr/bin/open-island"),
            true,
            false,
        )
        .expect("install");
        let entry = root.join(".local/share/applications/open-island-settings.desktop");
        assert!(!changed.contains(&entry), "{changed:?}");
        assert!(!entry.exists());
        let daemon_unit =
            fs::read_to_string(root.join(".config/systemd/user/open-islandd.service"))
                .expect("read daemon unit");
        assert!(
            daemon_unit.contains("ExecStart='/usr/bin/open-islandd'"),
            "{daemon_unit}"
        );
        let island_unit = fs::read_to_string(root.join(".config/systemd/user/open-island.service"))
            .expect("read island unit");
        assert!(
            island_unit.contains("ExecStart='/usr/bin/open-island'"),
            "{island_unit}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_build_tree_install_still_writes_the_settings_desktop_entry() {
        let root = home();
        let changed = install_autostart(
            &root,
            Path::new("/usr/bin-something/open-islandd"),
            Path::new("/usr/bin-something/open-island"),
            true,
            false,
        )
        .expect("install");
        let entry = root.join(".local/share/applications/open-island-settings.desktop");
        assert!(changed.contains(&entry), "{changed:?}");
        assert!(entry.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_packaged_uninstall_still_removes_a_settings_entry_a_build_tree_install_left() {
        let root = home();
        install_autostart(
            &root,
            Path::new("/opt/open-islandd"),
            Path::new("/opt/open-island"),
            true,
            false,
        )
        .expect("install");
        let entry = root.join(".local/share/applications/open-island-settings.desktop");
        assert!(entry.exists());
        let removed = install_autostart(
            &root,
            Path::new("/usr/bin/open-islandd"),
            Path::new("/usr/bin/open-island"),
            false,
            false,
        )
        .expect("uninstall");
        assert!(removed.contains(&entry), "{removed:?}");
        assert!(!entry.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn autostart_notes_never_tell_the_user_to_run_what_the_command_already_ran() {
        for notes in [autostart_notes(true), autostart_notes(false)] {
            assert!(!notes.is_empty());
            for note in notes {
                assert!(
                    !note.contains("run 'systemctl"),
                    "the CLI runs systemctl itself, so a note must not ask for it again: {note}"
                );
            }
        }
        assert!(autostart_notes(true)[0].contains("point at the executables"));
    }

    #[test]
    fn unrelated_opencode_plugin_is_never_replaced() {
        let root = home();
        let path = root.join(".config/opencode/plugins/open-island.ts");
        fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        fs::write(&path, "export default {};\n").expect("seed");
        let result = merge_opencode_plugin(&path, Path::new("/opt/open-islandd"), true, false);
        assert!(result.is_err());
        assert_eq!(
            fs::read_to_string(&path).expect("read"),
            "export default {};\n"
        );
        let _ = fs::remove_dir_all(root);
    }
}
