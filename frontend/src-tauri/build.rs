use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    bundle_engine_binary();
    tauri_build::build();
}

fn bundle_engine_binary() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let workspace = manifest_dir.join("../..");
    let profile = env::var("PROFILE").expect("PROFILE");
    let engine_file = if cfg!(windows) {
        "code-analysis-engine.exe"
    } else {
        "code-analysis-engine"
    };
    let engine_src = workspace.join("target").join(&profile).join(engine_file);

    println!("cargo:rerun-if-changed={}", workspace.join("src").display());
    println!("cargo:rerun-if-changed={}", workspace.join("Cargo.toml").display());
    println!("cargo:rerun-if-changed={}", engine_src.display());

    ensure_engine_built(&workspace, &profile);

    // dev 빌드는 워크스페이스 target/{profile} 바이너리를 직접 사용한다.
    // release 번들에만 binaries/로 복사한다.
    if profile == "debug" {
        return;
    }

    let binaries_dir = manifest_dir.join("binaries");
    fs::create_dir_all(&binaries_dir).expect("binaries 디렉터리를 만들어야 한다");
    let resource_dest = binaries_dir.join(engine_file);
    copy_engine_if_needed(&engine_src, &resource_dest);
}

fn ensure_engine_built(workspace: &Path, profile: &str) {
    let engine_file = if cfg!(windows) {
        "code-analysis-engine.exe"
    } else {
        "code-analysis-engine"
    };
    let engine_src = workspace.join("target").join(profile).join(engine_file);
    if profile != "debug" && engine_src.is_file() {
        return;
    }

    let mut command = Command::new("cargo");
    command
        .current_dir(workspace)
        .args(["build", "--bin", "code-analysis-engine"]);
    if profile != "debug" {
        command.arg("--profile").arg(profile);
    }
    let status = command
        .status()
        .expect("엔진 바이너리를 빌드해야 한다");
    if !status.success() {
        panic!("code-analysis-engine 빌드에 실패했습니다");
    }
}

fn copy_engine_if_needed(src: &Path, dest: &Path) {
    if should_skip_copy(src, dest) {
        return;
    }

    match fs::copy(src, dest) {
        Ok(_) => {}
        Err(error) if dest.is_file() && is_locked_copy_error(&error) => {
            println!(
                "cargo:warning=엔진 바이너리가 사용 중이라 복사를 건너뜁니다: {}",
                dest.display()
            );
        }
        Err(error) => {
            panic!("엔진 바이너리를 복사해야 한다 ({} -> {}): {error}", src.display(), dest.display());
        }
    }
}

fn should_skip_copy(src: &Path, dest: &Path) -> bool {
    if !dest.is_file() {
        return false;
    }
    let Ok(src_meta) = fs::metadata(src) else {
        return false;
    };
    let Ok(dest_meta) = fs::metadata(dest) else {
        return false;
    };
    if src_meta.len() != dest_meta.len() {
        return false;
    }
    match (src_meta.modified(), dest_meta.modified()) {
        (Ok(src_modified), Ok(dest_modified)) => src_modified <= dest_modified,
        _ => false,
    }
}

fn is_locked_copy_error(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(1224) | Some(32) // ERROR_USER_MAPPED_FILE | ERROR_SHARING_VIOLATION
    ) || error.kind() == io::ErrorKind::PermissionDenied
}
