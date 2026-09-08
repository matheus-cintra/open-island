use open_island_core::usage::{anthropic, redact, UsageError, UsageProvider, UsageSnapshot};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_BETA: &str = "oauth-2025-04-20";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const BODY_TIMEOUT: Duration = Duration::from_secs(15);

pub struct AnthropicUsage {
    home: PathBuf,
}

impl AnthropicUsage {
    pub fn new(home: PathBuf) -> Self {
        Self { home }
    }

    fn credentials_path(&self) -> PathBuf {
        self.home.join(".claude/.credentials.json")
    }

    fn credentials(&self) -> Result<Value, UsageError> {
        read_json(&self.credentials_path()).map_err(|error| UsageError::Credential(redact(&error)))
    }

    fn account(&self) -> Option<Value> {
        let current = fs::read_to_string(self.home.join(".claude/.account-current")).ok()?;
        let name = current.trim();
        if name.is_empty() {
            return None;
        }
        read_json(&self.home.join(format!(".claude/accounts/{name}.json"))).ok()
    }

    pub fn local_identity(&self) -> Option<String> {
        let credentials = self.credentials().ok()?;
        anthropic::identity(&credentials, self.account().as_ref())
    }

    fn access_token(&self, credentials: &Value) -> Result<String, UsageError> {
        let oauth = credentials
            .get("claudeAiOauth")
            .ok_or_else(|| UsageError::Credential("no claudeAiOauth block".to_owned()))?;
        if let Some(expires_at) = oauth.get("expiresAt").and_then(Value::as_u64) {
            if expires_at <= now_ms() {
                return Err(UsageError::Credential("oauth token expired".to_owned()));
            }
        }
        oauth
            .get("accessToken")
            .and_then(Value::as_str)
            .filter(|token| !token.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| UsageError::Credential("no access token".to_owned()))
    }
}

impl UsageProvider for AnthropicUsage {
    fn name(&self) -> &'static str {
        open_island_core::usage::PROVIDER_ANTHROPIC
    }

    fn discover(&self) -> bool {
        self.credentials_path().is_file()
    }

    fn fetch(&self) -> Result<Value, UsageError> {
        let credentials = self.credentials()?;
        let token = self.access_token(&credentials)?;
        let config = ureq::Agent::config_builder()
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_recv_body(Some(BODY_TIMEOUT))
            .build();
        let agent: ureq::Agent = config.into();
        let body = agent
            .get(USAGE_URL)
            .header("Authorization", format!("Bearer {token}"))
            .header("anthropic-beta", OAUTH_BETA)
            .call()
            .map_err(|error| UsageError::Transport(redact(&error.to_string())))?
            .body_mut()
            .read_to_string()
            .map_err(|error| UsageError::Transport(redact(&error.to_string())))?;
        serde_json::from_str(&body).map_err(|error| UsageError::Shape(redact(&error.to_string())))
    }

    fn identity(&self, _raw: &Value) -> Option<String> {
        self.local_identity()
    }

    fn normalize(&self, raw: &Value, now_ms: u64) -> Result<UsageSnapshot, UsageError> {
        anthropic::normalize(raw, self.identity(raw), now_ms)
    }
}

fn read_json(path: &Path) -> Result<Value, String> {
    let text =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("parse {}: {error}", path.display()))
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}
