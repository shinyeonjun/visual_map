#[cfg(test)]
mod clone_cleanup_tests {
    use super::*;

    #[test]
    fn failed_clone_does_not_leave_a_managed_project_directory() {
        let root = std::env::temp_dir().join(format!(
            "backend-visual-map-clone-failure-{}-{}",
            std::process::id(),
            timestamp()
        ));
        fs::create_dir_all(&root).unwrap();
        let target = root.join("partial-clone");
        let missing_source = root.join("missing.git");

        assert!(clone_github_repo(missing_source.to_str().unwrap(), &target).is_err());
        assert!(!target.exists());

        fs::remove_dir_all(root).unwrap();
    }
}

