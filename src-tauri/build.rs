fn main() {
    println!("cargo:rerun-if-env-changed=CODEBASE_WORKSPACE_BUILD_SCOPE");
    println!("cargo:rerun-if-env-changed=CODEBASE_WORKSPACE_SKIP_PROVIDER_RESOURCES");
    println!("cargo:rerun-if-env-changed=CODEBASE_WORKSPACE_PROVIDER_CATALOG_PUBLIC_KEY");
    println!("cargo:rustc-check-cfg=cfg(codebase_workspace_internal_build)");
    let internal = std::env::var("CODEBASE_WORKSPACE_BUILD_SCOPE").as_deref() == Ok("internal");
    if internal {
        println!("cargo:rustc-cfg=codebase_workspace_internal_build");
    }
    let provider_catalog_public_key =
        std::env::var("CODEBASE_WORKSPACE_PROVIDER_CATALOG_PUBLIC_KEY")
            .unwrap_or_else(|_| "IVL40Zt5HSRFMkLhXy6rbLfP+ntqXtMAl5YOBpiB2xI=".to_string());
    println!(
        "cargo:rustc-env=CODEBASE_WORKSPACE_PROVIDER_CATALOG_PUBLIC_KEY={provider_catalog_public_key}"
    );
    if std::env::var("PROFILE").as_deref() == Ok("release")
        && !internal
        && std::env::var("CODEBASE_WORKSPACE_PROVIDER_CATALOG_PUBLIC_KEY")
            .map(|value| value.trim().is_empty())
            .unwrap_or(true)
    {
        panic!("release builds require CODEBASE_WORKSPACE_PROVIDER_CATALOG_PUBLIC_KEY");
    }
    // Provider files are bundled for the desktop build, but debug/lint/test
    // builds do not need to copy or watch 68k files. Dev code resolves the
    // source provider directory directly; release builds keep the full bundle.
    let skip_provider_resources = std::env::var("PROFILE").as_deref() == Ok("debug")
        || std::env::var("CODEBASE_WORKSPACE_SKIP_PROVIDER_RESOURCES").as_deref() == Ok("1");
    if skip_provider_resources {
        std::env::set_var(
            "TAURI_CONFIG",
            r#"{"bundle":{"resources":{"../THIRD_PARTY_NOTICES.md":"THIRD_PARTY_NOTICES.md","engines/manifest.json":"engines/manifest.json","engines/code-memory-language.exe":"engines/code-memory-language.exe","../code_memory/packs":"engines/packs","engines/database-memory.exe":"engines/database-memory.exe"}}}"#,
        );
    }
    tauri_build::build()
}
