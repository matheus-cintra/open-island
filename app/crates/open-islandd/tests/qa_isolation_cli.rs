use open_island_core::process::{
    command, cwd, environment, exists, install_process_source_for_qa, parent_and_comm, pids,
    register_process_for_qa, stdin_device, ProcessBirthIdentity, ProcessSource,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

#[derive(Default)]
struct RecordingProcessSource {
    calls: AtomicUsize,
}

impl ProcessSource for RecordingProcessSource {
    fn pids(&self) -> Vec<(u32, ProcessBirthIdentity)> {
        Vec::new()
    }

    fn birth_identity(&self, _pid: u32) -> Option<ProcessBirthIdentity> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        None
    }

    fn parent_and_comm(&self, _pid: u32) -> Option<(u32, String)> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        None
    }

    fn command(&self, _pid: u32) -> Option<Vec<u8>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        None
    }

    fn environment(&self, _pid: u32) -> Vec<u8> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        b"QA_SENTINEL=leaked\0".to_vec()
    }

    fn cwd(&self, _pid: u32) -> Option<PathBuf> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        None
    }

    fn stdin_device(&self, _pid: u32) -> Option<u64> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        None
    }

    fn exists(&self, _pid: u32) -> bool {
        self.calls.fetch_add(1, Ordering::SeqCst);
        false
    }
}

#[test]
fn isolation_negative_rejects_unregistered_pid_before_process_source_call() {
    assert!(pids().is_empty());
    assert!(environment(std::process::id()).is_empty());
    let source = Arc::new(RecordingProcessSource::default());
    let _guard = install_process_source_for_qa(source.clone()).expect("install QA process source");
    register_process_for_qa(std::process::id(), ProcessBirthIdentity::new(1))
        .expect("register owned helper identity");
    let sentinel_pid = u32::try_from(unsafe { libc::getppid() }).unwrap_or(u32::MAX);

    let environment = environment(sentinel_pid);

    assert!(pids().is_empty());
    assert!(environment.is_empty());
    assert!(command(sentinel_pid).is_none());
    assert!(cwd(sentinel_pid).is_none());
    assert!(parent_and_comm(sentinel_pid).is_none());
    assert!(stdin_device(sentinel_pid).is_none());
    assert!(!exists(sentinel_pid));
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);
    assert!(open_island_core::process::environment(std::process::id()).is_empty());
    assert_eq!(source.calls.load(Ordering::SeqCst), 2);
}
