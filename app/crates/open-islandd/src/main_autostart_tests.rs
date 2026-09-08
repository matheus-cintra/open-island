use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

fn scratch() -> PathBuf {
    let root = env::temp_dir().join(format!(
        "open-island-autostart-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("scratch");
    root
}

#[test]
fn a_daemon_in_usr_bin_resolves_both_units_to_the_packaged_paths() {
    let (daemon, island) = autostart_executables(PathBuf::from("/usr/bin/open-islandd"));
    assert_eq!(daemon, Path::new("/usr/bin/open-islandd"));
    assert_eq!(island, Path::new("/usr/bin/open-island"));
}

#[test]
fn a_directory_that_merely_starts_like_usr_bin_is_not_the_packaged_layout() {
    let (daemon, island) = autostart_executables(PathBuf::from("/usr/bin-something/open-islandd"));
    assert_eq!(daemon, Path::new("/usr/bin-something/open-islandd"));
    assert_ne!(island, Path::new("/usr/bin/open-island"));
    assert_eq!(island, Path::new("open-island"));
}

#[test]
fn neither_a_deeper_nor_a_local_bin_directory_is_the_packaged_layout() {
    for path in [
        "/usr/bin/nested/open-islandd",
        "/usr/local/bin/open-islandd",
        "/usr/binary/open-islandd",
    ] {
        let (daemon, island) = autostart_executables(PathBuf::from(path));
        assert_eq!(daemon, Path::new(path));
        assert_ne!(island, Path::new("/usr/bin/open-island"));
    }
}

#[test]
fn a_build_tree_daemon_keeps_its_own_directory_and_its_sibling_island() {
    let root = scratch();
    let daemon = root.join("open-islandd");
    fs::write(&daemon, "").expect("daemon");
    fs::write(root.join("open-island"), "").expect("island");
    let (resolved, island) = autostart_executables(daemon.clone());
    assert_eq!(resolved, daemon);
    assert_eq!(island, root.join("open-island"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_build_tree_daemon_without_a_sibling_island_falls_back_to_the_bare_name() {
    let root = scratch();
    let daemon = root.join("open-islandd");
    fs::write(&daemon, "").expect("daemon");
    let (_, island) = autostart_executables(daemon);
    assert_eq!(island, Path::new("open-island"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn island_candidates_look_beside_the_daemon_before_the_bare_name() {
    assert_eq!(
        island_candidates_beside(Path::new("/opt/open-island/bin/open-islandd")),
        vec![
            PathBuf::from("/opt/open-island/bin/open-island"),
            PathBuf::from("open-island"),
        ]
    );
    assert_eq!(
        island_candidates_beside(Path::new("open-islandd")),
        vec![PathBuf::from("open-island"), PathBuf::from("open-island")]
    );
}
