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
    bounded(&message).unwrap_or_else(|| {
        bounded(&serde_json::json!({"v":1,"id":message.id,"ok":false,"error":"snapshot_requires_paging"}))
            .unwrap_or_else(|| r#"{"v":1,"id":null,"ok":false,"error":"snapshot_requires_paging"}"#.to_owned())
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
    bounded(&Event {
        v: 1,
        event: event.to_owned(),
        data,
    })
    .unwrap_or_else(|| r#"{"v":1,"event":"snapshot-requires-paging","data":null}"#.to_owned())
}

fn bounded(value: &impl serde::Serialize) -> Option<String> {
    struct Limited(Vec<u8>);
    impl std::io::Write for Limited {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.0.len().saturating_add(bytes.len()) >= super::outbox::MAX_FRAME {
                return Err(std::io::Error::other("snapshot_requires_paging"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Limited(Vec::new());
    serde_json::to_writer(&mut writer, value).ok()?;
    String::from_utf8(writer.0).ok()
}

#[cfg(test)]
mod tests {
    use super::{event_message, response};
    use open_island_core::protocol::{EventData, QuietScenes};
    use serde_json::{json, Value};

    #[test]
    fn oversized_legacy_reply_is_small_explicit_error() {
        let wire = response(json!(42), Ok(json!({"text":"x".repeat(5 * 1024 * 1024)})));
        let value: Value = serde_json::from_str(&wire).unwrap();
        assert_eq!(value["id"], 42);
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"], "snapshot_requires_paging");
        assert!(wire.len() < 256);
    }

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
