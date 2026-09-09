use open_island_core::{
    discovery, jump,
    protocol::{QuietScenes, Request},
    store::SessionStore,
};
use open_islandd::broadcast::{
    broadcast_except, broadcast_sessions, make_broadcast, make_hook_broadcast,
};
use open_islandd::config_handle::ConfigHandle;
use open_islandd::daemon_config::{
    approval_timeout, configure_detected_agents, env_locked, hook_timeout, idle_after, load_config,
    question_timeout, VERSION,
};
use open_islandd::notifications::{
    approval::QuestionSettlement,
    lifecycle::{self, DaemonContext, DaemonState, SharedState},
    shutdown::{BoundedThread, ShutdownFlag, SignalRegistrations},
};
use open_islandd::poll::{poller, refresh_update, send_message, update_poller, usage_poller};
use open_islandd::server::wire::{
    response, AnswerParams, CancelParams, JumpParams, PlayParams, ResolveParams, SendParams,
};
use open_islandd::sound::SoundPlayer;
use open_islandd::update;
use open_islandd::{claude_hook, codex_hook, installer, opencode_hook, usage};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    env, fs,
    io::{self, BufRead, BufReader, BufWriter, Read, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

const HELP: &str = "\
Uso: open-islandd [--socket <caminho>]
     open-islandd <comando> [opções]

Sem comando, sobe o daemon e fica escutando no socket.

Comandos:
  hook --agent claude|codex|opencode   trata um evento do agente lido da entrada padrão
  hooks install|uninstall|status       gerencia os hooks dos agentes [--agent <nome>] [--dry-run]
  hotkey install|uninstall|status      gerencia o atalho global [--combo <combinação>] [--dry-run]
  toggle                               abre ou fecha a ilha
  settings                             abre os ajustes da ilha
  autostart install|uninstall|status   gerencia o início automático do daemon [--dry-run]
  help, --help, -h                     mostra esta ajuda

Opções:
  --socket <caminho>   socket usado para falar com o daemon.
                       Padrão: $OPEN_ISLAND_SOCKET ou $XDG_RUNTIME_DIR/open-island.sock";

const STOP_DEADLINE: Duration = Duration::from_millis(500);

/// The accept loop polls instead of blocking so a shutdown can break it, and every new
/// connection waits out one full tick. `shutdown::TICK` is 250 ms, which the island never
/// notices because it connects once, but a per-keypress `toggle` pays it every time.
const ACCEPT_TICK: Duration = Duration::from_millis(20);

fn socket_path() -> PathBuf {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--socket" {
            if let Some(path) = args.next() {
                return PathBuf::from(path);
            }
        }
    }
    if let Some(path) = env::var_os("OPEN_ISLAND_SOCKET") {
        return PathBuf::from(path);
    }
    PathBuf::from(env::var_os("XDG_RUNTIME_DIR").unwrap_or_else(|| "/tmp".into()))
        .join("open-island.sock")
}

fn bind_socket(path: &Path) -> io::Result<UnixListener> {
    match UnixListener::bind(path) {
        Ok(listener) => {
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
            Ok(listener)
        }
        Err(bind_error) => match UnixStream::connect(path) {
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "another daemon is running",
            )),
            Err(_) => {
                let _ = fs::remove_file(path);
                UnixListener::bind(path)
                    .and_then(|listener| {
                        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
                        Ok(listener)
                    })
                    .map_err(|error| {
                        io::Error::new(
                            error.kind(),
                            format!("{bind_error}; rebinding failed: {error}"),
                        )
                    })
            }
        },
    }
}

fn handle(ctx: DaemonContext, connection_id: u64, request: Request) -> String {
    let id = request.id.clone();
    let broadcast = make_broadcast(Arc::clone(&ctx.state));
    let result: Result<Value, String> = match request.method.as_str() {
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
                open_islandd::sound::command(Path::new(&params.path), volume)
                    .spawn()
                    .map(|_| json!({"played": true}))
                    .map_err(|error| format!("{}: {error}", open_islandd::sound::PLAYER))
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

fn disconnect(ctx: &DaemonContext, connection_id: u64) {
    if let Ok(mut state) = ctx.state.lock() {
        state
            .subscribers
            .retain(|subscriber| subscriber.connection_id != connection_id);
    }
    lifecycle::disconnect_pending(ctx, connection_id, make_broadcast(Arc::clone(&ctx.state)));
}

fn client(stream: UnixStream, ctx: DaemonContext, connection_id: u64) {
    let (sender, receiver) = mpsc::channel::<String>();
    if let Ok(mut state) = ctx.state.lock() {
        state.subscribers.push(lifecycle::Subscriber {
            connection_id,
            sender: sender.clone(),
        });
    }
    let writer_ctx = ctx.clone();
    let writer_input = match stream.try_clone() {
        Ok(stream) => stream,
        Err(_) => return,
    };
    thread::spawn(move || {
        let mut writer = BufWriter::new(writer_input);
        for message in receiver {
            if writeln!(writer, "{message}")
                .and_then(|_| writer.flush())
                .is_err()
            {
                disconnect(&writer_ctx, connection_id);
                break;
            }
        }
    });
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        let read = match reader.read_line(&mut line) {
            Ok(read) => read,
            Err(_) => break,
        };
        if read == 0 {
            break;
        }
        let ctx_for_request = ctx.clone();
        let response_sender = sender.clone();
        match serde_json::from_str::<Request>(line.trim()) {
            Ok(request) => {
                thread::spawn(move || {
                    let _ = response_sender.send(handle(ctx_for_request, connection_id, request));
                });
            }
            Err(error) => {
                let _ = sender.send(response(
                    Value::Null,
                    Err(format!("invalid request: {error}")),
                ));
            }
        }
    }
    disconnect(&ctx, connection_id);
}

fn accept_loop(
    listener: &UnixListener,
    ctx: DaemonContext,
    flag: &ShutdownFlag,
    next_connection: &AtomicU64,
) {
    let _ = listener.set_nonblocking(true);
    loop {
        if flag.is_requested() {
            return;
        }
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(false);
                let connection_id = next_connection.fetch_add(1, Ordering::Relaxed);
                let ctx = ctx.clone();
                thread::spawn(move || client(stream, ctx, connection_id));
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(ACCEPT_TICK);
            }
            Err(error) => eprintln!("open-islandd accept error: {error}"),
        }
    }
}

fn run() -> io::Result<()> {
    let path = socket_path();
    let listener = bind_socket(&path)?;
    let mut loaded = load_config();
    configure_detected_agents(&mut loaded);
    let mut store = SessionStore::new();
    store.set_idle_after(idle_after(&loaded));
    store.set_cleanup_after(loaded.sessions.cleanup_after);
    store.set_filter_rules(
        loaded.filters.rules.clone(),
        loaded.filters.launchers.clone(),
    );
    let state: SharedState = Arc::new(Mutex::new(DaemonState {
        store,
        pending: HashMap::new(),
        pending_questions: HashMap::new(),
        subscribers: Vec::new(),
        approval_generation: 0,
        usage: usage::load_cached(),
        usage_watch: open_island_core::usage::ThresholdWatch::new(),
        scenes: QuietScenes::default(),
        update: None,
    }));
    let config = ConfigHandle::new(loaded);
    let (sound, mut sound_thread) = SoundPlayer::start(config.clone());
    let ctx = DaemonContext {
        state: Arc::clone(&state),
        config,
        sound,
    };
    let shutdown_flag = ShutdownFlag::new();
    let _signals = SignalRegistrations::register(&shutdown_flag)?;
    let poll_ctx = ctx.clone();
    let poll_flag = shutdown_flag.clone();
    let mut poller_thread = BoundedThread::spawn("open-island-poller", move || {
        poller(poll_ctx, poll_flag);
    })?;
    let usage_ctx = ctx.clone();
    let usage_flag = shutdown_flag.clone();
    let mut usage_thread = BoundedThread::spawn("open-island-usage", move || {
        usage_poller(usage_ctx, usage_flag);
    })?;
    let update_ctx = ctx.clone();
    let update_flag = shutdown_flag.clone();
    let mut update_thread = BoundedThread::spawn("open-island-update", move || {
        update_poller(update_ctx, update_flag);
    })?;
    let next_connection = AtomicU64::new(1);
    let sound_at_shutdown = ctx.sound.clone();
    accept_loop(&listener, ctx, &shutdown_flag, &next_connection);
    let _ = poller_thread.join_with_deadline(STOP_DEADLINE);
    let _ = usage_thread.join_with_deadline(STOP_DEADLINE);
    let _ = update_thread.join_with_deadline(STOP_DEADLINE);
    sound_at_shutdown.shutdown();
    if let Some(thread) = sound_thread.as_mut() {
        let _ = thread.join_with_deadline(STOP_DEADLINE);
    }
    Ok(())
}

fn run_hook() -> i32 {
    let socket = socket_path();
    let mut agent = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--agent" {
            agent = args.next();
            break;
        }
    }
    match agent.as_deref() {
        Some("claude") | Some("codex") | Some("opencode") => {}
        Some(other) => {
            eprintln!("open-islandd: unsupported agent '{other}'");
            return 2;
        }
        None => {
            eprintln!("open-islandd: hook requires --agent claude|codex|opencode");
            return 2;
        }
    }
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        eprintln!("open-islandd: failed to read hook input from stdin");
        return 1;
    }
    let result =
        match agent.as_deref() {
            Some("claude") => claude_hook::handle(&input, &socket, hook_timeout())
                .map_err(|error| error.to_string()),
            Some("codex") => {
                codex_hook::run(&input, &socket, hook_timeout()).map_err(|error| error.to_string())
            }
            Some("opencode") => opencode_hook::run(&input, &socket, hook_timeout())
                .map_err(|error| error.to_string()),
            Some(other) => Err(format!("unsupported agent '{other}'")),
            None => Err("hook requires --agent".to_owned()),
        };
    match result {
        Ok(output) => {
            if output.is_empty() {
                return 0;
            }
            let mut stdout = io::stdout().lock();
            if writeln!(stdout, "{output}").is_err() {
                return 1;
            }
            let _ = stdout.flush();
            0
        }
        Err(error) => {
            eprintln!("open-islandd: {error}");
            1
        }
    }
}

fn run_hotkey() -> i32 {
    let mut args = env::args().skip(2);
    let action = args.next();
    let mut combo = installer::DEFAULT_HOTKEY.to_owned();
    let mut dry_run = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--combo" => {
                let Some(value) = args.next() else {
                    eprintln!("open-islandd: --combo requires a value");
                    return 2;
                };
                combo = value;
            }
            "--dry-run" => dry_run = true,
            "--socket" => {
                let _ = args.next();
            }
            other => {
                eprintln!("open-islandd: unknown hotkey option '{other}'");
                return 2;
            }
        }
    }
    let install = match action.as_deref() {
        Some("install") => true,
        Some("uninstall") => false,
        Some("status") => return report_hotkey_status(&combo),
        Some(other) => {
            eprintln!("open-islandd: unknown hotkey action '{other}'");
            return 2;
        }
        None => {
            eprintln!("open-islandd: hotkey requires install, uninstall or status");
            return 2;
        }
    };
    let home = match installer::home_dir() {
        Ok(home) => home,
        Err(error) => {
            eprintln!("open-islandd: {error}");
            return 1;
        }
    };
    let executable = match env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("open-islandd: resolve executable: {error}");
            return 1;
        }
    };
    match installer::install_hotkey(&home, &executable, &combo, install, dry_run) {
        Ok(paths) => {
            for path in paths {
                println!(
                    "{} {}",
                    if dry_run { "would-change" } else { "changed" },
                    path.display()
                );
            }
            if !dry_run {
                println!("ran hyprctl reload -> {}", run_tool("hyprctl", &["reload"]));
            }
            if install {
                for note in installer::hotkey_notes(&combo) {
                    println!("note {note}");
                }
            }
            0
        }
        Err(error) => {
            eprintln!("open-islandd: {error}");
            1
        }
    }
}

fn run_tool(program: &str, args: &[&str]) -> String {
    match std::process::Command::new(program).args(args).output() {
        Ok(output) if output.status.success() => "ok".to_owned(),
        Ok(output) => String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        Err(error) => error.to_string(),
    }
}

/// The island holds one long-lived connection, so it never pays the accept tick; this
/// process is born and dies for a single keypress, which is why the reply matters more
/// than the exit code alone.
fn run_toggle() -> i32 {
    match ask_daemon("toggle", json!({"source": "hotkey"})) {
        Ok(data) if data["delivered"] == Value::Bool(true) => 0,
        Ok(_) => {
            eprintln!("open-islandd: no island is connected to the daemon");
            1
        }
        Err(error) => {
            eprintln!("open-islandd: {error}");
            1
        }
    }
}

fn ask_daemon(method: &str, params: Value) -> Result<Value, String> {
    let socket = socket_path();
    let mut stream = UnixStream::connect(&socket)
        .map_err(|error| format!("no daemon at {}: {error}", socket.display()))?;
    let request = json!({"v": 1, "id": 1, "method": method, "params": params});
    writeln!(stream, "{request}")
        .and_then(|_| stream.flush())
        .map_err(|error| format!("unable to send {method}: {error}"))?;
    let reader = stream
        .try_clone()
        .map(BufReader::new)
        .map_err(|error| format!("unable to read the {method} reply: {error}"))?;
    for line in reader.lines() {
        let Ok(line) = line else { break };
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if message.get("id").and_then(Value::as_i64) != Some(1) {
            continue;
        }
        if message["ok"] != Value::Bool(true) {
            return Err(format!(
                "{method} failed: {}",
                message["error"].as_str().unwrap_or("unknown error")
            ));
        }
        return Ok(message["data"].clone());
    }
    Err("daemon closed the connection before replying".to_owned())
}

const PACKAGED_DAEMON: &str = "/usr/bin/open-islandd";
const PACKAGED_ISLAND: &str = "/usr/bin/open-island";

fn island_candidates_beside(executable: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(directory) = executable.parent() {
        candidates.push(directory.join("open-island"));
    }
    candidates.push(PathBuf::from("open-island"));
    candidates
}

fn island_candidates() -> Vec<PathBuf> {
    match env::current_exe() {
        Ok(executable) => island_candidates_beside(&executable),
        Err(_) => vec![PathBuf::from("open-island")],
    }
}

fn run_settings() -> i32 {
    match ask_daemon("settings", json!({"source": "cli"})) {
        Ok(data) if data["delivered"] == Value::Bool(true) => return 0,
        Ok(_) => {}
        Err(error) => eprintln!("open-islandd: {error}"),
    }
    for candidate in island_candidates() {
        if std::process::Command::new(&candidate)
            .arg("--settings")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .is_ok()
        {
            return 0;
        }
    }
    eprintln!("open-islandd: no island is running and none could be started");
    1
}

fn run_autostart() -> i32 {
    let mut args = env::args().skip(2);
    let action = args.next();
    let mut dry_run = false;
    for arg in args {
        match arg.as_str() {
            "--dry-run" => dry_run = true,
            other => {
                eprintln!("open-islandd: unknown autostart option '{other}'");
                return 2;
            }
        }
    }
    let install = match action.as_deref() {
        Some("install") => true,
        Some("uninstall") => false,
        Some("status") => return report_autostart_status(),
        Some(other) => {
            eprintln!("open-islandd: unknown autostart action '{other}'");
            return 2;
        }
        None => {
            eprintln!("open-islandd: autostart requires install, uninstall or status");
            return 2;
        }
    };
    let Some((home, daemon, island)) = autostart_paths() else {
        return 1;
    };
    if !install && !dry_run {
        run_systemd_steps(false);
    }
    match installer::install_autostart(&home, &daemon, &island, install, dry_run) {
        Ok(paths) => {
            for path in paths {
                println!(
                    "{} {}",
                    if dry_run { "would-change" } else { "changed" },
                    path.display()
                );
            }
            if !dry_run {
                refresh_icon_cache(&home);
            }
            if install && !dry_run {
                run_systemd_steps(true);
            }
            for note in installer::autostart_notes(install) {
                println!("note {note}");
            }
            0
        }
        Err(error) => {
            eprintln!("open-islandd: {error}");
            1
        }
    }
}

// The three files existing is not the same as systemd being told to start them: the units
// were present and disabled after a reboot, and the pane still reported Ativo.
fn units_enabled() -> bool {
    ["open-islandd.service", "open-island.service"]
        .iter()
        .all(|unit| {
            std::process::Command::new("systemctl")
                .args(["--user", "is-enabled", unit])
                .output()
                .is_ok_and(|output| output.status.success())
        })
}

/// GTK trusts `icon-theme.cache` over the directory it sits in; without this the icon is invisible.
fn refresh_icon_cache(home: &Path) {
    let theme = home.join(".local/share/icons/hicolor");
    let Some(directory) = theme.to_str() else {
        return;
    };
    println!(
        "ran gtk-update-icon-cache {directory} -> {}",
        run_tool(
            "gtk-update-icon-cache",
            &["-f", "-t", "--ignore-theme-index", directory],
        )
    );
}

fn run_systemd_steps(install: bool) {
    for (program, arguments) in systemd_steps(install) {
        let rendered = arguments.join(" ");
        println!(
            "ran {program} {rendered} -> {}",
            run_tool(program, &arguments)
        );
    }
}

fn systemd_steps(install: bool) -> Vec<(&'static str, Vec<&'static str>)> {
    let units = ["open-islandd.service", "open-island.service"];
    let toggle = if install { "enable" } else { "disable" };
    let mut arguments = vec!["--user", toggle, "--now"];
    arguments.extend(units);
    if install {
        return vec![
            ("systemctl", vec!["--user", "daemon-reload"]),
            ("systemctl", arguments),
        ];
    }
    vec![
        ("systemctl", arguments),
        ("systemctl", vec!["--user", "daemon-reload"]),
    ]
}

fn autostart_paths() -> Option<(PathBuf, PathBuf, PathBuf)> {
    let home = match installer::home_dir() {
        Ok(home) => home,
        Err(error) => {
            eprintln!("open-islandd: {error}");
            return None;
        }
    };
    let daemon = match env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("open-islandd: resolve executable: {error}");
            return None;
        }
    };
    let daemon = fs::canonicalize(&daemon).unwrap_or(daemon);
    let (daemon, island) = autostart_executables(daemon);
    Some((home, daemon, island))
}

fn autostart_executables(daemon: PathBuf) -> (PathBuf, PathBuf) {
    if installer::is_packaged_executable(&daemon) {
        return (
            PathBuf::from(PACKAGED_DAEMON),
            PathBuf::from(PACKAGED_ISLAND),
        );
    }
    let island = island_candidates_beside(&daemon)
        .into_iter()
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| PathBuf::from("open-island"));
    (daemon, island)
}

fn report_hotkey_status(combo: &str) -> i32 {
    let home = match installer::home_dir() {
        Ok(home) => home,
        Err(error) => {
            eprintln!("open-islandd: {error}");
            return 1;
        }
    };
    let executable = match env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("open-islandd: resolve executable: {error}");
            return 1;
        }
    };
    match installer::install_hotkey(&home, &executable, combo, true, true) {
        Ok(pending) => {
            println!("{}", json!({"installed": pending.is_empty()}));
            0
        }
        Err(error) => {
            eprintln!("open-islandd: {error}");
            1
        }
    }
}

fn report_autostart_status() -> i32 {
    let Some((home, daemon, island)) = autostart_paths() else {
        return 1;
    };
    match installer::install_autostart(&home, &daemon, &island, true, true) {
        Ok(pending) => {
            println!(
                "{}",
                json!({"installed": pending.is_empty() && units_enabled()})
            );
            0
        }
        Err(error) => {
            eprintln!("open-islandd: {error}");
            1
        }
    }
}

fn report_hooks_status(home: &Path, agents: &[&str], executable: &Path) -> i32 {
    let mut report = serde_json::Map::new();
    for agent in agents {
        let detected = installer::detected(home, agent);
        let installed = detected && installer::installed(home, agent, executable).unwrap_or(false);
        report.insert(
            (*agent).to_owned(),
            json!({"detected": detected, "installed": installed}),
        );
    }
    println!("{}", Value::Object(report));
    0
}

fn run_hooks() -> i32 {
    let mut args = env::args().skip(2);
    let action = args.next();
    let mut agent = "all".to_owned();
    let mut dry_run = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--agent" => {
                let Some(value) = args.next() else {
                    eprintln!("open-islandd: --agent requires a value");
                    return 2;
                };
                agent = value;
            }
            "--dry-run" => dry_run = true,
            "--socket" => {
                let _ = args.next();
            }
            other => {
                eprintln!("open-islandd: unknown hooks option '{other}'");
                return 2;
            }
        }
    }
    let agents = match installer::selected_agents(&agent) {
        Ok(agents) => agents,
        Err(error) => {
            eprintln!("open-islandd: {error}");
            return 2;
        }
    };
    let home = match installer::home_dir() {
        Ok(home) => home,
        Err(error) => {
            eprintln!("open-islandd: {error}");
            return 1;
        }
    };
    let executable = match env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("open-islandd: resolve executable: {error}");
            return 1;
        }
    };
    if action.as_deref() == Some("status") {
        return report_hooks_status(&home, &agents, &executable);
    }
    let result = match action.as_deref() {
        Some("install") => installer::install(&home, &agents, &executable, dry_run),
        Some("uninstall") => installer::uninstall(&home, &agents, &executable, dry_run),
        Some(other) => Err(format!("unknown hooks action '{other}'")),
        None => Err("hooks requires install, uninstall or status".to_owned()),
    };
    match result {
        Ok(paths) => {
            for path in paths {
                println!(
                    "{} {}",
                    if dry_run { "would-change" } else { "changed" },
                    path.display()
                );
            }
            if action.as_deref() == Some("install") {
                for note in installer::install_notes(&agents) {
                    println!("note {note}");
                }
            }
            0
        }
        Err(error) => {
            eprintln!("open-islandd: {error}");
            1
        }
    }
}

fn main() {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("hook") => {
            let code = run_hook();
            if code != 0 {
                std::process::exit(code);
            }
        }
        Some("hooks") => {
            let code = run_hooks();
            if code != 0 {
                std::process::exit(code);
            }
        }
        Some("hotkey") => {
            let code = run_hotkey();
            if code != 0 {
                std::process::exit(code);
            }
        }
        Some("toggle") => {
            let code = run_toggle();
            if code != 0 {
                std::process::exit(code);
            }
        }
        Some("settings") => {
            let code = run_settings();
            if code != 0 {
                std::process::exit(code);
            }
        }
        Some("autostart") => {
            let code = run_autostart();
            if code != 0 {
                std::process::exit(code);
            }
        }
        Some("help") | Some("--help") | Some("-h") => {
            println!("{HELP}");
        }
        _ => {
            if let Err(error) = run() {
                if error.kind() != io::ErrorKind::AlreadyExists {
                    eprintln!("open-islandd: {error}");
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "main_autostart_tests.rs"]
mod autostart_tests;
