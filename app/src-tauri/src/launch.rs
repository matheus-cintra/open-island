pub const AGENTS: [&str; 3] = ["claude", "codex", "opencode"];

pub fn known_agent(name: &str) -> Result<&'static str, String> {
    AGENTS
        .iter()
        .copied()
        .find(|agent| *agent == name)
        .ok_or_else(|| format!("unknown agent '{name}'"))
}

#[cfg(test)]
#[path = "launch_tests.rs"]
mod tests;
