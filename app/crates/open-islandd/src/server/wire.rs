use open_island_core::protocol::{ApprovalDecision, Event, EventData, Response};
use serde::Deserialize;
use serde_json::Value;

pub fn response(id: Value, data: Result<Value, String>) -> String {
    let message = match data {
        Ok(data) => Response {
            v: 1,
            id,
            ok: true,
            data: Some(data),
            error: None,
        },
        Err(error) => Response {
            v: 1,
            id,
            ok: false,
            data: None,
            error: Some(error),
        },
    };
    serde_json::to_string(&message).unwrap_or_else(|_| {
        "{\"v\":1,\"id\":null,\"ok\":false,\"error\":\"serialization failed\"}".to_owned()
    })
}

#[derive(Deserialize)]
pub struct JumpParams {
    pub id: String,
}

#[derive(Deserialize)]
pub struct SendParams {
    pub id: String,
    pub text: String,
}

#[derive(Deserialize)]
pub struct CancelParams {
    pub id: String,
    pub message_id: u64,
}

#[derive(Deserialize)]
pub struct ResolveParams {
    pub approval_id: String,
    pub decision: ApprovalDecision,
}

#[derive(Deserialize)]
pub struct PlayParams {
    pub path: String,
}

#[derive(Deserialize)]
pub struct AnswerParams {
    pub question_id: String,
    pub answers: Vec<Vec<String>>,
}

pub fn event_message(event: &str, data: EventData) -> String {
    serde_json::to_string(&Event {
        v: 1,
        event: event.to_owned(),
        data,
    })
    .unwrap_or_else(|_| "{\"v\":1,\"event\":\"sessions-updated\",\"data\":[]}".to_owned())
}
