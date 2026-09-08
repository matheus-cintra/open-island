#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchType {
    Contains,
    Prefix,
    Equals,
}

impl MatchType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::Prefix => "prefix",
            Self::Equals => "equals",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "contains" => Some(Self::Contains),
            "prefix" => Some(Self::Prefix),
            "equals" => Some(Self::Equals),
            _ => None,
        }
    }

    fn matches(self, haystack: &str, pattern: &str) -> bool {
        match self {
            Self::Contains => haystack.contains(pattern),
            Self::Prefix => haystack.starts_with(pattern),
            Self::Equals => haystack == pattern,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleField {
    Cwd,
    Prompt,
}

impl RuleField {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cwd => "cwd",
            Self::Prompt => "prompt",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "cwd" => Some(Self::Cwd),
            "prompt" => Some(Self::Prompt),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SilenceRule {
    pub field: RuleField,
    pub match_type: MatchType,
    pub pattern: String,
    pub name: String,
    pub built_in: bool,
    pub enabled: bool,
}

impl SilenceRule {
    fn matches(&self, cwd: &str, prompt: Option<&str>) -> bool {
        if !self.enabled || self.pattern.is_empty() {
            return false;
        }
        let haystack = match self.field {
            RuleField::Cwd => cwd,
            RuleField::Prompt => match prompt {
                Some(text) => text,
                None => return false,
            },
        };
        self.match_type.matches(haystack, &self.pattern)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LauncherRule {
    pub app_id: String,
    pub name: String,
    pub enabled: bool,
}

impl LauncherRule {
    fn matches(&self, launcher: Option<&str>) -> bool {
        if !self.enabled || self.app_id.is_empty() {
            return false;
        }
        launcher.is_some_and(|app_id| app_id == self.app_id)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Subject<'a> {
    pub cwd: &'a str,
    pub prompt: Option<&'a str>,
    pub launcher: Option<&'a str>,
}

impl<'a> Subject<'a> {
    pub fn new(cwd: &'a str, prompt: Option<&'a str>, launcher: Option<&'a str>) -> Self {
        Self {
            cwd,
            prompt,
            launcher,
        }
    }
}

fn built_in(name: &str, field: RuleField, match_type: MatchType, pattern: &str) -> SilenceRule {
    SilenceRule {
        field,
        match_type,
        pattern: pattern.to_owned(),
        name: name.to_owned(),
        built_in: true,
        enabled: true,
    }
}

pub fn built_in_rules() -> Vec<SilenceRule> {
    vec![
        built_in(
            "Codex Memory Writer (diretório)",
            RuleField::Cwd,
            MatchType::Contains,
            "/.codex/memories",
        ),
        built_in(
            "Resumo de Memória Codex Chronicle",
            RuleField::Cwd,
            MatchType::Contains,
            "/chronicle/screen_recording",
        ),
        built_in(
            "Sessões em segundo plano do plugin claude-mem",
            RuleField::Cwd,
            MatchType::Contains,
            "/.claude-mem",
        ),
        built_in(
            "Codex Memory Writer (prefixo de prompt)",
            RuleField::Prompt,
            MatchType::Prefix,
            "## Memory Writing Agent",
        ),
    ]
}

pub fn admits(rules: &[SilenceRule], launchers: &[LauncherRule], subject: Subject<'_>) -> bool {
    !rules
        .iter()
        .any(|rule| rule.matches(subject.cwd, subject.prompt))
        && !launchers
            .iter()
            .any(|launcher| launcher.matches(subject.launcher))
}

#[cfg(test)]
#[path = "filters_tests.rs"]
mod tests;
