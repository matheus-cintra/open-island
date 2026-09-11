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
    }
    tauri_build::build()
}
