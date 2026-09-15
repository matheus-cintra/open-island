use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs::OpenOptions, io::Read, os::unix::fs::OpenOptionsExt, path::Path};
#[derive(Debug, Serialize, Deserialize)]
pub struct HookStatus {
    pub agent: String,
    pub managed: bool,
    pub state: String,
}
fn json_state(text: &str, agent: &str) -> (bool, &'static str) {
    let Ok(root) = serde_json::from_str::<Value>(text) else {
        return (false, "conflict");
    };
    if !root.is_object() {
        return (false, "conflict");
    }
    let Some(hooks) = root.get("hooks") else {
        return (false, "missing");
    };
    let Some(events) = hooks.as_object() else {
        return (false, "conflict");
    };
    let expected = crate::hook_templates::command(agent);
    let mut managed = false;
    let mut legacy = false;
    let mut conflict = false;
    for groups in events.values() {
        let Some(groups) = groups.as_array() else {
            conflict = true;
            continue;
        };
        let mut count = 0;
        for item in groups
            .iter()
            .filter_map(|group| group.get("hooks").and_then(Value::as_array))
            .flatten()
        {
            let Some(command) = item.get("command").and_then(Value::as_str) else {
                continue;
            };
            if !command.contains("open-island") {
                continue;
            }
            count += 1;
            if command.contains("--managed-by open-island")
                && command.contains(&format!("--agent {agent}"))
            {
                managed = true;
                legacy |= command != expected;
            } else {
                conflict = true;
            }
        }
        conflict |= count > 1;
    }
    (
        managed,
        if conflict {
            "conflict"
        } else if legacy {
            "legacy"
        } else if managed {
            "current"
        } else {
            "missing"
        },
    )
}
fn status(path: &Path, agent: &str) -> (bool, &'static str) {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return (false, "missing"),
        Err(_) => return (false, "unreadable"),
    };
    if !file
        .metadata()
        .is_ok_and(|meta| meta.is_file() && meta.len() <= 1024 * 1024)
    {
        return (false, "unreadable");
    }
    let mut text = String::new();
    if file
        .take(1024 * 1024 + 1)
        .read_to_string(&mut text)
        .is_err()
        || text.len() > 1024 * 1024
    {
        return (false, "unreadable");
    }
    if agent != "opencode" {
        return json_state(&text, agent);
    }
    let managed =
        text.contains("// open-island-managed") && text.contains("// end-open-island-managed");
    (
        managed,
        if !managed {
            "conflict"
        } else if crate::hook_templates::opencode_plugin().is_ok_and(|expected| text == expected) {
            "current"
        } else {
            "legacy"
        },
    )
}
pub fn collect(home: Option<&Path>) -> Vec<HookStatus> {
    [
        ("claude", ".claude/settings.json"),
        ("codex", ".codex/hooks.json"),
        ("opencode", ".config/opencode/plugins/open-island.ts"),
    ]
    .into_iter()
    .map(|(agent, suffix)| {
        let (managed, state) = home
            .map(|home| status(&home.join(suffix), agent))
            .unwrap_or((false, "unreadable"));
        HookStatus {
            agent: agent.into(),
            managed,
            state: state.into(),
        }
    })
    .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn current_legacy_and_conflicting_hooks_use_the_installer_contract() {
        let document = |command| {
            json!({"hooks":{"Stop":[{"hooks":[{"type":"command","command":command}]}]}}).to_string()
        };
        assert_eq!(
            json_state(
                &document(crate::hook_templates::command("claude")),
                "claude"
            ),
            (true, "current")
        );
        assert_eq!(
            json_state(
                &document("/old/open-islandd hook --agent claude --managed-by open-island".into()),
                "claude"
            ),
            (true, "legacy")
        );
        assert_eq!(
            json_state(&document("custom-open-island-handler".into()), "claude"),
            (false, "conflict")
        );
    }
}
