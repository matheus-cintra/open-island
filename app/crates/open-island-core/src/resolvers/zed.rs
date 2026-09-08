use crate::{
    jump::{JumpStep, LocationResolver},
    runner::CommandRunner,
    terminal::{EditorKind, TerminalInfo},
};
pub struct ZedResolver;
impl LocationResolver for ZedResolver {
    fn id(&self) -> &str {
        "zed"
    }
    fn can_resolve(&self, host: &TerminalInfo) -> bool {
        host.editor
            .as_ref()
            .is_some_and(|editor| editor.kind == EditorKind::Zed)
    }
    fn resolve(&self, host: &TerminalInfo, _: &dyn CommandRunner) -> Option<Vec<JumpStep>> {
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
        // Deliberately do not pass the project path: it can alter Zed's open workspace,
        // and Zed has no per-window targeting flag that avoids that.
        Some(vec![JumpStep::RaiseWindow { pid: editor.pid }])
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::FakeRunner;
    use crate::terminal::{EditorKind, EditorLayer, TerminalLayer};
    use std::collections::HashMap;

    fn host(editor: Option<EditorLayer>, terminal: Option<TerminalLayer>) -> TerminalInfo {
        TerminalInfo {
            kind: "zed".into(),
            raise_pid: 986_081,
            agent_pid: 42,
            multiplexer: None,
            terminal,
            editor,
            env: HashMap::new(),
        }
    }

    #[test]
    fn zed_host_raises_editor_window_without_commands() {
        let host = host(
            Some(EditorLayer {
                kind: EditorKind::Zed,
                pid: 986_081,
            }),
            None,
        );
        let runner = FakeRunner::new();

        let plan = ZedResolver.resolve(&host, &runner);

        assert_eq!(plan, Some(vec![JumpStep::RaiseWindow { pid: 986_081 }]));
        assert!(runner.calls().is_empty());
    }

    #[test]
    fn zed_plan_contains_only_raise_window() {
        let host = host(
            Some(EditorLayer {
                kind: EditorKind::Zed,
                pid: 986_081,
            }),
            None,
        );
        let runner = FakeRunner::new();

        let plan = ZedResolver.resolve(&host, &runner);

        assert_eq!(plan, Some(vec![JumpStep::RaiseWindow { pid: 986_081 }]));
    }

    #[test]
    fn absent_editor_activates_host_without_commands() {
        let host = host(None, None);
        let runner = FakeRunner::new();

        let plan = ZedResolver.resolve(&host, &runner);

        assert_eq!(plan, Some(vec![JumpStep::ActivateApp { pid: 986_081 }]));
        assert!(runner.calls().is_empty());
    }

    #[test]
    fn zero_editor_pid_activates_host_without_commands() {
        let host = host(
            Some(EditorLayer {
                kind: EditorKind::Zed,
                pid: 0,
            }),
            None,
        );
        let runner = FakeRunner::new();

        let plan = ZedResolver.resolve(&host, &runner);

        assert_eq!(plan, Some(vec![JumpStep::ActivateApp { pid: 986_081 }]));
        assert!(runner.calls().is_empty());
    }

    #[test]
    fn can_resolve_only_zed_editor_hosts() {
        let zed = host(
            Some(EditorLayer {
                kind: EditorKind::Zed,
                pid: 986_081,
            }),
            None,
        );
        let cursor = host(
            Some(EditorLayer {
                kind: EditorKind::Cursor,
                pid: 986_081,
            }),
            None,
        );
        let kitty = host(
            None,
            Some(TerminalLayer {
                kind: "kitty".into(),
                pid: 7,
                window_id: Some("1".into()),
            }),
        );
        let absent = host(None, None);

        assert!(ZedResolver.can_resolve(&zed));
        assert!(!ZedResolver.can_resolve(&cursor));
        assert!(!ZedResolver.can_resolve(&kitty));
        assert!(!ZedResolver.can_resolve(&absent));
    }
}
