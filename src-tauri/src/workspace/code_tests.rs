#[cfg(test)]
mod tests {
    use super::architecture_diagnostics;

    #[test]
    fn test_file_name_conventions_are_treated_as_non_product_code() {
        for path in [
            "src/auth.test.ts",
            "src/auth.spec.ts",
            "pkg/orders_test.go",
            "tests/test_auth.py",
            "src/UserServiceTest.java",
            "src/widget_spec.rb",
        ] {
            assert!(super::is_test_file_path(path), "{path}");
        }
        for path in [
            "src/TestController.java",
            "src/Contest.java",
            "src/testClient.go",
        ] {
            assert!(!super::is_test_file_path(path), "{path}");
        }
    }

    #[test]
    fn provider_diagnostics_become_visible_inventory_gaps() {
        let gaps = architecture_diagnostics(
            Some(&serde_json::json!({
                "diagnostics": [{
                    "language": "java",
                    "status": "missing-tool",
                    "message": "jdtls is not available"
                }]
            })),
            "shop",
        );

        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].kind, "unknown");
        assert_eq!(gaps[0].from, "provider:java");
        assert_eq!(gaps[0].to, "shop");
        assert_eq!(gaps[0].message, "jdtls is not available");
    }

    #[test]
    fn compiler_warnings_do_not_mark_provider_coverage_partial() {
        let gaps = architecture_diagnostics(
            Some(&serde_json::json!({
                "diagnostics": [{
                    "kind": "provider",
                    "path": "src/Owner.java",
                    "message": "java:40: Temporal is deprecated"
                }]
            })),
            "shop",
        );

        assert!(gaps.is_empty());
    }

    #[test]
    fn structured_provider_code_is_independent_of_message_wording() {
        let complete = architecture_diagnostics(
            Some(&serde_json::json!({
                "diagnostics": [{
                    "language": "java",
                    "level": "warning",
                    "code": "provider-diagnostic",
                    "message": "native provider wording changed"
                }]
            })),
            "shop",
        );
        assert!(complete.is_empty());

        let missing = architecture_diagnostics(
            Some(&serde_json::json!({
                "diagnostics": [{
                    "language": "java",
                    "level": "warning",
                    "code": "provider-missing",
                    "message": "any human wording"
                }]
            })),
            "shop",
        );
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].kind, "provider-missing");
    }
}
