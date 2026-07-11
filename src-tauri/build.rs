fn main() {
    println!("cargo:rerun-if-env-changed=BACKEND_VISUAL_MAP_BUILD_SCOPE");
    println!("cargo:rustc-check-cfg=cfg(backend_visual_map_internal_build)");
    if std::env::var("BACKEND_VISUAL_MAP_BUILD_SCOPE").as_deref() == Ok("internal") {
        println!("cargo:rustc-cfg=backend_visual_map_internal_build");
    }
    tauri_build::build()
}
