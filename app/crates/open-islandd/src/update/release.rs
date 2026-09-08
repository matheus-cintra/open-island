use serde_json::Value;
use std::time::Duration;

pub const RELEASE_URL: &str =
    "https://api.github.com/repos/matheus-cintra/open-island/releases/latest";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const BODY_TIMEOUT: Duration = Duration::from_secs(15);
const GLOBAL_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReleaseError {
    Transport(String),
    Shape(String),
}

impl ReleaseError {
    pub fn message(&self) -> &str {
        match self {
            Self::Transport(text) | Self::Shape(text) => text,
        }
    }
}

impl std::fmt::Display for ReleaseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message())
    }
}

pub fn fetch() -> Result<String, ReleaseError> {
    let config = ureq::Agent::config_builder()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_recv_body(Some(BODY_TIMEOUT))
        .timeout_global(Some(GLOBAL_TIMEOUT))
        .build();
    let agent: ureq::Agent = config.into();
    agent
        .get(RELEASE_URL)
        .call()
        .map_err(|error| ReleaseError::Transport(error.to_string()))?
        .body_mut()
        .read_to_string()
        .map_err(|error| ReleaseError::Transport(error.to_string()))
}

pub fn parse(body: &str) -> Result<String, ReleaseError> {
    let document: Value =
        serde_json::from_str(body).map_err(|error| ReleaseError::Shape(error.to_string()))?;
    let release = document
        .as_object()
        .ok_or_else(|| ReleaseError::Shape("release is not an object".to_owned()))?;
    release
        .get("tag_name")
        .and_then(Value::as_str)
        .filter(|tag| !tag.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| ReleaseError::Shape("release has no tag_name".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_document_yields_its_tag() {
        let body = r#"{
            "url": "https://api.github.com/repos/matheus-cintra/open-island/releases/1",
            "tag_name": "v0.2.0",
            "name": "v0.2.0",
            "draft": false,
            "prerelease": false,
            "published_at": "2026-09-01T12:00:00Z",
            "html_url": "https://github.com/matheus-cintra/open-island/releases/tag/v0.2.0",
            "assets": []
        }"#;
        assert_eq!(parse(body), Ok("v0.2.0".to_owned()));
    }

    #[test]
    fn a_release_without_a_tag_is_an_error_not_an_empty_string() {
        assert!(matches!(
            parse(r#"{"name": "v0.2.0", "draft": false}"#),
            Err(ReleaseError::Shape(_))
        ));
        assert!(matches!(
            parse(r#"{"tag_name": ""}"#),
            Err(ReleaseError::Shape(_))
        ));
        assert!(matches!(
            parse(r#"{"tag_name": 7}"#),
            Err(ReleaseError::Shape(_))
        ));
    }

    #[test]
    fn a_body_that_is_not_json_is_an_error() {
        assert!(matches!(
            parse("<html>rate limited</html>"),
            Err(ReleaseError::Shape(_))
        ));
        assert!(matches!(parse(""), Err(ReleaseError::Shape(_))));
    }

    #[test]
    fn a_json_body_that_is_not_an_object_is_an_error() {
        assert_eq!(
            parse(r#"[{"tag_name": "v0.2.0"}]"#),
            Err(ReleaseError::Shape("release is not an object".to_owned()))
        );
        assert!(matches!(parse(r#""v0.2.0""#), Err(ReleaseError::Shape(_))));
        assert!(matches!(parse("null"), Err(ReleaseError::Shape(_))));
    }
}
