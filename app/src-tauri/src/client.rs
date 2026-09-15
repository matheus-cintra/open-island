use crate::daemon_transport::Transport;
use open_island_core::{protocol::ApprovalDecision, session::Session};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
#[cfg(any(test, not(feature = "qa-harness")))]
use std::{env, path::PathBuf};
use tauri::{AppHandle, Emitter};

pub struct DaemonClient {
    app: AppHandle,
    _refresh_worker: crate::daemon_transport::refresh::Worker,
    refresh: Arc<crate::daemon_transport::refresh::Refresh>,
    recovery: Arc<Mutex<crate::message_recovery::MessageRecovery>>,
    transport: Arc<Transport>,
}

#[cfg(any(test, not(feature = "qa-harness")))]
pub fn daemon_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("OPEN_ISLANDD") {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(current) = env::current_exe() {
        if let Some(dir) = current.parent() {
            candidates.push(dir.join("open-islandd"));
            candidates.push(dir.join("../target/debug/open-islandd"));
        }
    }
    candidates
}

impl DaemonClient {
    pub fn start(app: AppHandle) -> Result<Self, String> {
        let path = open_island_core::paths::socket();
        let spawn_path = path.clone();
        let refresh = Arc::new(crate::daemon_transport::refresh::Refresh::default());
        let recovery = Arc::new(Mutex::new(
            crate::message_recovery::MessageRecovery::new().map_err(|e| e.to_string())?,
        ));
        let events_refresh = refresh.clone();
        let events_app = app.clone();
        let transport = Arc::new(Transport::start(
            path,
            Arc::new(move |event, data| {
                events_refresh.event(event, &data);
                let _ = events_app.emit(event, data);
                if event == "daemon-connection" {
                    let _ = events_app.emit("daemon-ui-state", events_refresh.cache());
                }
            }),
            Box::new(move || {
                #[cfg(not(feature = "qa-harness"))]
                for candidate in daemon_candidates() {
                    if std::process::Command::new(candidate)
                        .arg("--socket")
                        .arg(&spawn_path)
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn()
                        .is_ok()
                    {
                        break;
                    }
                }
                #[cfg(feature = "qa-harness")]
                let _ = spawn_path;
            }),
        )?);
        let updates = recovery.clone();
        let client_app = app.clone();
        let worker = refresh
            .start(
                transport.clone(),
                Arc::new(move |cache| {
                    if cache.phase == crate::daemon_transport::refresh::Phase::Connected {
                        if let Some(snapshot) = &cache.snapshot {
                            if let Ok(mut recovery) = updates.lock() {
                                recovery.reconcile(
                                    &snapshot.snapshot.daemon_epoch,
                                    &snapshot.snapshot.message_deliveries,
                                );
                            }
                        }
                    }
                    let _ = app.emit("daemon-ui-state", cache);
                    if let Ok(recovery) = updates.lock() {
                        let _ = app.emit("message-recovery", recovery.snapshot());
                    }
                }),
            )
            .map_err(|e| e.to_string())?;
        Ok(Self {
            app: client_app,
            _refresh_worker: worker,
            refresh,
            transport,
            recovery,
        })
    }

    fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        self.transport.request(method, params)
    }

    #[cfg(target_os = "macos")]
    pub fn report_focus(&self, focus: Option<bool>) -> Result<(), String> {
        self.request("native_state", json!({"focus": focus}))
            .map(|_| ())
    }

    pub fn play_sound(&self, path: &str) -> Result<(), String> {
        self.request("play_sound", json!({ "path": path }))
            .map(|_| ())
    }

    pub fn get_ui_state(&self) -> Result<crate::daemon_transport::sync::SyncedSnapshot, String> {
        self.refresh
            .cache()
            .snapshot
            .ok_or("daemon_state_unavailable".into())
    }
    pub fn get_daemon_ui_state(&self) -> crate::daemon_transport::refresh::UiCache {
        self.refresh.cache()
    }

    pub fn list_sessions(&self) -> Result<Vec<Session>, String> {
        let data = self.request("list_sessions", json!({}))?;
        serde_json::from_value(data)
            .map_err(|error| format!("invalid daemon session list: {error}"))
    }

    pub fn get_config(&self) -> Result<Value, String> {
        self.request("get_config", json!({}))
    }

    pub fn get_usage(&self) -> Result<Value, String> {
        self.request("get_usage", json!({}))
    }

    pub fn get_update(&self) -> Result<Value, String> {
        self.request("get_update", json!({}))
    }

    pub fn check_update(&self) -> Result<Value, String> {
        self.request("check_update", json!({}))
    }

    fn require_ready(
        &self,
        identity: &open_island_core::message_delivery::DeliveryIdentity,
    ) -> Result<(), String> {
        let cache = self.refresh.cache();
        if cache.phase == crate::daemon_transport::refresh::Phase::Incompatible {
            return Err("daemon_incompatible".into());
        }
        if cache.phase != crate::daemon_transport::refresh::Phase::Connected {
            return Err("daemon_unavailable".into());
        }
        let snapshot = cache.snapshot.ok_or("daemon_unavailable")?;
        if snapshot.snapshot.daemon_epoch != identity.daemon_epoch {
            return Err("stale_epoch".into());
        }
        Ok(())
    }

    pub fn message_recovery(&self) -> Result<Vec<crate::message_recovery::RecoveryRecord>, String> {
        Ok(self
            .recovery
            .lock()
            .map_err(|_| "recovery_unavailable")?
            .snapshot())
    }
    fn publish_recovery(&self) {
        if let Ok(records) = self.message_recovery() {
            let _ = self.app.emit("message-recovery", records);
        }
    }
    pub fn discard_message_recovery(
        &self,
        id: open_island_core::message_delivery::ClientSubmissionId,
    ) -> Result<bool, String> {
        let removed = self
            .recovery
            .lock()
            .map_err(|_| "recovery_unavailable")?
            .discard(&id);
        self.publish_recovery();
        Ok(removed)
    }
    pub fn send_message_v2(
        &self,
        id: &str,
        text: &str,
        identity: open_island_core::message_delivery::DeliveryIdentity,
    ) -> Result<Value, String> {
        self.require_ready(&identity)?;
        let submission = self
            .recovery
            .lock()
            .map_err(|_| "recovery_unavailable")?
            .reserve(&identity, text.to_owned())?;
        let result = self.request("send_message_v2", json!({"id":id,"text":text,"daemon_epoch":identity.daemon_epoch,"session_instance_id":identity.session_instance_id,"client_submission_id":submission}));
        let mut recovery = self.recovery.lock().map_err(|_| "recovery_unavailable")?;
        let outcome = match result {
            Ok(mut receipt) => {
                let Some(message_id) = receipt.get("message_id").and_then(Value::as_u64) else {
                    recovery.failed(&submission, true);
                    drop(recovery);
                    self.publish_recovery();
                    return Err("invalid_admission_receipt".into());
                };
                recovery.admitted(&submission, &identity.daemon_epoch, message_id);
                receipt["client_submission_id"] = json!(submission);
                Ok(receipt)
            }
            Err(error) => {
                recovery.failed(
                    &submission,
                    matches!(
                        error.as_str(),
                        "daemon_unavailable" | "daemon_response_timeout" | "transport_unavailable"
                    ),
                );
                Err(error)
            }
        };
        drop(recovery);
        self.publish_recovery();
        outcome
    }

    pub fn send_message(&self, id: &str, text: &str) -> Result<Value, String> {
        self.request("send_message", json!({"id": id, "text": text}))
    }

    pub fn jump_v2(
        &self,
        id: &str,
        identity: open_island_core::message_delivery::DeliveryIdentity,
    ) -> Result<(), String> {
        self.require_ready(&identity)?;
        self.request("jump_v2", json!({"id":id,"daemon_epoch":identity.daemon_epoch,"session_instance_id":identity.session_instance_id})).map(|_| ())
    }
    pub fn resolve_approval_v2(
        &self,
        approval_id: &str,
        pending_generation: u64,
        decision: ApprovalDecision,
        identity: open_island_core::message_delivery::DeliveryIdentity,
    ) -> Result<(), String> {
        self.require_ready(&identity)?;
        self.request("resolve_approval_v2", json!({"approval_id":approval_id,"pending_generation":pending_generation,"decision":decision,"daemon_epoch":identity.daemon_epoch,"session_instance_id":identity.session_instance_id})).map(|_| ())
    }
    pub fn answer_question_v2(
        &self,
        question_id: &str,
        pending_generation: u64,
        answers: Vec<Vec<String>>,
        identity: open_island_core::message_delivery::DeliveryIdentity,
    ) -> Result<(), String> {
        self.require_ready(&identity)?;
        self.request("answer_question_v2", json!({"question_id":question_id,"pending_generation":pending_generation,"answers":answers,"daemon_epoch":identity.daemon_epoch,"session_instance_id":identity.session_instance_id})).map(|_| ())
    }
    pub fn cancel_message_v2(
        &self,
        id: &str,
        message_id: u64,
        identity: open_island_core::message_delivery::DeliveryIdentity,
    ) -> Result<(), String> {
        self.require_ready(&identity)?;
        self.request("cancel_message_v2", json!({"id":id,"message_id":message_id,"daemon_epoch":identity.daemon_epoch,"session_instance_id":identity.session_instance_id}))?;
        self.recovery
            .lock()
            .map_err(|_| "recovery_unavailable")?
            .confirmed_cancel(&identity, message_id);
        self.publish_recovery();
        Ok(())
    }
    pub fn cancel_message(&self, id: &str, message_id: u64) -> Result<(), String> {
        let _ = self.request(
            "cancel_message",
            json!({"id": id, "message_id": message_id}),
        )?;
        Ok(())
    }

    pub fn jump(&self, id: &str) -> Result<(), String> {
        let _ = self.request("jump", json!({"id": id}))?;
        Ok(())
    }

    pub fn answer_question(
        &self,
        question_id: &str,
        answers: Vec<Vec<String>>,
    ) -> Result<(), String> {
        let _ = self.request(
            "answer_question",
            json!({"question_id": question_id, "answers": answers}),
        )?;
        Ok(())
    }

    pub fn resolve_approval(
        &self,
        approval_id: &str,
        decision: ApprovalDecision,
    ) -> Result<(), String> {
        let _ = self.request(
            "resolve_approval",
            json!({"approval_id": approval_id, "decision": decision}),
        )?;
        Ok(())
    }
}
