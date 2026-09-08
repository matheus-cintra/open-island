use super::*;

#[test]
fn base64_matches_the_rfc_test_vectors() {
    assert_eq!(base64(b""), "");
    assert_eq!(base64(b"f"), "Zg==");
    assert_eq!(base64(b"fo"), "Zm8=");
    assert_eq!(base64(b"foo"), "Zm9v");
    assert_eq!(base64(b"foob"), "Zm9vYg==");
    assert_eq!(base64(b"fooba"), "Zm9vYmE=");
    assert_eq!(base64(b"foobar"), "Zm9vYmFy");
}

#[test]
fn base64_survives_every_byte_value() {
    let bytes: Vec<u8> = (0..=255u8).collect();
    let encoded = base64(&bytes);
    assert_eq!(encoded.len(), 344);
    assert!(encoded.starts_with("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8"));
    assert!(encoded.ends_with("9vf4+fr7/P3+/w=="));
}

#[test]
fn the_current_theme_outscores_hicolor_and_a_bigger_size_outscores_a_smaller_one() {
    let themed = Path::new("/usr/share/icons/Papirus/48x48/apps/kitty.png");
    let hicolor = Path::new("/usr/share/icons/hicolor/48x48/apps/kitty.png");
    let small = Path::new("/usr/share/icons/hicolor/16x16/apps/kitty.png");
    assert!(score(themed, "Papirus") > score(hicolor, "Papirus"));
    assert!(score(hicolor, "Papirus") > score(small, "Papirus"));
}

#[test]
fn scalable_beats_every_raster_size_inside_the_same_theme() {
    let scalable = Path::new("/usr/share/icons/hicolor/scalable/apps/kitty.svg");
    let raster = Path::new("/usr/share/icons/hicolor/256x256/apps/kitty.png");
    assert!(score(scalable, "") > score(raster, ""));
}

#[test]
fn a_desktop_key_is_read_and_a_missing_one_is_none() {
    let directory =
        std::env::temp_dir().join(format!("open-island-appicon-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("temp dir");
    let entry = directory.join("probe.desktop");
    std::fs::write(
        &entry,
        "[Desktop Entry]\nName=Probe\nIcon=probe-icon\nStartupWMClass=Probe\n",
    )
    .expect("write entry");
    assert_eq!(icon_name(&entry).as_deref(), Some("probe-icon"));
    assert_eq!(
        reads_key(&entry, "StartupWMClass").as_deref(),
        Some("Probe")
    );
    assert_eq!(reads_key(&entry, "NoSuchKey"), None);
    let _ = std::fs::remove_dir_all(&directory);
}
