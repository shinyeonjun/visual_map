use super::{push_dimension, unquote};
use codebase_fact_model::analysis::{ContextDimension, ContextDimensionKind};

pub(super) fn collect(name: &str, text: &str, output: &mut Vec<ContextDimension>) {
    if name != "pubspec.yaml" {
        return;
    }
    let mut environment_indent = None;
    for raw_line in text.lines() {
        let without_comment = raw_line.split('#').next().unwrap_or_default();
        let trimmed = without_comment.trim();
        if trimmed.is_empty() {
            continue;
        }
        let indent = without_comment.len() - without_comment.trim_start().len();
        if trimmed == "environment:" {
            environment_indent = Some(indent);
            continue;
        }
        let Some(parent_indent) = environment_indent else {
            continue;
        };
        if indent <= parent_indent {
            environment_indent = None;
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        if key.trim() == "sdk" {
            let value = unquote(value);
            if !value.is_empty() {
                push_dimension(output, ContextDimensionKind::LanguageVersion, value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_only_environment_sdk_constraint() {
        let mut output = Vec::new();
        collect(
            "pubspec.yaml",
            "name: sample\nenvironment:\n  sdk: '>=3.0.0 <4.0.0'\ndependencies:\n  sdk: fake\n",
            &mut output,
        );
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].value, ">=3.0.0 <4.0.0");
    }
}
