#[cfg(test)]
mod tests {
    use super::connection::initialization_options;
    use super::{
        bundled_java_home, java_home_is_usable, java_language_server_settings,
        default_lsp_request_timeout, large_workspace_workload, lsp_message_length_allowed,
        lsp_reference_enrichment_enabled, rust_analyzer_settings, symbol_string,
        uri_to_relative_path, MAX_LSP_MESSAGE_BYTES,
    };
    use std::fs;
    use std::path::Path;

    #[test]
    fn lsp_message_size_is_bounded() {
        assert!(lsp_message_length_allowed(0));
        assert!(lsp_message_length_allowed(MAX_LSP_MESSAGE_BYTES));
        assert!(!lsp_message_length_allowed(MAX_LSP_MESSAGE_BYTES + 1));
    }

    #[test]
    fn cold_provider_start_has_a_reliable_default_response_window() {
        assert_eq!(default_lsp_request_timeout().as_secs(), 60);
    }

    #[test]
    fn lsp_symbol_identity_ignores_call_hierarchy_return_type_suffix() {
        assert_eq!(
            symbol_string("src/Client.java", "getOwner(int) : Mono<Owner>", 3, 8),
            symbol_string("src/Client.java", "getOwner(int)", 3, 8)
        );
        assert_eq!(
            symbol_string("src/Client.java", "getOwner(int)", 3, 8),
            "lsp . . . src.Client.java#getOwner(int)@3:8"
        );
    }

    #[test]
    fn rust_reference_enrichment_is_enabled_by_default() {
        assert!(lsp_reference_enrichment_enabled("rust"));
        assert!(lsp_reference_enrichment_enabled("ruby"));
        assert!(!lsp_reference_enrichment_enabled("typescript"));
    }

    #[test]
    fn semantic_query_volume_promotes_few_large_files_to_large_workspace_mode() {
        assert!(!large_workspace_workload("rust-analyzer", 62, 500));
        assert!(large_workspace_workload("rust-analyzer", 62, 501));
        assert!(large_workspace_workload("gopls", 251, 0));
    }

    #[cfg(windows)]
    #[test]
    fn windows_file_uri_drive_case_is_relative_to_the_workspace() {
        assert_eq!(
            uri_to_relative_path("file:///d:/Project/src/App.java", Path::new(r"D:\Project")),
            "src/App.java"
        );
    }

    #[test]
    fn bundled_java_home_supports_a_jdtls_bin_launcher() {
        let root =
            std::env::temp_dir().join(format!("code-memory-jdtls-runtime-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let launcher = root.join("jdtls").join("bin").join("jdtls.cmd");
        let runtime_bin = root.join("jdtls").join("runtime").join("bin");
        fs::create_dir_all(launcher.parent().expect("launcher parent")).expect("create launcher");
        fs::create_dir_all(&runtime_bin).expect("create runtime");
        fs::write(&launcher, b"launcher").expect("write launcher");
        assert_eq!(
            bundled_java_home(&launcher),
            Some(root.join("jdtls/runtime"))
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn java_source_only_settings_disable_build_importers() {
        let settings = java_language_server_settings(true);
        assert_eq!(
            settings.pointer("/java/import/gradle/enabled"),
            Some(&serde_json::Value::Bool(false))
        );
        assert_eq!(
            settings.pointer("/java/import/maven/enabled"),
            Some(&serde_json::Value::Bool(false))
        );
        assert_eq!(
            settings.pointer("/java/project/importOnFirstTimeStartup"),
            Some(&serde_json::Value::String("disabled".to_string()))
        );
    }

    #[test]
    fn rust_restart_only_settings_are_sent_during_initialize() {
        let options = initialization_options("rust", &rust_analyzer_settings());
        assert_eq!(
            options.pointer("/cargo/sysroot"),
            Some(&serde_json::Value::Null)
        );
        assert_eq!(
            options.pointer("/checkOnSave/enable"),
            Some(&serde_json::Value::Bool(false))
        );
    }

    #[test]
    fn java_home_requires_a_real_launcher() {
        let root =
            std::env::temp_dir().join(format!("code-memory-java-home-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        assert!(!java_home_is_usable(&root));
        let bin = root.join("bin");
        fs::create_dir_all(&bin).expect("create java bin");
        let executable = if cfg!(windows) { "java.exe" } else { "java" };
        fs::write(bin.join(executable), b"launcher").expect("write java launcher");
        assert!(java_home_is_usable(&root));
        let _ = fs::remove_dir_all(root);
    }
}
