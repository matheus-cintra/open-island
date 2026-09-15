fn main() {
    println!("cargo:rerun-if-changed=src/platform/macos.m");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new()
            .file("src/platform/macos.m")
            .flag("-fobjc-arc")
            .flag("-mmacosx-version-min=12.0")
            .compile("oi_panel");
        println!("cargo:rustc-link-lib=framework=AppKit");
        println!("cargo:rustc-link-lib=framework=CoreGraphics");
        println!("cargo:rustc-link-lib=framework=Intents");
        println!("cargo:rustc-link-lib=framework=AVFoundation");
    }
    tauri_build::try_build(tauri_build::Attributes::new().plugins([(
        "voice",
        tauri_build::InlinedPlugin::new().commands(&[
            "voice_start",
            "voice_get_state",
            "voice_state",
            "voice_stop",
            "voice_cancel",
            "voice_model_status",
            "voice_select_model",
            "voice_clear_model",
            "voice_open_microphone_settings",
        ]),
    )]))
    .expect("build Tauri voice permissions");
}
