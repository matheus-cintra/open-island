use crate::filters::{built_in_rules, LauncherRule, MatchType, RuleField, SilenceRule};
use crate::store::IDLE_AFTER;
use serde_json::{json, Value};
#[cfg(test)]
use std::ffi::OsString;
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};

pub const SOUND_THEME_DIR: &str = "/usr/share/sounds/freedesktop/stereo";
pub const DEFAULT_IDLE_REMINDER_AFTER: Duration = Duration::from_secs(300);
pub const DEFAULT_VOLUME: f32 = 0.3;
pub const DEFAULT_HOVER_DWELL: Duration = Duration::from_millis(250);
pub const DEFAULT_AUTO_COLLAPSE: Duration = Duration::from_millis(2500);
pub const DEFAULT_IDLE_FADE: Duration = Duration::from_millis(120_000);
pub const DEFAULT_USAGE_REFRESH: Duration = Duration::from_secs(300);
pub const DEFAULT_USAGE_WARN_THRESHOLD: f32 = 90.0;
pub const MIN_USAGE_REFRESH: Duration = Duration::from_secs(60);
pub const DEFAULT_SESSION_CLEANUP: Duration = Duration::from_secs(2 * 60 * 60);
pub const NOTCH_OFFSET_LIMIT: i32 = 12;
pub const MAX_ISLAND_HEIGHT: u32 = 200;
pub const DEFAULT_CONTENT_FONT: u32 = 11;
pub const DEFAULT_PANEL_MAX_WIDTH: u32 = 664;
pub const DEFAULT_PANEL_MAX_HEIGHT: u32 = 720;
pub const DEFAULT_COMPLETION_CARD_HEIGHT: u32 = 90;
pub const DEFAULT_QUIET_HOURS_START: u32 = 22 * 60;
pub const DEFAULT_QUIET_HOURS_END: u32 = 8 * 60;
pub const DEFAULT_SPAM_WINDOW: Duration = Duration::from_secs(10);
pub const DEFAULT_SPAM_THRESHOLD: u32 = 3;
pub const MINUTES_PER_DAY: u32 = 24 * 60;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Config {
    pub notifications: NotificationConfig,
    pub sound: SoundConfig,
    pub island: IslandConfig,
    pub sessions: SessionsConfig,
    pub display: DisplayConfig,
    pub integrations: IntegrationsConfig,
    pub usage: UsageConfig,
    pub filters: FiltersConfig,
    pub updates: UpdatesConfig,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageValueMode {
    Used,
    Remaining,
}

impl UsageValueMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Used => "used",
            Self::Remaining => "remaining",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "used" => Some(Self::Used),
            "remaining" => Some(Self::Remaining),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageProviderChoice {
    Auto,
    Anthropic,
    Codex,
}

impl UsageProviderChoice {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Anthropic => "anthropic",
            Self::Codex => "codex",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "auto" => Some(Self::Auto),
            "anthropic" => Some(Self::Anthropic),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompactLayout {
    Clean,
    Detailed,
}

impl CompactLayout {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Detailed => "detailed",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "clean" => Some(Self::Clean),
            "detailed" => Some(Self::Detailed),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodexCreditDisplay {
    Credits,
    Dollars,
}

impl CodexCreditDisplay {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Credits => "credits",
            Self::Dollars => "dollars",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "credits" => Some(Self::Credits),
            "dollars" => Some(Self::Dollars),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UsageConfig {
    pub show_limits: bool,
    pub use_claude_login: bool,
    pub value_mode: UsageValueMode,
    pub preferred_provider: UsageProviderChoice,
    pub show_reset_cards: bool,
    pub codex_credit_display: CodexCreditDisplay,
    pub warn_threshold: f32,
    pub refresh_interval: Duration,
}

impl Default for UsageConfig {
    fn default() -> Self {
        Self {
            show_limits: true,
            use_claude_login: true,
            value_mode: UsageValueMode::Used,
            preferred_provider: UsageProviderChoice::Auto,
            show_reset_cards: true,
            codex_credit_display: CodexCreditDisplay::Credits,
            warn_threshold: DEFAULT_USAGE_WARN_THRESHOLD,
            refresh_interval: DEFAULT_USAGE_REFRESH,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UpdatesConfig {
    pub check_enabled: bool,
}

impl Default for UpdatesConfig {
    fn default() -> Self {
        Self {
            check_enabled: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FiltersConfig {
    pub rules: Vec<SilenceRule>,
    pub launchers: Vec<LauncherRule>,
    pub quiet_focus_mode: bool,
    pub quiet_screen_off: bool,
}

impl Default for FiltersConfig {
    fn default() -> Self {
        Self {
            rules: built_in_rules(),
            launchers: Vec::new(),
            quiet_focus_mode: false,
            quiet_screen_off: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct IntegrationsConfig {
    pub macos_terminal: String,
    pub auto_configure: bool,
    pub known_agents: Vec<String>,
}

impl Default for IntegrationsConfig {
    fn default() -> Self {
        Self {
            macos_terminal: "terminal".into(),
            auto_configure: true,
            known_agents: Vec::new(),
        }
    }
}

/// What the session row shows. `tasks` only ever has data from OpenCode, because
/// `TodoWrite` reaches the daemon through its Claude-compatible bridge alone.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayConfig {
    pub compact_layout: CompactLayout,
    pub monitor: Option<String>,
    pub notch_width_offset: i32,
    pub notch_height_offset: i32,
    pub island_height: u32,
    pub project: bool,
    pub worktree: bool,
    pub agent_icons: bool,
    pub terminal_icons: bool,
    pub model: bool,
    pub effort: bool,
    pub activity: bool,
    pub subagents: bool,
    pub tasks: bool,
    pub ui_scale: f32,
    pub content_font: u32,
    pub panel_max_width: u32,
    pub panel_max_height: u32,
    pub completion_card_height: u32,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            compact_layout: CompactLayout::Detailed,
            monitor: None,
            notch_width_offset: 0,
            notch_height_offset: 0,
            island_height: 0,
            project: true,
            worktree: true,
            agent_icons: true,
            terminal_icons: true,
            model: true,
            effort: false,
            activity: true,
            subagents: true,
            tasks: true,
            ui_scale: 0.0,
            content_font: DEFAULT_CONTENT_FONT,
            panel_max_width: DEFAULT_PANEL_MAX_WIDTH,
            panel_max_height: DEFAULT_PANEL_MAX_HEIGHT,
            completion_card_height: DEFAULT_COMPLETION_CARD_HEIGHT,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct IslandConfig {
    pub hover_dwell: Duration,
    pub auto_collapse: Duration,
    pub idle_fade: Duration,
    pub idle_fade_enabled: bool,
    pub expand_on_hover: bool,
    pub collapse_on_leave: bool,
    pub hide_in_fullscreen: bool,
    pub hide_when_idle: bool,
    pub click_to_jump: bool,
    pub smart_suppression: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SessionsConfig {
    pub idle_after: Duration,
    pub cleanup_after: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubagentTiming {
    RootResponses,
    AllFinished,
    EveryCompletion,
}

impl SubagentTiming {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RootResponses => "root_responses",
            Self::AllFinished => "all_finished",
            Self::EveryCompletion => "every_completion",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "root_responses" => Some(Self::RootResponses),
            "all_finished" => Some(Self::AllFinished),
            "every_completion" => Some(Self::EveryCompletion),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NotificationConfig {
    pub idle_reminder_after: Duration,
    pub reminder_needs_response: bool,
    pub reminder_completed_tasks: bool,
    pub expand_on_completion: bool,
    pub expand_on_question: bool,
    pub subagent_timing: SubagentTiming,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SoundConfig {
    pub enabled: bool,
    pub volume: f32,
    pub quiet: bool,
    pub follow_dnd: bool,
    pub quiet_hours: bool,
    pub quiet_hours_start: u32,
    pub quiet_hours_end: u32,
    pub spam_window: Duration,
    pub spam_threshold: u32,
    pub events: SoundEvents,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SoundEvent {
    SessionStart,
    TaskComplete,
    ApprovalNeeded,
    TaskAcknowledge,
    IdleReminder,
    ContextLimit,
    UserSpam,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SoundEvents {
    pub session_start: Option<PathBuf>,
    pub task_complete: Option<PathBuf>,
    pub approval_needed: Option<PathBuf>,
    pub task_acknowledge: Option<PathBuf>,
    pub idle_reminder: Option<PathBuf>,
    pub context_limit: Option<PathBuf>,
    pub user_spam: Option<PathBuf>,
}

impl SoundEvents {
    pub fn path(&self, event: SoundEvent) -> Option<&PathBuf> {
        match event {
            SoundEvent::SessionStart => self.session_start.as_ref(),
            SoundEvent::TaskComplete => self.task_complete.as_ref(),
            SoundEvent::ApprovalNeeded => self.approval_needed.as_ref(),
            SoundEvent::TaskAcknowledge => self.task_acknowledge.as_ref(),
            SoundEvent::IdleReminder => self.idle_reminder.as_ref(),
            SoundEvent::ContextLimit => self.context_limit.as_ref(),
            SoundEvent::UserSpam => self.user_spam.as_ref(),
        }
    }
}

fn theme_sound(name: &str) -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        crate::paths::bundled_sounds().map(|dir| dir.join(format!("{name}.wav")))
    } else {
        Some(PathBuf::from(format!("{SOUND_THEME_DIR}/{name}.oga")))
    }
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            idle_reminder_after: DEFAULT_IDLE_REMINDER_AFTER,
            reminder_needs_response: false,
            reminder_completed_tasks: true,
            expand_on_completion: true,
            expand_on_question: true,
            subagent_timing: SubagentTiming::RootResponses,
        }
    }
}

impl Default for SoundEvents {
    fn default() -> Self {
        Self {
            session_start: theme_sound("device-added"),
            task_complete: theme_sound("complete"),
            approval_needed: theme_sound("message"),
            task_acknowledge: None,
            idle_reminder: theme_sound("dialog-warning"),
            context_limit: theme_sound("suspend-error"),
            user_spam: None,
        }
    }
}

impl Default for IslandConfig {
    fn default() -> Self {
        Self {
            hover_dwell: DEFAULT_HOVER_DWELL,
            auto_collapse: DEFAULT_AUTO_COLLAPSE,
            idle_fade: DEFAULT_IDLE_FADE,
            idle_fade_enabled: false,
            expand_on_hover: true,
            collapse_on_leave: true,
            hide_in_fullscreen: true,
            hide_when_idle: false,
            click_to_jump: true,
            smart_suppression: true,
        }
    }
}

impl Default for SessionsConfig {
    fn default() -> Self {
        Self {
            idle_after: IDLE_AFTER,
            cleanup_after: DEFAULT_SESSION_CLEANUP,
        }
    }
}

impl Default for SoundConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            volume: DEFAULT_VOLUME,
            quiet: false,
            follow_dnd: true,
            quiet_hours: false,
            quiet_hours_start: DEFAULT_QUIET_HOURS_START,
            quiet_hours_end: DEFAULT_QUIET_HOURS_END,
            spam_window: DEFAULT_SPAM_WINDOW,
            spam_threshold: DEFAULT_SPAM_THRESHOLD,
            events: SoundEvents::default(),
        }
    }
}

impl Config {
    pub fn from_json_str(text: &str) -> Self {
        let root = serde_json::from_str::<Value>(text).unwrap_or(Value::Null);
        let defaults = Self::default();
        Self {
            notifications: notifications_from(root.get("notifications"), defaults.notifications),
            sound: sound_from(root.get("sound"), defaults.sound),
            island: island_from(root.get("island"), defaults.island),
            sessions: sessions_from(root.get("sessions"), defaults.sessions),
            display: display_from(root.get("display"), defaults.display),
            integrations: integrations_from(root.get("integrations"), defaults.integrations),
            usage: usage_from(root.get("usage"), defaults.usage),
            filters: filters_from(root.get("filters"), defaults.filters),
            updates: updates_from(root.get("updates"), defaults.updates),
        }
    }

    pub fn to_json_value(&self) -> Value {
        json!({
            "notifications": {
                "idle_reminder_after_ms": as_millis(self.notifications.idle_reminder_after),
                "reminder_needs_response": self.notifications.reminder_needs_response,
                "reminder_completed_tasks": self.notifications.reminder_completed_tasks,
                "expand_on_completion": self.notifications.expand_on_completion,
                "expand_on_question": self.notifications.expand_on_question,
                "subagent_timing": self.notifications.subagent_timing.as_str(),
            },
            "sound": {
                "enabled": self.sound.enabled,
                "volume": as_volume(self.sound.volume),
                "quiet": self.sound.quiet,
                "follow_dnd": self.sound.follow_dnd,
                "quiet_hours": self.sound.quiet_hours,
                "quiet_hours_start": self.sound.quiet_hours_start,
                "quiet_hours_end": self.sound.quiet_hours_end,
                "spam_window_ms": as_millis(self.sound.spam_window),
                "spam_threshold": self.sound.spam_threshold,
                "events": {
                    "session_start": as_sound_path(self.sound.events.session_start.as_ref()),
                    "task_complete": as_sound_path(self.sound.events.task_complete.as_ref()),
                    "approval_needed": as_sound_path(self.sound.events.approval_needed.as_ref()),
                    "task_acknowledge": as_sound_path(self.sound.events.task_acknowledge.as_ref()),
                    "idle_reminder": as_sound_path(self.sound.events.idle_reminder.as_ref()),
                    "context_limit": as_sound_path(self.sound.events.context_limit.as_ref()),
                    "user_spam": as_sound_path(self.sound.events.user_spam.as_ref()),
                },
            },
            "island": {
                "hover_dwell_ms": as_millis(self.island.hover_dwell),
                "auto_collapse_ms": as_millis(self.island.auto_collapse),
                "idle_fade": self.island.idle_fade_enabled,
                "idle_fade_ms": as_millis(self.island.idle_fade),
                "expand_on_hover": self.island.expand_on_hover,
                "collapse_on_leave": self.island.collapse_on_leave,
                "hide_in_fullscreen": self.island.hide_in_fullscreen,
                "hide_when_idle": self.island.hide_when_idle,
                "click_to_jump": self.island.click_to_jump,
                "smart_suppression": self.island.smart_suppression,
            },
            "sessions": {
                "idle_after_ms": as_millis(self.sessions.idle_after),
                "cleanup_after_ms": as_millis(self.sessions.cleanup_after),
            },
            "display": {
                "compact_layout": self.display.compact_layout.as_str(),
                "monitor": self.display.monitor.clone().unwrap_or_default(),
                "notch_width_offset": self.display.notch_width_offset,
                "notch_height_offset": self.display.notch_height_offset,
                "island_height": self.display.island_height,
                "project": self.display.project,
                "worktree": self.display.worktree,
                "agent_icons": self.display.agent_icons,
                "terminal_icons": self.display.terminal_icons,
                "model": self.display.model,
                "effort": self.display.effort,
                "activity": self.display.activity,
                "subagents": self.display.subagents,
                "tasks": self.display.tasks,
                "ui_scale": as_ui_scale(self.display.ui_scale),
                "content_font": self.display.content_font,
                "panel_max_width": self.display.panel_max_width,
                "panel_max_height": self.display.panel_max_height,
                "completion_card_height": self.display.completion_card_height,
            },
            "integrations": {
                "macos_terminal": &self.integrations.macos_terminal,
                "auto_configure": self.integrations.auto_configure,
                "known_agents": &self.integrations.known_agents,
            },
            "usage": {
                "show_limits": self.usage.show_limits,
                "use_claude_login": self.usage.use_claude_login,
                "value_mode": self.usage.value_mode.as_str(),
                "preferred_provider": self.usage.preferred_provider.as_str(),
                "show_reset_cards": self.usage.show_reset_cards,
                "codex_credit_display": self.usage.codex_credit_display.as_str(),
                "warn_threshold": as_threshold(self.usage.warn_threshold),
                "refresh_interval_ms": as_millis(self.usage.refresh_interval),
            },
            "filters": {
                "rules": self.filters.rules.iter().map(as_rule).collect::<Vec<_>>(),
                "launchers": self
                    .filters
                    .launchers
                    .iter()
                    .map(as_launcher)
                    .collect::<Vec<_>>(),
                "quiet": {
                    "focus_mode": self.filters.quiet_focus_mode,
                    "screen_off": self.filters.quiet_screen_off,
                },
            },
            "updates": {
                "check_enabled": self.updates.check_enabled,
            },
        })
    }
}

pub fn path() -> Option<PathBuf> {
    std::env::var_os("OPEN_ISLAND_CONFIG")
        .map(PathBuf::from)
        .or_else(|| crate::paths::config_dir().map(|dir| dir.join("config.json")))
}

#[cfg(test)]
fn path_from(
    config_override: Option<OsString>,
    xdg_config_home: Option<OsString>,
    home: Option<OsString>,
) -> Option<PathBuf> {
    if let Some(path) = config_override {
        return Some(PathBuf::from(path));
    }
    let base = xdg_config_home
        .map(PathBuf::from)
        .or_else(|| home.map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("open-island").join("config.json"))
}

pub fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent", path.display()))?;
    fs::create_dir_all(parent).map_err(|error| format!("create {}: {error}", parent.display()))?;
    let temp = parent.join(format!(".open-island-{}.tmp", std::process::id()));
    let result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&temp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(|error| format!("write {}: {error}", path.display()))
}

pub fn save(path: &Path, config: &Config) -> Result<(), String> {
    let existing = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .filter(Value::is_object)
        .unwrap_or(Value::Null);
    let merged = merge_document(existing, config.to_json_value());
    let text = serde_json::to_string_pretty(&merged)
        .map_err(|error| format!("serialize config: {error}"))?;
    write_atomic(path, &format!("{text}\n"))
}

fn merge_document(existing: Value, fresh: Value) -> Value {
    match (existing, fresh) {
        (Value::Object(mut document), Value::Object(incoming)) => {
            for (key, value) in incoming {
                let merged = match document.remove(&key) {
                    Some(previous) => merge_document(previous, value),
                    None => value,
                };
                document.insert(key, merged);
            }
            Value::Object(document)
        }
        (_, fresh) => fresh,
    }
}

fn as_millis(duration: Duration) -> u64 {
    duration.as_millis() as u64
}

fn as_ui_scale(value: f32) -> Value {
    Value::from((f64::from(value) * 100.0).round() / 100.0)
}

fn as_volume(volume: f32) -> f64 {
    (f64::from(volume) * 1000.0).round() / 1000.0
}

fn as_sound_path(path: Option<&PathBuf>) -> Value {
    match path {
        Some(path) => Value::String(path.to_string_lossy().into_owned()),
        None => Value::Null,
    }
}

fn notifications_from(value: Option<&Value>, defaults: NotificationConfig) -> NotificationConfig {
    NotificationConfig {
        idle_reminder_after: milliseconds(
            value,
            "idle_reminder_after_ms",
            defaults.idle_reminder_after,
        ),
        reminder_needs_response: boolean(
            value,
            "reminder_needs_response",
            defaults.reminder_needs_response,
        ),
        reminder_completed_tasks: boolean(
            value,
            "reminder_completed_tasks",
            defaults.reminder_completed_tasks,
        ),
        expand_on_completion: boolean(value, "expand_on_completion", defaults.expand_on_completion),
        expand_on_question: boolean(value, "expand_on_question", defaults.expand_on_question),
        subagent_timing: value
            .and_then(|section| section.get("subagent_timing"))
            .and_then(Value::as_str)
            .and_then(SubagentTiming::parse)
            .unwrap_or(defaults.subagent_timing),
    }
}

fn sound_from(value: Option<&Value>, defaults: SoundConfig) -> SoundConfig {
    let events = value.and_then(|sound| sound.get("events"));
    SoundConfig {
        enabled: boolean(value, "enabled", defaults.enabled),
        volume: volume(value, defaults.volume),
        quiet: boolean(value, "quiet", defaults.quiet),
        follow_dnd: boolean(value, "follow_dnd", defaults.follow_dnd),
        quiet_hours: boolean(value, "quiet_hours", defaults.quiet_hours),
        quiet_hours_start: minute_of_day(value, "quiet_hours_start", defaults.quiet_hours_start),
        quiet_hours_end: minute_of_day(value, "quiet_hours_end", defaults.quiet_hours_end),
        spam_window: milliseconds(value, "spam_window_ms", defaults.spam_window),
        spam_threshold: pixels(value, "spam_threshold", 2, 20, defaults.spam_threshold),
        events: SoundEvents {
            session_start: sound_path(events, "session_start", defaults.events.session_start),
            task_complete: sound_path(events, "task_complete", defaults.events.task_complete),
            approval_needed: sound_path(events, "approval_needed", defaults.events.approval_needed),
            task_acknowledge: sound_path(
                events,
                "task_acknowledge",
                defaults.events.task_acknowledge,
            ),
            idle_reminder: sound_path(events, "idle_reminder", defaults.events.idle_reminder),
            context_limit: sound_path(events, "context_limit", defaults.events.context_limit),
            user_spam: sound_path(events, "user_spam", defaults.events.user_spam),
        },
    }
}

fn island_from(value: Option<&Value>, defaults: IslandConfig) -> IslandConfig {
    IslandConfig {
        hover_dwell: milliseconds(value, "hover_dwell_ms", defaults.hover_dwell),
        auto_collapse: milliseconds(value, "auto_collapse_ms", defaults.auto_collapse),
        idle_fade: milliseconds(value, "idle_fade_ms", defaults.idle_fade),
        idle_fade_enabled: boolean(value, "idle_fade", defaults.idle_fade_enabled),
        expand_on_hover: boolean(value, "expand_on_hover", defaults.expand_on_hover),
        collapse_on_leave: boolean(value, "collapse_on_leave", defaults.collapse_on_leave),
        hide_in_fullscreen: boolean(value, "hide_in_fullscreen", defaults.hide_in_fullscreen),
        hide_when_idle: boolean(value, "hide_when_idle", defaults.hide_when_idle),
        click_to_jump: boolean(value, "click_to_jump", defaults.click_to_jump),
        smart_suppression: boolean(value, "smart_suppression", defaults.smart_suppression),
    }
}

fn sessions_from(value: Option<&Value>, defaults: SessionsConfig) -> SessionsConfig {
    SessionsConfig {
        idle_after: milliseconds(value, "idle_after_ms", defaults.idle_after),
        cleanup_after: milliseconds(value, "cleanup_after_ms", defaults.cleanup_after),
    }
}

fn display_from(value: Option<&Value>, defaults: DisplayConfig) -> DisplayConfig {
    DisplayConfig {
        notch_width_offset: offset(
            value,
            "notch_width_offset",
            NOTCH_OFFSET_LIMIT,
            defaults.notch_width_offset,
        ),
        notch_height_offset: offset(
            value,
            "notch_height_offset",
            NOTCH_OFFSET_LIMIT,
            defaults.notch_height_offset,
        ),
        island_height: pixels(
            value,
            "island_height",
            0,
            MAX_ISLAND_HEIGHT,
            defaults.island_height,
        ),
        monitor: value
            .and_then(|section| section.get("monitor"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .or_else(|| defaults.monitor.clone()),
        compact_layout: choice(
            value,
            "compact_layout",
            CompactLayout::parse,
            defaults.compact_layout,
        ),
        project: boolean(value, "project", defaults.project),
        worktree: boolean(value, "worktree", defaults.worktree),
        agent_icons: boolean(value, "agent_icons", defaults.agent_icons),
        terminal_icons: boolean(value, "terminal_icons", defaults.terminal_icons),
        model: boolean(value, "model", defaults.model),
        effort: boolean(value, "effort", defaults.effort),
        activity: boolean(value, "activity", defaults.activity),
        subagents: boolean(value, "subagents", defaults.subagents),
        tasks: boolean(value, "tasks", defaults.tasks),
        ui_scale: ui_scale(value, defaults.ui_scale),
        content_font: pixels(value, "content_font", 9, 16, defaults.content_font),
        panel_max_width: pixels(
            value,
            "panel_max_width",
            480,
            1000,
            defaults.panel_max_width,
        ),
        panel_max_height: pixels(
            value,
            "panel_max_height",
            320,
            1200,
            defaults.panel_max_height,
        ),
        completion_card_height: pixels(
            value,
            "completion_card_height",
            60,
            240,
            defaults.completion_card_height,
        ),
    }
}

fn minute_of_day(section: Option<&Value>, key: &str, fallback: u32) -> u32 {
    section
        .and_then(|value| value.get(key))
        .and_then(Value::as_u64)
        .filter(|value| *value < u64::from(MINUTES_PER_DAY))
        .map(|value| value as u32)
        .unwrap_or(fallback)
}

fn offset(section: Option<&Value>, key: &str, limit: i32, fallback: i32) -> i32 {
    section
        .and_then(|value| value.get(key))
        .and_then(Value::as_i64)
        .map(|value| (value as i32).clamp(-limit, limit))
        .unwrap_or(fallback)
}

fn pixels(section: Option<&Value>, key: &str, min: u32, max: u32, fallback: u32) -> u32 {
    section
        .and_then(|value| value.get(key))
        .and_then(Value::as_u64)
        .map(|value| (value as u32).clamp(min, max))
        .unwrap_or(fallback)
}

fn integrations_from(value: Option<&Value>, defaults: IntegrationsConfig) -> IntegrationsConfig {
    IntegrationsConfig {
        macos_terminal: value
            .and_then(|value| value.get("macos_terminal"))
            .and_then(Value::as_str)
            .filter(|name| matches!(*name, "terminal" | "iterm2" | "warp" | "wezterm" | "kitty"))
            .unwrap_or(&defaults.macos_terminal)
            .to_owned(),
        auto_configure: boolean(value, "auto_configure", defaults.auto_configure),
        known_agents: known_agents(value, defaults.known_agents),
    }
}

fn updates_from(value: Option<&Value>, defaults: UpdatesConfig) -> UpdatesConfig {
    UpdatesConfig {
        check_enabled: boolean(value, "check_enabled", defaults.check_enabled),
    }
}

fn usage_from(value: Option<&Value>, defaults: UsageConfig) -> UsageConfig {
    UsageConfig {
        show_limits: boolean(value, "show_limits", defaults.show_limits),
        use_claude_login: boolean(value, "use_claude_login", defaults.use_claude_login),
        value_mode: choice(
            value,
            "value_mode",
            UsageValueMode::parse,
            defaults.value_mode,
        ),
        preferred_provider: choice(
            value,
            "preferred_provider",
            UsageProviderChoice::parse,
            defaults.preferred_provider,
        ),
        show_reset_cards: boolean(value, "show_reset_cards", defaults.show_reset_cards),
        codex_credit_display: choice(
            value,
            "codex_credit_display",
            CodexCreditDisplay::parse,
            defaults.codex_credit_display,
        ),
        warn_threshold: threshold(value, defaults.warn_threshold),
        refresh_interval: milliseconds(value, "refresh_interval_ms", defaults.refresh_interval)
            .max(MIN_USAGE_REFRESH),
    }
}

fn choice<T: Copy>(
    section: Option<&Value>,
    key: &str,
    parse: fn(&str) -> Option<T>,
    fallback: T,
) -> T {
    section
        .and_then(|value| value.get(key))
        .and_then(Value::as_str)
        .and_then(parse)
        .unwrap_or(fallback)
}

fn threshold(section: Option<&Value>, fallback: f32) -> f32 {
    section
        .and_then(|value| value.get("warn_threshold"))
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(50.0, 100.0) as f32)
        .unwrap_or(fallback)
}

fn as_threshold(value: f32) -> f64 {
    (f64::from(value) * 10.0).round() / 10.0
}

fn filters_from(value: Option<&Value>, defaults: FiltersConfig) -> FiltersConfig {
    let quiet = value.and_then(|section| section.get("quiet"));
    FiltersConfig {
        rules: rules(value, defaults.rules),
        launchers: launchers(value, defaults.launchers),
        quiet_focus_mode: boolean(quiet, "focus_mode", defaults.quiet_focus_mode),
        quiet_screen_off: boolean(quiet, "screen_off", defaults.quiet_screen_off),
    }
}

fn launchers(section: Option<&Value>, fallback: Vec<LauncherRule>) -> Vec<LauncherRule> {
    let Some(Value::Array(entries)) = section.and_then(|value| value.get("launchers")) else {
        return fallback;
    };
    entries.iter().filter_map(launcher).collect()
}

fn launcher(entry: &Value) -> Option<LauncherRule> {
    let app_id = entry.get("app_id").and_then(Value::as_str)?.trim();
    if app_id.is_empty() {
        return None;
    }
    Some(LauncherRule {
        app_id: app_id.to_owned(),
        name: entry
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(app_id)
            .to_owned(),
        enabled: entry
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true),
    })
}

fn as_launcher(rule: &LauncherRule) -> Value {
    json!({
        "app_id": rule.app_id,
        "name": rule.name,
        "enabled": rule.enabled,
    })
}

fn rules(section: Option<&Value>, fallback: Vec<SilenceRule>) -> Vec<SilenceRule> {
    let Some(Value::Array(entries)) = section.and_then(|value| value.get("rules")) else {
        return fallback;
    };
    entries.iter().filter_map(rule).collect()
}

fn rule(entry: &Value) -> Option<SilenceRule> {
    let pattern = entry.get("pattern").and_then(Value::as_str)?.trim();
    if pattern.is_empty() {
        return None;
    }
    let text = |key: &str| entry.get(key).and_then(Value::as_str);
    Some(SilenceRule {
        field: text("field")
            .and_then(RuleField::parse)
            .unwrap_or(RuleField::Cwd),
        match_type: text("match_type")
            .and_then(MatchType::parse)
            .unwrap_or(MatchType::Contains),
        pattern: pattern.to_owned(),
        name: text("name")
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(pattern)
            .to_owned(),
        built_in: entry
            .get("built_in")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        enabled: entry
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true),
    })
}

fn as_rule(rule: &SilenceRule) -> Value {
    json!({
        "field": rule.field.as_str(),
        "match_type": rule.match_type.as_str(),
        "pattern": rule.pattern,
        "name": rule.name,
        "built_in": rule.built_in,
        "enabled": rule.enabled,
    })
}

fn known_agents(section: Option<&Value>, fallback: Vec<String>) -> Vec<String> {
    let Some(Value::Array(entries)) = section.and_then(|value| value.get("known_agents")) else {
        return fallback;
    };
    let mut names = entries
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

fn boolean(section: Option<&Value>, key: &str, fallback: bool) -> bool {
    section
        .and_then(|value| value.get(key))
        .and_then(Value::as_bool)
        .unwrap_or(fallback)
}

fn milliseconds(section: Option<&Value>, key: &str, fallback: Duration) -> Duration {
    section
        .and_then(|value| value.get(key))
        .and_then(Value::as_u64)
        .map(Duration::from_millis)
        .unwrap_or(fallback)
}

fn ui_scale(section: Option<&Value>, fallback: f32) -> f32 {
    section
        .and_then(|value| value.get("ui_scale"))
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .map(|value| {
            if value <= 0.0 {
                0.0
            } else {
                value.clamp(1.0, 3.0) as f32
            }
        })
        .unwrap_or(fallback)
}

fn volume(section: Option<&Value>, fallback: f32) -> f32 {
    section
        .and_then(|value| value.get("volume"))
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(0.0, 1.0) as f32)
        .unwrap_or(fallback)
}

fn sound_path(events: Option<&Value>, key: &str, fallback: Option<PathBuf>) -> Option<PathBuf> {
    match events.and_then(|value| value.get(key)) {
        None => fallback,
        Some(Value::Null) => None,
        Some(Value::String(text)) if !text.trim().is_empty() => Some(PathBuf::from(text)),
        Some(_) => fallback,
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
