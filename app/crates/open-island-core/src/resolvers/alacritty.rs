use crate::{
    jump::{JumpStep, LocationResolver},
    runner::CommandRunner,
    terminal::TerminalInfo,
};

pub struct AlacrittyResolver;
impl LocationResolver for AlacrittyResolver {
    fn id(&self) -> &str {
        "alacritty"
    }
    fn can_resolve(&self, host: &TerminalInfo) -> bool {
        host.terminal
            .as_ref()
            .is_some_and(|t| t.kind == "alacritty")
    }
    fn resolve(&self, host: &TerminalInfo, _: &dyn CommandRunner) -> Option<Vec<JumpStep>> {
        Some(vec![JumpStep::RaiseWindow {
            pid: host.raise_pid,
        }])
    }
}
