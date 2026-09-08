use crate::{
    jump::{JumpStep, LocationResolver},
    runner::CommandRunner,
    terminal::{EditorKind, TerminalInfo},
};

pub struct VscodeResolver;
impl LocationResolver for VscodeResolver {
    fn id(&self) -> &str {
        "vscode"
    }
    fn can_resolve(&self, host: &TerminalInfo) -> bool {
        host.editor.as_ref().is_some_and(|editor| {
            matches!(
                editor.kind,
                EditorKind::VsCode | EditorKind::Cursor | EditorKind::Windsurf | EditorKind::Codium
            )
        })
    }
    fn resolve(&self, host: &TerminalInfo, _runner: &dyn CommandRunner) -> Option<Vec<JumpStep>> {
        let Some(editor) = host.editor.as_ref() else {
            return Some(vec![JumpStep::ActivateApp {
                pid: host.raise_pid,
            }]);
        };
        if editor.pid == 0 {
            return Some(vec![JumpStep::ActivateApp {
                pid: host.raise_pid,
            }]);
        }

        // Windsurf and Codium share this identical CLI contract; neither is installed here,
        // so both use this path without a real-binary test.
        Some(vec![JumpStep::RaiseWindow { pid: editor.pid }])
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::FakeRunner;
    use crate::terminal::{EditorLayer, TerminalLayer};
    use std::collections::HashMap;

    fn host(editor: Option<EditorLayer>, terminal: Option<TerminalLayer>) -> TerminalInfo {
        TerminalInfo {
            kind: "kitty".into(),
            raise_pid: 99,
            agent_pid: 30,
            multiplexer: None,
            terminal,
            editor,
            env: HashMap::new(),
        }
    }

    fn editor(kind: EditorKind, pid: u32) -> EditorLayer {
        EditorLayer { kind, pid }
    }

    #[test]
    fn agent_inside_cursor_raises_editor_not_agent() {
        let runner = FakeRunner::new();
        let host = host(Some(editor(EditorKind::Cursor, 10)), None);

        assert_eq!(
            VscodeResolver.resolve(&host, &runner),
            Some(vec![JumpStep::RaiseWindow { pid: 10 }])
        );
        assert_ne!(host.agent_pid, 10);
        assert!(runner.calls().is_empty());
    }

    #[test]
    fn cursor_as_agent_raises_its_own_editor_window() {
        let runner = FakeRunner::new();
        let mut host = host(Some(editor(EditorKind::Cursor, 10)), None);
        host.agent_pid = 10;

        assert_eq!(
            VscodeResolver.resolve(&host, &runner),
            Some(vec![JumpStep::RaiseWindow { pid: 10 }])
        );
        assert_eq!(
            host.agent_pid,
            host.editor.as_ref().map_or(0, |layer| layer.pid)
        );
        assert!(runner.calls().is_empty());
    }

    #[test]
    fn all_vscode_family_editors_raise_their_window() {
        for kind in [EditorKind::VsCode, EditorKind::Windsurf, EditorKind::Codium] {
            let runner = FakeRunner::new();
            let host = host(Some(editor(kind, 10)), None);
            assert_eq!(
                VscodeResolver.resolve(&host, &runner),
                Some(vec![JumpStep::RaiseWindow { pid: 10 }])
            );
            assert!(runner.calls().is_empty());
        }
    }

    #[test]
    fn declines_kitty_terminal_host() {
        let runner = FakeRunner::new();
        let host = host(
            None,
            Some(TerminalLayer {
                kind: "kitty".into(),
                pid: 10,
                window_id: None,
            }),
        );

        assert!(!VscodeResolver.can_resolve(&host));
        assert!(runner.calls().is_empty());
    }

    #[test]
    fn declines_zed() {
        let host = host(Some(editor(EditorKind::Zed, 10)), None);
        assert!(!VscodeResolver.can_resolve(&host));
    }

    #[test]
    fn absent_editor_degrades_to_activate_app_without_commands() {
        let runner = FakeRunner::new();
        let host = host(None, None);

        assert_eq!(
            VscodeResolver.resolve(&host, &runner),
            Some(vec![JumpStep::ActivateApp { pid: 99 }])
        );
        assert!(runner.calls().is_empty());
    }

    #[test]
    fn zero_editor_pid_degrades_to_activate_app_without_commands() {
        let runner = FakeRunner::new();
        let host = host(Some(editor(EditorKind::VsCode, 0)), None);

        assert_eq!(
            VscodeResolver.resolve(&host, &runner),
            Some(vec![JumpStep::ActivateApp { pid: 99 }])
        );
        assert!(runner.calls().is_empty());
    }
}
