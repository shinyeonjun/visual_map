use super::push_dimension;
use codebase_fact_model::analysis::{ContextDimension, ContextDimensionKind};
use std::env;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GoExecutionEnvironment {
    pub(crate) platform: String,
    pub(crate) architecture: String,
    pub(crate) flags: String,
    pub(crate) build_tags: Vec<String>,
}

pub(crate) fn execution_environment() -> GoExecutionEnvironment {
    let platform = non_blank_env("GOOS").unwrap_or_else(host_goos);
    let architecture = non_blank_env("GOARCH").unwrap_or_else(host_goarch);
    let mut build_tags = env::var("GOFLAGS")
        .ok()
        .map(|flags| parse_build_tags(&flags))
        .unwrap_or_default();
    build_tags.sort();
    build_tags.dedup();
    let flags = if build_tags.is_empty() {
        String::new()
    } else {
        format!("-tags={}", build_tags.join(","))
    };
    GoExecutionEnvironment {
        platform,
        architecture,
        flags,
        build_tags,
    }
}

pub(super) fn collect_config(name: &str, text: &str, output: &mut Vec<ContextDimension>) {
    if !matches!(name, "go.mod" | "go.work") {
        return;
    }
    if let Some(version) = text.lines().find_map(|line| {
        line.trim()
            .strip_prefix("go ")
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }) {
        push_dimension(output, ContextDimensionKind::LanguageVersion, version);
    }
}

pub(super) fn collect_execution_environment(output: &mut Vec<ContextDimension>) {
    let environment = execution_environment();
    push_dimension(output, ContextDimensionKind::Platform, environment.platform);
    push_dimension(
        output,
        ContextDimensionKind::Architecture,
        environment.architecture,
    );
    if environment.build_tags.is_empty() {
        push_dimension(output, ContextDimensionKind::BuildTag, "none");
    } else {
        for tag in environment.build_tags {
            push_dimension(output, ContextDimensionKind::BuildTag, tag);
        }
    }
}

fn non_blank_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
}

fn parse_build_tags(flags: &str) -> Vec<String> {
    let tokens = flags.split_whitespace().collect::<Vec<_>>();
    let mut tags = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        let value = token
            .strip_prefix("-tags=")
            .or_else(|| token.strip_prefix("--tags="))
            .or_else(|| {
                matches!(token, "-tags" | "--tags")
                    .then(|| tokens.get(index + 1).copied())
                    .flatten()
            });
        if let Some(value) = value {
            tags.extend(
                value
                    .split([',', ' '])
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_ascii_lowercase),
            );
            if matches!(token, "-tags" | "--tags") {
                index += 1;
            }
        }
        index += 1;
    }
    tags
}

fn host_goos() -> String {
    match env::consts::OS {
        "macos" => "darwin",
        other => other,
    }
    .to_string()
}

fn host_goarch() -> String {
    match env::consts::ARCH {
        "x86" => "386",
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "arm" => "arm",
        other => other,
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_semantic_build_tags_survive_goflags() {
        assert_eq!(
            parse_build_tags("-mod=readonly -tags=enterprise,sqlite -trimpath"),
            vec!["enterprise", "sqlite"]
        );
    }
}
