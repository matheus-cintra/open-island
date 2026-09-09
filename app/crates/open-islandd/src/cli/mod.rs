pub mod autostart;
pub mod client;
pub mod hook;
pub mod hooks;
pub mod hotkey;

pub fn run_tool(program: &str, args: &[&str]) -> String {
    match std::process::Command::new(program).args(args).output() {
        Ok(output) if output.status.success() => "ok".to_owned(),
        Ok(output) => String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        Err(error) => error.to_string(),
    }
}
