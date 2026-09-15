pub mod adapters;
pub mod config;
pub mod discovery;
pub mod filters;
pub mod focus;
pub mod input_bridge;
pub mod jump;
pub mod naming;
pub mod protocol;
pub mod resolvers;
pub mod runner;
pub mod send;
pub mod session;
pub mod store;
pub mod terminal;
pub mod usage;

pub use config::{Config, NotificationConfig, SoundConfig, SoundEvent, SoundEvents};
pub use protocol::{
    ApprovalDecision, ApprovalRequest, ApprovalResolution, ApprovalResolved, EventData,
    EventPayload, GenericEvent, HookEvent, HookEventKind, IslandToggle, Question, QuestionAnswer,
    QuestionFocus, QuestionOption, QuestionOutcome, QuestionRequest, QuestionResolved, V1Event,
};
pub use session::{Attention, HookId, HookSessionId, PermissionState, QuestionState, Session};
pub use store::{IdleReminder, SessionStore, IDLE_AFTER, MAX_PENDING_APPROVALS};
pub use usage::{
    ProviderUsage, ResetCard, ThresholdWatch, UsageCredits, UsageError, UsageModelWindow,
    UsageProvider, UsageReport, UsageSnapshot, UsageWindow, PROVIDER_ANTHROPIC, PROVIDER_CODEX,
};

pub mod paths;
pub mod process;
pub mod process_args;

pub mod message_delivery;
#[cfg(test)]
mod message_delivery_tests;

pub mod epoch;
pub mod unix_socket;

pub mod snapshot_page;

pub mod ui_state;

pub mod focus_guarded;

pub mod diagnostics;

pub mod hook_templates;
