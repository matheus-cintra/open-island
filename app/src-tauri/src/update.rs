use crate::terminal;

const INSTALL_SCRIPT: &str = "curl -fsSL https://raw.githubusercontent.com/matheus-cintra/open-island/master/install.sh | sh && systemctl --user restart open-islandd.service open-island.service; printf '\\n%s' \"$1\"; read dummy";

#[cfg(not(target_os = "macos"))]
pub fn run(prompt: &str) -> Result<(), String> {
    let program =
        terminal::pick().ok_or_else(|| "no terminal emulator found on PATH".to_owned())?;
    terminal::spawn_detached(terminal::terminal_argv(&program, INSTALL_SCRIPT, &[prompt]))
}
#[cfg(test)]
mod tests {
    use super::INSTALL_SCRIPT;

    #[test]
    fn the_shipped_script_reinstalls_then_restarts_both_units_and_waits() {
        assert!(INSTALL_SCRIPT.starts_with(
            "curl -fsSL https://raw.githubusercontent.com/matheus-cintra/open-island/master/install.sh | sh && systemctl --user restart open-islandd.service open-island.service;"
        ));
        assert!(INSTALL_SCRIPT.ends_with("printf '\\n%s' \"$1\"; read dummy"));
    }
}
