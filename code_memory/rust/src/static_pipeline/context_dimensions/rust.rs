use super::{literal_assignment_in_section, push_dimension};
use codebase_fact_model::analysis::{ContextDimension, ContextDimensionKind};
use codebase_fact_model::source::RepositoryPath;
use std::env;

pub(super) fn collect_config(
    path: &RepositoryPath,
    text: &str,
    output: &mut Vec<ContextDimension>,
) {
    let name = path
        .as_str()
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if name == "cargo.toml" {
        for section in ["package", "workspace.package"] {
            if let Some(edition) = literal_assignment_in_section(text, section, "edition") {
                push_dimension(
                    output,
                    ContextDimensionKind::LanguageVersion,
                    format!("edition-{edition}"),
                );
            }
        }
    } else if path.as_str().to_ascii_lowercase().contains("/.cargo/")
        || path.as_str().to_ascii_lowercase().starts_with(".cargo/")
    {
        if let Some(target) = literal_assignment_in_section(text, "build", "target") {
            push_dimension(output, ContextDimensionKind::Target, target);
        }
    }
}

pub(super) fn complete_defaults(output: &mut Vec<ContextDimension>) {
    if let Ok(target) = env::var("CARGO_BUILD_TARGET") {
        let target = target.trim();
        if !target.is_empty() {
            output.retain(|item| item.kind != ContextDimensionKind::Target);
            push_dimension(output, ContextDimensionKind::Target, target);
        }
    }
    if !output
        .iter()
        .any(|item| item.kind == ContextDimensionKind::Target)
    {
        // rust-analyzer is configured without an explicit cargo.target. In
        // that state it uses the host target pinned by the provider bundle.
        push_dimension(output, ContextDimensionKind::Target, "host");
    }
    // The runner does not set cargo.features/allFeatures; rust-analyzer uses
    // Cargo's default feature selection, whose exact membership is sealed by
    // the Cargo.toml digests.
    push_dimension(output, ContextDimensionKind::Feature, "default");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_edition_and_default_feature_mode() {
        let mut output = Vec::new();
        collect_config(
            &RepositoryPath::parse("Cargo.toml").unwrap(),
            "[package]\nedition='2021'\n",
            &mut output,
        );
        complete_defaults(&mut output);
        assert!(output.iter().any(|item| item.value == "edition-2021"));
        assert!(output.iter().any(|item| item.value == "default"));
        assert!(output
            .iter()
            .any(|item| item.kind == ContextDimensionKind::Target));
    }
}
