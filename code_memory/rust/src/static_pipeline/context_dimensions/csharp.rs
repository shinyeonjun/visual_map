use super::{push_dimension, xml_tag_values};
use codebase_fact_model::analysis::{ContextDimension, ContextDimensionKind};

pub(super) fn collect(name: &str, text: &str, output: &mut Vec<ContextDimension>) {
    if !(name.ends_with(".csproj") || name.ends_with(".props") || name.ends_with(".targets")) {
        return;
    }
    let property_groups = unconditional_property_groups(text);
    for group in property_groups {
        collect_split_values(
            group,
            &["TargetFramework", "TargetFrameworks"],
            ContextDimensionKind::TargetFramework,
            output,
        );
        collect_split_values(
            group,
            &["Configuration", "Configurations"],
            ContextDimensionKind::Profile,
            output,
        );
        collect_split_values(
            group,
            &["Platform", "Platforms"],
            ContextDimensionKind::Platform,
            output,
        );
    }
}

fn collect_split_values(
    text: &str,
    tags: &[&str],
    kind: ContextDimensionKind,
    output: &mut Vec<ContextDimension>,
) {
    for tag in tags {
        for value in xml_tag_values(text, tag) {
            if value.contains("$(") {
                continue;
            }
            for value in value
                .split(';')
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                push_dimension(output, kind, value);
            }
        }
    }
}

/// Conditional property groups describe possible builds, not necessarily the
/// design-time build that scip-dotnet loaded. Only unconditional literals may
/// be called an executed dimension.
fn unconditional_property_groups(text: &str) -> Vec<&str> {
    let mut remaining = text;
    let mut groups = Vec::new();
    while let Some(start) = find_ascii_case_insensitive(remaining, "<propertygroup") {
        let after_start = &remaining[start..];
        let Some(open_end) = after_start.find('>') else {
            break;
        };
        let open = &after_start[..=open_end];
        let body_start = &after_start[open_end + 1..];
        let Some(close_start) = find_ascii_case_insensitive(body_start, "</propertygroup>") else {
            break;
        };
        if !open.to_ascii_lowercase().contains("condition=") {
            groups.push(&body_start[..close_start]);
        }
        remaining = &body_start[close_start + "</propertygroup>".len()..];
    }
    groups
}

fn find_ascii_case_insensitive(text: &str, needle: &str) -> Option<usize> {
    text.to_ascii_lowercase().find(&needle.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_conditional_configuration_and_keeps_executed_literals() {
        let text = r#"
          <Project>
            <PropertyGroup>
              <TargetFramework>net8.0</TargetFramework>
              <Configuration>Debug</Configuration>
              <Platform>AnyCPU</Platform>
            </PropertyGroup>
            <PropertyGroup Condition="'$(Configuration)' == 'Release'">
              <Configuration>Release</Configuration>
            </PropertyGroup>
          </Project>"#;
        let mut output = Vec::new();
        collect("app.csproj", text, &mut output);
        assert!(output.iter().any(|item| item.value == "net8.0"));
        assert!(output.iter().any(|item| item.value == "debug"));
        assert!(output.iter().any(|item| item.value == "anycpu"));
        assert!(!output.iter().any(|item| item.value == "release"));
    }
}
