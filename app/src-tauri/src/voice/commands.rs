use super::controller::{self, Controller, Target, VerifiedTarget, View};
use crate::{
    client::DaemonClient,
    daemon_transport::refresh::{Phase, UiCache},
};
use tauri::{Emitter, Manager, State, WebviewWindow};

fn main_window(label: &str) -> Result<(), &'static str> {
    if label == "main" {
        Ok(())
    } else {
        Err("voice_window_forbidden")
    }
}
fn verified(target: Target, cache: UiCache) -> Result<VerifiedTarget, &'static str> {
    verified_with_birth(target, cache, open_island_core::process::birth_identity)
}
fn verified_with_birth(
    target: Target,
    cache: UiCache,
    birth: impl FnOnce(u32) -> Option<open_island_core::process::ProcessBirthIdentity>,
) -> Result<VerifiedTarget, &'static str> {
    match cache.phase {
        Phase::Incompatible => return Err("daemon_incompatible"),
        Phase::Connected => {}
        _ => return Err("daemon_unavailable"),
    }
    let snapshot = cache.snapshot.ok_or("daemon_unavailable")?;
    let verified = VerifiedTarget::from_snapshot(target.clone(), &snapshot.snapshot)?;
    if !target.session_instance_id.0.starts_with("hook:") {
        let session = snapshot
            .snapshot
            .targets()
            .find(|session| {
                session.session.id == target.session_id
                    && session.session_instance_id.as_ref() == Some(&target.session_instance_id)
            })
            .ok_or("voice_target_stale")?;
        let birth = birth(session.session.pid).ok_or("voice_target_stale")?;
        let current = open_island_core::message_delivery::DeliveryIdentity::for_process(
            &session.session,
            birth,
            &target.daemon_epoch,
        );
        if current.session_instance_id != target.session_instance_id {
            return Err("voice_target_stale");
        }
    }
    Ok(verified)
}

#[tauri::command]
fn voice_start(
    window: WebviewWindow,
    client: State<'_, DaemonClient>,
    voice: State<'_, Controller>,
    target: Target,
) -> Result<View, String> {
    main_window(window.label())?;
    let original = target.clone();
    let target = verified(target, client.get_daemon_ui_state())?;
    let directory = open_island_core::paths::state_dir().ok_or("model_unavailable")?;
    let app = window.app_handle().clone();
    let completion_app = app.clone();
    let completion_target = original.clone();
    let levels = app.clone();
    voice
        .start_checked(
            target,
            controller::native_work(
                directory,
                move || {
                    // Model configuration IO may have taken time. Validate again at the
                    // boundary before the native permission request, without rescanning.
                    verified(original, app.state::<DaemonClient>().get_daemon_ui_state()).map(|_| ())
                },
                move |job, value| {
                    let _ = levels.emit_to(
                        "main",
                        "voice-level",
                        serde_json::json!({"job_id": job, "level": value}),
                    );
                },
            ),
            move || {
                verified(
                    completion_target,
                    completion_app.state::<DaemonClient>().get_daemon_ui_state(),
                )
                .is_ok()
            },
        )
        .map_err(str::to_owned)
}
#[tauri::command]
fn voice_get_state(window: WebviewWindow, voice: State<'_, Controller>) -> Result<View, String> {
    voice_state(window, voice)
}
#[tauri::command]
fn voice_state(window: WebviewWindow, voice: State<'_, Controller>) -> Result<View, String> {
    main_window(window.label())?;
    Ok(voice.snapshot())
}
#[tauri::command]
fn voice_stop(
    window: WebviewWindow,
    voice: State<'_, Controller>,
    job_id: String,
) -> Result<View, String> {
    main_window(window.label())?;
    voice.stop(&job_id).map_err(str::to_owned)
}
#[tauri::command]
fn voice_cancel(
    window: WebviewWindow,
    voice: State<'_, Controller>,
    job_id: String,
) -> Result<View, String> {
    main_window(window.label())?;
    voice.cancel(&job_id).map_err(str::to_owned)
}

pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::new("voice")
        .setup(|app, _| {
            let events = app.clone();
            app.manage(Controller::with_events(move |view| {
                let _ = events.emit_to("main", "voice-state", view);
            }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            voice_start,
            voice_get_state,
            voice_state,
            voice_stop,
            voice_cancel,
            super::settings::voice_model_status,
            super::settings::voice_select_model,
            super::settings::voice_clear_model,
            super::settings::voice_open_microphone_settings
        ])
        .on_event(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                app.state::<Controller>().shutdown();
            }
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_main_window_can_access_draft_audio_jobs() {
        assert!(main_window("main").is_ok());
        for label in ["settings", "preview", "", "main-child"] {
            assert_eq!(main_window(label), Err("voice_window_forbidden"));
        }
    }
    #[test]
    fn disconnected_or_incompatible_cache_is_rejected_before_job_creation() {
        let target = Target {
            session_id: "session".into(),
            session_instance_id: open_island_core::message_delivery::SessionInstanceId(
                "instance".into(),
            ),
            daemon_epoch: open_island_core::message_delivery::DaemonEpoch("epoch".into()),
        };
        for (phase, expected) in [
            (Phase::Connecting, "daemon_unavailable"),
            (Phase::Reconnecting, "daemon_unavailable"),
            (Phase::Incompatible, "daemon_incompatible"),
            (Phase::Connected, "daemon_unavailable"),
        ] {
            assert_eq!(
                verified(
                    target.clone(),
                    UiCache {
                        phase,
                        generation: 1,
                        snapshot: None
                    }
                )
                .err(),
                Some(expected)
            );
        }
    }
    #[test]
    fn completion_revalidates_disappearance_recycled_instance_and_restart() {
        use crate::daemon_transport::sync::SyncedSnapshot;
        use open_island_core::{
            session::Session,
            ui_state::{UiSession, UiSnapshot},
        };
        use std::{
            sync::{mpsc, Arc, Mutex},
            time::{Duration, Instant},
        };
        for change in ["removed", "recycled", "restart", "offline", "birth"] {
            let mut session = Session::new("codex", "/fixture", 42, "kitty");
            session.id = "same-id".into();
            let original = Target {
                session_id: session.id.clone(),
                session_instance_id:
                    open_island_core::message_delivery::DeliveryIdentity::for_process(
                        &session,
                        open_island_core::process::ProcessBirthIdentity::new(123),
                        &open_island_core::message_delivery::DaemonEpoch("epoch".into()),
                    )
                    .session_instance_id,
                daemon_epoch: open_island_core::message_delivery::DaemonEpoch("epoch".into()),
            };
            let expected_instance = original.session_instance_id.clone();
            let snapshot = UiSnapshot {
                schema_version: 1,
                discovering: false,
                daemon_epoch: original.daemon_epoch.clone(),
                publication_revision: 1,
                sessions: vec![UiSession {
                    session,
                    session_instance_id: Some(original.session_instance_id.clone()),
                }],
                child_sessions: vec![],
                approvals: vec![],
                questions: vec![],
                message_deliveries: vec![],
                config: serde_json::json!({}),
                usage: Default::default(),
                update: None,
                quiet_scenes: Default::default(),
            };
            let cache = Arc::new(Mutex::new(UiCache {
                phase: Phase::Connected,
                generation: 1,
                snapshot: Some(SyncedSnapshot {
                    generation: 1,
                    snapshot,
                }),
            }));
            let target =
                verified_with_birth(original.clone(), cache.lock().unwrap().clone(), |_| {
                    Some(open_island_core::process::ProcessBirthIdentity::new(123))
                })
                .unwrap();
            let controller = Controller::default();
            let (entered, receiving) = mpsc::channel();
            let (release, released) = mpsc::channel();
            let current = cache.clone();
            controller
                .start_checked(
                    target,
                    move |job| {
                        job.recording()?;
                        job.transcribing(123)?;
                        entered.send(()).unwrap();
                        released.recv_timeout(Duration::from_secs(3)).unwrap();
                        Ok("texto preservado".into())
                    },
                    move || {
                        verified_with_birth(original, current.lock().unwrap().clone(), |_| {
                            Some(open_island_core::process::ProcessBirthIdentity::new(
                                if change == "birth" { 124 } else { 123 },
                            ))
                        })
                        .is_ok()
                    },
                )
                .unwrap();
            receiving.recv_timeout(Duration::from_secs(3)).unwrap();
            {
                let mut cache = cache.lock().unwrap();
                match change {
                    "removed" => cache.snapshot.as_mut().unwrap().snapshot.sessions.clear(),
                    "recycled" => {
                        cache.snapshot.as_mut().unwrap().snapshot.sessions[0]
                            .session_instance_id
                            .as_mut()
                            .unwrap()
                            .0 = "replacement".into()
                    }
                    "restart" => {
                        cache.snapshot.as_mut().unwrap().snapshot.daemon_epoch.0 =
                            "next-epoch".into()
                    }
                    "offline" => cache.phase = Phase::Reconnecting,
                    _ => {}
                }
            }
            release.send(()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            while controller.snapshot().worker_active {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(2));
            }
            let ready = controller.snapshot();
            assert_eq!(ready.phase, controller::Phase::Ready);
            assert!(ready.target_unavailable);
            assert_eq!(ready.target.unwrap().session_instance_id, expected_instance);
            assert_eq!(ready.transcript.as_deref(), Some("texto preservado"));
        }
    }
}
