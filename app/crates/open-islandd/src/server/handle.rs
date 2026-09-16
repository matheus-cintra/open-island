use super::wire::{
    response, AnswerParams, CancelParams, JumpParams, PlayParams, ResolveParams, SendParams,
};
use crate::broadcast::{broadcast_except, broadcast_sessions, make_broadcast, make_hook_broadcast};
use crate::daemon_config::{approval_timeout, env_locked, question_timeout, VERSION};
use crate::notifications::approval::QuestionSettlement;
use crate::notifications::lifecycle::{self, DaemonContext};
use crate::poll::{refresh_update, send_message};
use crate::update;
use open_island_core::{jump, protocol::Request};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc, time::Instant};

pub fn handle(ctx: DaemonContext, connection_id: u64, request: Request) -> String {
    let id = request.id.clone();
    let broadcast = make_broadcast(Arc::clone(&ctx.state));
    let result: Result<Value, String> = match request.method.as_str() {
        #[cfg(target_os = "macos")]
        "native_state" => serde_json::from_value::<crate::native_state::Report>(
            request.params.unwrap_or(Value::Null),
        )
        .map_err(|error| format!("invalid native state: {error}"))
        .map(|report| {
            crate::native_state::report(report);
            json!({"reported": true})
        }),
        "ping" => {
            let mut reply = json!({"daemon":"open-islandd", "version": VERSION, "pid": std::process::id(), "daemon_epoch": ctx.messages.epoch, "capabilities":open_island_core::diagnostics::CAPABILITIES});
            if request.params.as_ref().and_then(|params| params.get("client_role")).and_then(Value::as_str) == Some("diagnostic") {
                if let Ok(state) = ctx.state.lock() {
                    reply["counters"] = json!(open_island_core::diagnostics::Counters {
                        outbox: ctx.admission.outbox.snapshot(),
                        connections_legacy: ctx.admission.connections.snapshot(32),
                        connections_managed: ctx.admission.managed.snapshot(32),
                        fast_requests: ctx.admission.fast.snapshot(64),
                        blocking_requests: ctx.admission.blocking.snapshot(64),
                        bulk_requests: ctx.admission.bulk.snapshot(4),
                        no_island: state.no_island,
                        pending_approvals: state.pending.len() as u64,
                        pending_questions: state.pending_questions.len() as u64,
                    });
                }
            }
            Ok(reply)
        }
        "subscribe_ui" => ctx.state.lock().map_err(|_| "daemon_unavailable".to_owned()).and_then(|mut state| {
            let subscriber = state.subscribers.iter_mut().find(|s| s.connection_id == connection_id).ok_or("connection_not_found")?;
            subscriber.ui_epoch = Some(ctx.messages.epoch.clone());
            Ok(json!({"daemon_epoch":ctx.messages.epoch,"capabilities":open_island_core::diagnostics::CAPABILITIES}))
        }),
        "get_ui_state" => {
            let mut revision = 0;
            ctx.snapshots.begin_with(connection_id, || {
                let document = crate::ui_state::freeze(&ctx)?;
                revision = document.publication_revision;
                Ok(document)
            }).map(|id| json!({"snapshot_id":id,"daemon_epoch":ctx.messages.epoch,"publication_revision":revision}))
        },
        "get_ui_state_page" => request.params.as_ref().ok_or("missing_snapshot_params".to_owned()).and_then(|params| {
            let id = params.get("snapshot_id").and_then(Value::as_u64).ok_or("invalid_snapshot_id")?;
            let page = params.get("expected_page").and_then(Value::as_u64).ok_or("invalid_snapshot_page")?;
            ctx.snapshots.page(connection_id, id, page).and_then(|page| serde_json::to_value(page).map_err(|_| "snapshot_serialization".into()))
        }),
        "cancel_ui_state" => { ctx.snapshots.cancel(connection_id); Ok(Value::Null) },
        "get_message_deliveries" => ctx.state.lock().map_err(|_| "daemon_unavailable".to_owned()).map(|state| json!({"daemon_epoch":ctx.messages.epoch,"message_deliveries":state.store.deliveries.snapshot()})),
        "get_usage" => ctx
            .state
            .lock()
            .map_err(|_| "daemon state unavailable".to_owned())
            .and_then(|state| {
                serde_json::to_value(&state.usage)
                    .map_err(|error| format!("usage is not serialisable: {error}"))
            }),
        "get_update" => ctx
            .state
            .lock()
            .map_err(|_| "daemon state unavailable".to_owned())
            .and_then(|state| {
                serde_json::to_value(&state.update)
                    .map_err(|error| format!("update is not serialisable: {error}"))
            }),
        "play_sound" => request
            .params
            .ok_or_else(|| "missing play_sound params".to_owned())
            .and_then(|params| {
                serde_json::from_value::<PlayParams>(params)
                    .map_err(|error| format!("invalid play_sound params: {error}"))
            })
            .and_then(|params| {
                let volume = ctx.config.get().sound.volume;
                // A preview answers a click, so it ignores quiet, DND and quiet hours: the
                // user is asking to hear this one now.
                crate::sound::command(Path::new(&params.path), volume)
                    .spawn()
                    .map(|_| json!({"played": true}))
                    .map_err(|error| format!("{}: {error}", crate::sound::PLAYER))
            }),
        "get_config" => Ok(json!({
            "config": ctx.config.get().to_json_value(),
            "env_locked": env_locked(),
        })),
        "list_sessions" => {
            if !crate::poll::background_scanning(&ctx.state) {
                crate::poll::refresh_discovery_now(&ctx);
            }
            crate::discovery_cache::sessions(&ctx).and_then(|sessions| serde_json::to_value(sessions).map_err(|_| "snapshot_serialization".into()))
        }
        "jump_v2" => serde_json::from_value::<super::guarded_jump::Jump>(request.params.unwrap_or(Value::Null))
            .map_err(|_| "invalid_guarded_jump".to_owned())
            .and_then(|request| super::guarded_jump::jump(&ctx, request)),
        "jump" => {
            let params = request.params.unwrap_or(Value::Null);
            serde_json::from_value::<JumpParams>(params)
                .map_err(|error| format!("invalid jump params: {error}"))
                .and_then(|params| {
                    let sessions = crate::discovery_cache::sessions(&ctx)?;
                    sessions
                        .into_iter()
                        .find(|session| {
                            session.id == params.id
                                || (session.agent == "opencode"
                                    && session.subagents.as_ref().is_some_and(|children| {
                                        children.iter().any(|child| {
                                            format!("opencode:{}", child.id) == params.id
                                        })
                                    }))
                        })
                        .ok_or_else(|| format!("session '{}' not found", params.id))
                })
                .and_then(|session| {
                    if let (Some(hook_id), Ok(mut state)) =
                        (session.hook_id.as_ref(), ctx.state.lock())
                    {
                        state.store.mark_seen(hook_id, Instant::now());
                    }
                    jump::jump(&session).map(|()| Value::Null)
                })
        }
        "check_update" => {
            let cache_path = update::cache::path();
            refresh_update(&ctx, cache_path.as_deref(), true)
                .map(|notice| json!({"version": notice.map(|notice| notice.version)}))
        }
        "send_message_v2" => serde_json::from_value::<super::guarded_actions::Send>(request.params.unwrap_or(Value::Null))
            .map_err(|_| "invalid_guarded_send".to_owned())
            .and_then(|request| crate::poll::send_message_guarded(&ctx, request)),
        "send_message" => request
            .params
            .ok_or_else(|| "missing send_message params".to_owned())
            .and_then(|params| {
                serde_json::from_value::<SendParams>(params)
                    .map_err(|error| format!("invalid send_message params: {error}"))
            })
            .and_then(|params| send_message(&ctx, &params.id, &params.text)),
        "cancel_message_v2" => serde_json::from_value::<super::guarded_actions::Cancel>(request.params.unwrap_or(Value::Null))
            .map_err(|_| "invalid_guarded_cancel".to_owned())
            .and_then(|request| super::guarded_actions::cancel(&ctx, request)),
        "cancel_message" => request
            .params
            .ok_or_else(|| "missing cancel_message params".to_owned())
            .and_then(|params| {
                serde_json::from_value::<CancelParams>(params)
                    .map_err(|error| format!("invalid cancel_message params: {error}"))
            })
            .and_then(|params| {
                let cancelled = ctx
                    .state
                    .lock()
                    .map_err(|_| "daemon state unavailable".to_owned())
                    .and_then(|mut state| state.store.cancel_message(&params.id, params.message_id))?;
                if !cancelled {
                    return Err("message not queued".to_owned());
                }
                broadcast_sessions(&ctx);
                crate::message_executor::publish(&ctx.state);
                Ok(Value::Null)
            }),
        "toggle" => {
            let source = request
                .params
                .as_ref()
                .and_then(|params| params.get("source"))
                .and_then(Value::as_str)
                .unwrap_or("hotkey");
            let delivered = broadcast_except(
                &ctx.state,
                lifecycle::island_toggle_message(source),
                Some(connection_id),
            );
            Ok(json!({"delivered": delivered}))
        }
        "settings" => {
            let delivered = broadcast_except(
                &ctx.state,
                lifecycle::open_settings_message("cli"),
                Some(connection_id),
            );
            Ok(json!({"delivered": delivered}))
        }
        "hook_event" => request
            .params
            .ok_or_else(|| "missing hook_event params".to_owned())
            .and_then(|params| {
                serde_json::from_value(params)
                    .map_err(|error| format!("invalid hook_event params: {error}"))
            })
            .and_then(|event| {
                lifecycle::hook_event(
                    ctx.clone(),
                    connection_id,
                    event,
                    approval_timeout(),
                    question_timeout(),
                    make_hook_broadcast(Arc::clone(&ctx.state), connection_id),
                )
            }),
        "resolve_approval_v2" => serde_json::from_value::<super::guarded_actions::Resolve>(request.params.unwrap_or(Value::Null))
            .map_err(|_| "invalid_guarded_approval".to_owned())
            .and_then(|request| super::guarded_actions::resolve(&ctx, request)),
        "answer_question_v2" => serde_json::from_value::<super::guarded_actions::Answer>(request.params.unwrap_or(Value::Null))
            .map_err(|_| "invalid_guarded_question".to_owned())
            .and_then(|request| super::guarded_actions::answer(&ctx, request)),
        "resolve_approval" => request
            .params
            .ok_or_else(|| "missing resolve_approval params".to_owned())
            .and_then(|params| {
                serde_json::from_value::<ResolveParams>(params)
                    .map_err(|error| format!("invalid resolve_approval params: {error}"))
            })
            .and_then(|params| {
                lifecycle::resolve_if_current(
                    &ctx,
                    &params.approval_id,
                    None,
                    params.decision,
                    broadcast.clone(),
                )
                .map(|_| Value::Null)
            }),
        "answer_question" => request
            .params
            .ok_or_else(|| "missing answer_question params".to_owned())
            .and_then(|params| {
                serde_json::from_value::<AnswerParams>(params)
                    .map_err(|error| format!("invalid answer_question params: {error}"))
            })
            .and_then(|params| {
                lifecycle::settle_question(
                    &ctx,
                    &params.question_id,
                    None,
                    QuestionSettlement::Answered(params.answers),
                    broadcast.clone(),
                )
                .map(|_| Value::Null)
            }),
        method => Err(format!("unknown method '{method}'")),
    };
    response(id, result)
}
