use super::{push_dimension, xml_tag_values};
use codebase_fact_model::analysis::{ContextDimension, ContextDimensionKind};
use codebase_fact_model::source::RepositoryPath;

pub(super) fn collect_config(name: &str, text: &str, output: &mut Vec<ContextDimension>) {
    if name == "pom.xml" {
        collect_maven(text, output);
    } else if matches!(
        name,
        "build.gradle" | "build.gradle.kts" | "settings.gradle" | "settings.gradle.kts"
    ) {
        collect_gradle(text, output);
    }
}

pub(super) fn collect_source_sets(
    source_paths: &[RepositoryPath],
    output: &mut Vec<ContextDimension>,
) {
    for path in source_paths {
        let components = path.as_str().split('/').collect::<Vec<_>>();
        for window in components.windows(3) {
            if window[0].eq_ignore_ascii_case("src") && window[2].eq_ignore_ascii_case("java") {
                push_dimension(output, ContextDimensionKind::SourceSet, window[1]);
            }
        }
    }
}

fn collect_maven(text: &str, output: &mut Vec<ContextDimension>) {
    for tag in [
        "maven.compiler.release",
        "maven.compiler.source",
        "release",
        "source",
    ] {
        for value in xml_tag_values(text, tag) {
            let Some(value) = resolve_property(text, value) else {
                continue;
            };
            if is_java_version(value) {
                push_dimension(output, ContextDimensionKind::LanguageVersion, value);
            }
        }
    }
}

fn resolve_property<'a>(text: &'a str, value: &'a str) -> Option<&'a str> {
    let value = value.trim();
    if let Some(key) = value
        .strip_prefix("${")
        .and_then(|value| value.strip_suffix('}'))
    {
        return xml_tag_values(text, key)
            .into_iter()
            .find(|item| !item.contains("${"));
    }
    (!value.contains("${")).then_some(value)
}

fn is_java_version(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.len() <= 16
        && value
            .chars()
            .all(|character| character.is_ascii_digit() || character == '.')
}

fn collect_gradle(text: &str, output: &mut Vec<ContextDimension>) {
    for raw_line in text.lines() {
        let line = raw_line.split("//").next().unwrap_or_default();
        if ![
            "sourceCompatibility",
            "targetCompatibility",
            "languageVersion",
            "options.release",
        ]
        .iter()
        .any(|marker| line.contains(marker))
        {
            continue;
        }
        if let Some(value) = gradle_java_version(line) {
            push_dimension(output, ContextDimensionKind::LanguageVersion, value);
        }
    }
}

fn gradle_java_version(line: &str) -> Option<String> {
    if let Some((_, suffix)) = line.split_once("VERSION_") {
        let digits = suffix
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>();
        return (!digits.is_empty()).then_some(digits);
    }
    if let Some((_, suffix)) = line.split_once(".of(") {
        let digits = suffix
            .chars()
            .skip_while(|character| !character.is_ascii_digit())
            .take_while(char::is_ascii_digit)
            .collect::<String>();
        return (!digits.is_empty()).then_some(digits);
    }
    let (_, value) = line.split_once('=')?;
    let value = value.trim().trim_matches(['\'', '"']);
    is_java_version(value).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_maven_release_property_and_standard_source_set() {
        let mut output = Vec::new();
        collect_maven(
            "<properties><java.version>21</java.version></properties><release>${java.version}</release>",
            &mut output,
        );
        collect_source_sets(
            &[RepositoryPath::parse("src/main/java/demo/App.java").unwrap()],
            &mut output,
        );
        assert!(output.iter().any(|item| item.value == "21"));
        assert!(output.iter().any(|item| item.value == "main"));
    }

    #[test]
    fn does_not_treat_resource_source_path_as_language_version() {
        let mut output = Vec::new();
        collect_maven("<source>src/main/resources</source>", &mut output);
        assert!(output.is_empty());
    }
}
