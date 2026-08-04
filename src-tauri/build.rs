fn main() {
    println!("cargo:rerun-if-env-changed=BACKEND_VISUAL_MAP_BUILD_SCOPE");
    println!("cargo:rerun-if-env-changed=BACKEND_VISUAL_MAP_SKIP_PROVIDER_RESOURCES");
    println!("cargo:rustc-check-cfg=cfg(backend_visual_map_internal_build)");
    if std::env::var("BACKEND_VISUAL_MAP_BUILD_SCOPE").as_deref() == Ok("internal") {
        println!("cargo:rustc-cfg=backend_visual_map_internal_build");
    }
    // Provider files are bundled for the desktop build, but debug/lint/test
    // builds do not need to copy or watch 68k files. Dev code resolves the
    // source provider directory directly; release builds keep the full bundle.
    let skip_provider_resources = std::env::var("PROFILE").as_deref() == Ok("debug")
        || std::env::var("BACKEND_VISUAL_MAP_SKIP_PROVIDER_RESOURCES").as_deref() == Ok("1");
    if skip_provider_resources {
        std::env::set_var(
            "TAURI_CONFIG",
            r#"{"bundle":{"resources":{"../THIRD_PARTY_NOTICES.md":"THIRD_PARTY_NOTICES.md","engines/manifest.json":"engines/manifest.json","engines/code-memory-language.exe":"engines/code-memory-language.exe","../code_memory/packs":"engines/packs","engines/database-memory.exe":"engines/database-memory.exe"}}}"#,
        );
    }
    tauri_build::build()
}
