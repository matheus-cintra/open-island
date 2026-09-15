const DAEMON_RESOLVER: &str = include_str!("../templates/resolve-daemon.sh");
pub fn command(agent: &str) -> String {
    let quoted = format!("'{}'", DAEMON_RESOLVER.replace('\'', "'\"'\"'"));
    format!("/bin/sh -c {quoted} open-island-hook hook --agent {agent} --managed-by open-island")
}
pub fn opencode_plugin() -> Result<String, String> {
    let argv = serde_json::to_string(&["/bin/sh", "-c", DAEMON_RESOLVER, "open-island-hook"])
        .map_err(|_| "invalid_daemon_resolver".to_owned())?;
    Ok(
        include_str!("../templates/open-island-opencode.ts.template")
            .replace("__OPEN_ISLANDD_ARGV__", &argv),
    )
}
