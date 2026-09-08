use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

pub const PROVIDER_ANTHROPIC: &str = "anthropic";
pub const PROVIDER_CODEX: &str = "codex";
pub const USAGE_STALE_AFTER: Duration = Duration::from_secs(900);
pub const CODEX_CREDITS_PER_DOLLAR: f64 = 25.0;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct UsageWindow {
    pub key: String,
    pub label: String,
    pub percent: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_at_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct UsageModelWindow {
    pub model: String,
    pub percent: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_at_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ResetCard {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct UsageCredits {
    pub balance: f64,
    pub unlimited: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct UsageSnapshot {
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    pub windows: Vec<UsageWindow>,
    #[serde(default)]
    pub models: Vec<UsageModelWindow>,
    #[serde(default)]
    pub reset_cards: Vec<ResetCard>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credits: Option<UsageCredits>,
    pub fetched_at_ms: u64,
}

impl UsageSnapshot {
    pub fn peak_percent(&self) -> f32 {
        self.windows
            .iter()
            .map(|window| window.percent)
            .chain(self.models.iter().map(|model| model.percent))
            .fold(0.0, f32::max)
    }

    pub fn is_stale(&self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.fetched_at_ms) > USAGE_STALE_AFTER.as_millis() as u64
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ProviderUsage {
    pub provider: String,
    pub detected: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<UsageSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub checked_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Default)]
pub struct UsageReport {
    pub providers: Vec<ProviderUsage>,
}

impl UsageReport {
    pub fn provider(&self, name: &str) -> Option<&ProviderUsage> {
        self.providers.iter().find(|entry| entry.provider == name)
    }

    pub fn snapshot(&self, name: &str) -> Option<&UsageSnapshot> {
        self.provider(name)
            .and_then(|entry| entry.snapshot.as_ref())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UsageError {
    NotConfigured(String),
    Credential(String),
    Transport(String),
    Shape(String),
}

impl UsageError {
    pub fn message(&self) -> &str {
        match self {
            Self::NotConfigured(text)
            | Self::Credential(text)
            | Self::Transport(text)
            | Self::Shape(text) => text,
        }
    }
}

impl std::fmt::Display for UsageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message())
    }
}

pub trait UsageProvider {
    fn name(&self) -> &'static str;
    fn discover(&self) -> bool;
    fn fetch(&self) -> Result<Value, UsageError>;
    fn identity(&self, raw: &Value) -> Option<String>;
    fn normalize(&self, raw: &Value, now_ms: u64) -> Result<UsageSnapshot, UsageError>;
}

pub fn redact(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for token in split_keeping_separators(text) {
        if is_secret(token) {
            out.push_str("<redacted>");
        } else {
            out.push_str(token);
        }
    }
    out
}

fn split_keeping_separators(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    for (index, character) in text.char_indices() {
        if character.is_whitespace() || matches!(character, '"' | '\'' | ',' | '(' | ')') {
            if index > start {
                parts.push(&text[start..index]);
            }
            parts.push(&text[index..index + character.len_utf8()]);
            start = index + character.len_utf8();
        }
    }
    if start < text.len() {
        parts.push(&text[start..]);
    }
    parts
}

fn is_secret(token: &str) -> bool {
    let trimmed = token.trim_matches(|c: char| {
        !c.is_ascii_alphanumeric() && c != '-' && c != '_' && c != '.' && c != '@'
    });
    if trimmed.len() < 12 {
        return false;
    }
    if trimmed.starts_with("sk-") || trimmed.starts_with("eyJ") {
        return true;
    }
    if trimmed.contains('@') && trimmed.contains('.') {
        return true;
    }
    false
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThresholdWatch {
    marks: Vec<(String, bool)>,
}

impl ThresholdWatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe(&mut self, provider: &str, percent: f32, threshold: f32) -> bool {
        let over = percent >= threshold;
        match self.marks.iter_mut().find(|(name, _)| name == provider) {
            Some((_, previous)) => {
                let crossed = over && !*previous;
                *previous = over;
                crossed
            }
            None => {
                self.marks.push((provider.to_owned(), over));
                over
            }
        }
    }

    pub fn forget(&mut self, provider: &str) {
        self.marks.retain(|(name, _)| name != provider);
    }
}

pub fn window_label(minutes: u64) -> String {
    match minutes {
        0 => String::new(),
        m if m % 10080 == 0 => format!("{}d", m / 1440),
        m if m % 1440 == 0 => format!("{}d", m / 1440),
        m if m % 60 == 0 => format!("{}h", m / 60),
        m => format!("{m}m"),
    }
}

pub mod anthropic {
    use super::{
        rfc3339_to_ms, ResetCard, UsageError, UsageModelWindow, UsageSnapshot, UsageWindow,
        PROVIDER_ANTHROPIC,
    };
    use serde_json::Value;

    pub fn identity(credentials: &Value, account: Option<&Value>) -> Option<String> {
        account
            .and_then(|value| value.get("oauthAccount"))
            .and_then(|value| value.get("accountUuid"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                credentials
                    .get("claudeAiOauth")
                    .and_then(|value| value.get("subscriptionType"))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
    }

    pub fn normalize(
        raw: &Value,
        identity: Option<String>,
        now_ms: u64,
    ) -> Result<UsageSnapshot, UsageError> {
        let mut windows = Vec::new();
        let mut models = Vec::new();
        if let Some(limits) = raw.get("limits").and_then(Value::as_array) {
            for limit in limits {
                let percent = number(limit.get("percent"))?;
                let resets_at_ms = limit
                    .get("resets_at")
                    .and_then(Value::as_str)
                    .and_then(rfc3339_to_ms);
                match model_name(limit) {
                    Some(model) => models.push(UsageModelWindow {
                        model,
                        percent,
                        resets_at_ms,
                    }),
                    None => {
                        let Some((key, label)) = window_key(limit) else {
                            continue;
                        };
                        windows.push(UsageWindow {
                            key: key.to_owned(),
                            label: label.to_owned(),
                            percent,
                            resets_at_ms,
                        });
                    }
                }
            }
        }
        if windows.is_empty() {
            for (field, key, label) in [
                ("five_hour", "session", "5h"),
                ("seven_day", "weekly", "7d"),
            ] {
                let Some(section) = raw.get(field).filter(|value| !value.is_null()) else {
                    continue;
                };
                windows.push(UsageWindow {
                    key: key.to_owned(),
                    label: label.to_owned(),
                    percent: number(section.get("utilization"))?,
                    resets_at_ms: section
                        .get("resets_at")
                        .and_then(Value::as_str)
                        .and_then(rfc3339_to_ms),
                });
            }
        }
        if windows.is_empty() && models.is_empty() {
            return Err(UsageError::Shape(
                "no usable usage windows in the response".to_owned(),
            ));
        }
        Ok(UsageSnapshot {
            provider: PROVIDER_ANTHROPIC.to_owned(),
            identity,
            plan: None,
            windows,
            models,
            reset_cards: Vec::<ResetCard>::new(),
            credits: None,
            fetched_at_ms: now_ms,
        })
    }

    fn window_key(limit: &Value) -> Option<(&'static str, &'static str)> {
        match limit.get("kind").and_then(Value::as_str)? {
            "session" => Some(("session", "5h")),
            "weekly_all" => Some(("weekly", "7d")),
            _ => None,
        }
    }

    fn model_name(limit: &Value) -> Option<String> {
        limit
            .get("scope")
            .and_then(|scope| scope.get("model"))
            .and_then(|model| model.get("display_name"))
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .map(str::to_owned)
    }

    fn number(value: Option<&Value>) -> Result<f32, UsageError> {
        value
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
            .map(|value| value.clamp(0.0, 100.0) as f32)
            .ok_or_else(|| UsageError::Shape("a usage window carried no percentage".to_owned()))
    }
}

pub mod codex {
    use super::{
        window_label, ResetCard, UsageCredits, UsageError, UsageModelWindow, UsageSnapshot,
        UsageWindow, PROVIDER_CODEX,
    };
    use serde_json::Value;

    pub fn identity(raw: &Value) -> Option<String> {
        raw.get("accountId")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    }

    pub fn normalize(
        raw: &Value,
        identity: Option<String>,
        now_ms: u64,
    ) -> Result<UsageSnapshot, UsageError> {
        let limits = raw
            .get("rateLimits")
            .filter(|value| !value.is_null())
            .ok_or_else(|| UsageError::Shape("the response carried no rateLimits".to_owned()))?;
        let mut windows = Vec::new();
        for (field, key) in [("primary", "primary"), ("secondary", "secondary")] {
            let Some(window) = limits.get(field).filter(|value| !value.is_null()) else {
                continue;
            };
            windows.push(window_from(window, key)?);
        }
        if windows.is_empty() {
            return Err(UsageError::Shape(
                "no usable usage windows in the response".to_owned(),
            ));
        }
        let primary_id = limits.get("limitId").and_then(Value::as_str);
        let mut models = Vec::new();
        if let Some(by_id) = raw.get("rateLimitsByLimitId").and_then(Value::as_object) {
            for (id, bucket) in by_id {
                if Some(id.as_str()) == primary_id {
                    continue;
                }
                let Some(name) = bucket
                    .get("limitName")
                    .and_then(Value::as_str)
                    .filter(|name| !name.trim().is_empty())
                else {
                    continue;
                };
                let Some(window) = bucket.get("primary").filter(|value| !value.is_null()) else {
                    continue;
                };
                let window = window_from(window, "primary")?;
                models.push(UsageModelWindow {
                    model: name.to_owned(),
                    percent: window.percent,
                    resets_at_ms: window.resets_at_ms,
                });
            }
            models.sort_by(|left, right| left.model.cmp(&right.model));
        }
        Ok(UsageSnapshot {
            provider: PROVIDER_CODEX.to_owned(),
            identity,
            plan: limits
                .get("planType")
                .and_then(Value::as_str)
                .map(str::to_owned),
            windows,
            models,
            reset_cards: reset_cards(raw),
            credits: credits(limits),
            fetched_at_ms: now_ms,
        })
    }

    fn window_from(window: &Value, key: &str) -> Result<UsageWindow, UsageError> {
        let percent = window
            .get("usedPercent")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
            .map(|value| value.clamp(0.0, 100.0) as f32)
            .ok_or_else(|| UsageError::Shape("a usage window carried no usedPercent".to_owned()))?;
        let minutes = window
            .get("windowDurationMins")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        Ok(UsageWindow {
            key: key.to_owned(),
            label: window_label(minutes),
            percent,
            resets_at_ms: window
                .get("resetsAt")
                .and_then(Value::as_u64)
                .map(|seconds| seconds.saturating_mul(1000)),
        })
    }

    fn reset_cards(raw: &Value) -> Vec<ResetCard> {
        let Some(entries) = raw
            .get("rateLimitResetCredits")
            .and_then(|value| value.get("credits"))
            .and_then(Value::as_array)
        else {
            return Vec::new();
        };
        entries
            .iter()
            .filter(|card| card.get("status").and_then(Value::as_str) == Some("available"))
            .filter_map(|card| {
                Some(ResetCard {
                    id: card.get("id").and_then(Value::as_str)?.to_owned(),
                    title: card.get("title").and_then(Value::as_str).map(str::to_owned),
                    expires_at_ms: card
                        .get("expiresAt")
                        .and_then(Value::as_u64)
                        .map(|seconds| seconds.saturating_mul(1000)),
                })
            })
            .collect()
    }

    fn credits(limits: &Value) -> Option<UsageCredits> {
        let credits = limits.get("credits").filter(|value| !value.is_null())?;
        let balance = credits
            .get("balance")
            .and_then(|value| match value {
                Value::String(text) => text.parse::<f64>().ok(),
                other => other.as_f64(),
            })
            .unwrap_or_default();
        Some(UsageCredits {
            balance,
            unlimited: credits
                .get("unlimited")
                .and_then(Value::as_bool)
                .unwrap_or_default(),
        })
    }
}

pub fn rfc3339_to_ms(text: &str) -> Option<u64> {
    let bytes = text.as_bytes();
    if bytes.len() < 19 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    if !matches!(bytes[10], b'T' | b't' | b' ') || bytes[13] != b':' || bytes[16] != b':' {
        return None;
    }
    let year = digits(&text[0..4])? as i64;
    let month = digits(&text[5..7])? as i64;
    let day = digits(&text[8..10])? as i64;
    let hour = digits(&text[11..13])?;
    let minute = digits(&text[14..16])?;
    let second = digits(&text[17..19])?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }
    let offset_minutes = zone_offset_minutes(&text[19..])?;
    let days = days_from_civil(year, month, day);
    let seconds =
        days * 86_400 + i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second)
            - i64::from(offset_minutes) * 60;
    u64::try_from(seconds).ok()?.checked_mul(1000)
}

fn zone_offset_minutes(rest: &str) -> Option<i32> {
    if rest.contains(['Z', 'z']) {
        return Some(0);
    }
    let Some(index) = rest.rfind(['+', '-']) else {
        return Some(0);
    };
    let zone = &rest[index..];
    if zone.len() < 6 || zone.as_bytes()[3] != b':' {
        return Some(0);
    }
    let sign = if zone.starts_with('-') { -1 } else { 1 };
    let hours = digits(&zone[1..3])? as i32;
    let minutes = digits(&zone[4..6])? as i32;
    Some(sign * (hours * 60 + minutes))
}

fn digits(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse::<u32>().ok()
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
#[path = "usage_tests.rs"]
mod tests;
