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

#[cfg(test)]
mod tests {
    use super::{event_message, response};
    use open_island_core::protocol::{EventData, QuietScenes};
    use serde_json::{json, Value};

    #[test]
    fn an_ok_reply_carries_the_data_and_no_error() {
        let parsed: Value = serde_json::from_str(&response(json!(7), Ok(json!({"pong": true}))))
            .expect("the ok reply parses");
        assert_eq!(parsed["v"], json!(1));
        assert_eq!(parsed["id"], json!(7));
        assert_eq!(parsed["ok"], json!(true));
        assert_eq!(parsed["data"]["pong"], json!(true));
        assert!(parsed.get("error").is_none());
    }

    #[test]
    fn an_error_reply_carries_the_message_and_no_data() {
        let parsed: Value = serde_json::from_str(&response(json!("a"), Err("boom".to_owned())))
            .expect("the error reply parses");
        assert_eq!(parsed["v"], json!(1));
        assert_eq!(parsed["id"], json!("a"));
        assert_eq!(parsed["ok"], json!(false));
        assert_eq!(parsed["error"], json!("boom"));
        assert!(parsed.get("data").is_none());
    }

    #[test]
    fn an_event_envelope_names_the_event() {
        let scenes = QuietScenes {
            active: true,
            focus_mode: false,
            screen_off: true,
        };
        let parsed: Value = serde_json::from_str(&event_message(
            "quiet-scenes",
            EventData::QuietScenes(scenes),
        ))
        .expect("the event parses");
        assert_eq!(parsed["v"], json!(1));
        assert_eq!(parsed["event"], json!("quiet-scenes"));
        assert_eq!(
            parsed["data"],
            json!({"active": true, "focus_mode": false, "screen_off": true})
        );
    }
}
