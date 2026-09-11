use open_island_core::{protocol::QuietScenes, store::SessionStore};
use open_islandd::cli::{
    autostart::run_autostart,
    client::{run_settings, run_toggle},
    hook::run_hook,
    hooks::run_hooks,
    hotkey::run_hotkey,
};
use open_islandd::config_handle::ConfigHandle;
use open_islandd::daemon_config::{configure_detected_agents, idle_after, load_config};
use open_islandd::notifications::{
    lifecycle::{DaemonContext, DaemonState, SharedState},
    shutdown::{BoundedThread, ShutdownFlag, SignalRegistrations},
};
use open_islandd::poll::{poller, update_poller, usage_poller};
use open_islandd::server::{accept_loop, bind_socket, socket_path};
use open_islandd::sound::SoundPlayer;
use open_islandd::usage;
use std::{
    collections::HashMap,
    env, io,
    sync::{atomic::AtomicU64, Arc, Mutex},
    time::Duration,
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
        #[cfg(target_os = "macos")]
        Some("stop") => {
            std::process::exit(open_islandd::cli::client::run_stop());
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
