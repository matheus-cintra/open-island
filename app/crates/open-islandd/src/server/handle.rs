use super::wire::{
    response, AnswerParams, CancelParams, JumpParams, PlayParams, ResolveParams, SendParams,
};
use crate::broadcast::{broadcast_except, broadcast_sessions, make_broadcast, make_hook_broadcast};
use crate::daemon_config::{approval_timeout, env_locked, question_timeout, VERSION};
use crate::notifications::approval::QuestionSettlement;
use crate::notifications::lifecycle::{self, DaemonContext};
use crate::poll::{refresh_update, send_message};
use crate::update;
use open_island_core::{discovery, jump, protocol::Request};
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
            Ok(json!({"daemon":"open-islandd", "version": VERSION, "pid": std::process::id()}))
        }
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
            let sessions = discovery::scan();
            ctx.state
                .lock()
                .map_err(|_| "daemon state unavailable".to_owned())
                .map(|mut state| {
                    serde_json::to_value(state.store.snapshot(&sessions))
                        .unwrap_or(Value::Array(Vec::new()))
                })
        }
        "jump" => {
            let params = request.params.unwrap_or(Value::Null);
            serde_json::from_value::<JumpParams>(params)
                .map_err(|error| format!("invalid jump params: {error}"))
                .and_then(|params| {
                    let processes = discovery::scan();
                    let sessions = ctx
                        .state
                        .lock()
                        .map_err(|error| format!("daemon state unavailable: {error}"))
                        .map(|mut state| state.store.snapshot(&processes))?;
                    sessions
                        .into_iter()
                        .find(|session| session.id == params.id)
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
        "send_message" => request
            .params
            .ok_or_else(|| "missing send_message params".to_owned())
            .and_then(|params| {
                serde_json::from_value::<SendParams>(params)
                    .map_err(|error| format!("invalid send_message params: {error}"))
            })
            .and_then(|params| send_message(&ctx, &params.id, &params.text)),
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
                    .map(|mut state| state.store.cancel_message(&params.id, params.message_id))?;
                if !cancelled {
                    return Err("message not queued".to_owned());
                }
                broadcast_sessions(&ctx);
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
