use super::{native_path, push_dimension};
use codebase_fact_model::analysis::{ContextDimension, ContextDimensionKind, ProgrammingLanguage};
use codebase_fact_model::source::RepositoryPath;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub(super) fn collect(
    language: ProgrammingLanguage,
    project_root: &Path,
    config_path: &RepositoryPath,
    text: &str,
    source_paths: &[RepositoryPath],
    output: &mut Vec<ContextDimension>,
) {
    let name = config_path
        .as_str()
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if name == "compile_commands.json" {
        collect_compile_database(
            language,
            project_root,
            config_path,
            text,
            source_paths,
            output,
        );
    } else if name == "compile_flags.txt" {
        let tokens = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(str::to_string)
            .collect::<Vec<_>>();
        collect_tokens(language, &tokens, output);
    }
}

fn collect_compile_database(
    language: ProgrammingLanguage,
    project_root: &Path,
    config_path: &RepositoryPath,
    text: &str,
    source_paths: &[RepositoryPath],
    output: &mut Vec<ContextDimension>,
) {
    let Ok(entries) = serde_json::from_str::<Value>(text) else {
        return;
    };
    let Some(entries) = entries.as_array() else {
        return;
    };
    let source_paths = source_paths.iter().cloned().collect::<BTreeSet<_>>();
    let config_directory = native_path(project_root, config_path)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| project_root.to_path_buf());
    for entry in entries {
        let Some(file) = entry.get("file").and_then(Value::as_str) else {
            continue;
        };
        let directory = entry
            .get("directory")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .map(|path| {
                if path.is_absolute() {
                    path
                } else {
                    config_directory.join(path)
                }
            })
            .unwrap_or_else(|| config_directory.clone());
        let file = PathBuf::from(file);
        let absolute_file = if file.is_absolute() {
            file
        } else {
            directory.join(file)
        };
        let Some(repository_file) = repository_path(project_root, &absolute_file) else {
            continue;
        };
        if !source_paths.contains(&repository_file)
            || !source_matches_language(language, &repository_file)
        {
            continue;
        }
        let tokens = if let Some(arguments) = entry.get("arguments").and_then(Value::as_array) {
            arguments
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        } else {
            entry
                .get("command")
                .and_then(Value::as_str)
                .map(tokenize_command)
                .unwrap_or_default()
        };
        collect_tokens(language, &tokens, output);
    }
}

fn collect_tokens(
    language: ProgrammingLanguage,
    tokens: &[String],
    output: &mut Vec<ContextDimension>,
) {
    let mut standard = None;
    let mut target = None;
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index].trim();
        if let Some(value) = token
            .strip_prefix("-std=")
            .or_else(|| token.strip_prefix("--std="))
            .or_else(|| token.strip_prefix("/std:"))
        {
            if standard_applies(language, value) {
                standard = Some(value.to_string());
            }
        } else if matches!(token, "-std" | "--std") {
            if let Some(value) = tokens
                .get(index + 1)
                .filter(|value| standard_applies(language, value))
            {
                standard = Some(value.clone());
                index += 1;
            }
        } else if let Some(value) = token.strip_prefix("--target=") {
            target = Some(value.to_string());
        } else if matches!(token, "--target" | "-target") {
            if let Some(value) = tokens.get(index + 1) {
                target = Some(value.clone());
                index += 1;
            }
        } else if token == "-m32" {
            target = Some("host-32".to_string());
        } else if token == "-m64" {
            target = Some("host-64".to_string());
        }
        index += 1;
    }
    push_dimension(
        output,
        ContextDimensionKind::LanguageVersion,
        standard.unwrap_or_else(|| "default".to_string()),
    );
    push_dimension(
        output,
        ContextDimensionKind::Target,
        target.unwrap_or_else(|| "host".to_string()),
    );
}

fn source_matches_language(language: ProgrammingLanguage, path: &RepositoryPath) -> bool {
    let extension = path
        .as_str()
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase());
    match language {
        ProgrammingLanguage::C => extension.as_deref() == Some("c"),
        ProgrammingLanguage::Cpp => matches!(
            extension.as_deref(),
            Some("cc" | "cp" | "cpp" | "cxx" | "c++")
        ),
        _ => false,
    }
}

fn standard_applies(language: ProgrammingLanguage, value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    match language {
        ProgrammingLanguage::C => {
            !value.contains("++") && (value.starts_with('c') || value.starts_with("gnu"))
        }
        ProgrammingLanguage::Cpp => value.contains("++"),
        _ => false,
    }
}

fn repository_path(root: &Path, path: &Path) -> Option<RepositoryPath> {
    let root = root.canonicalize().ok()?;
    let path = path.canonicalize().ok()?;
    let relative = path.strip_prefix(root).ok()?;
    RepositoryPath::parse(relative.to_string_lossy().replace('\\', "/")).ok()
}

fn tokenize_command(command: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut quote = None;
    let mut escaped = false;
    for character in command.chars() {
        if escaped {
            token.push(character);
            escaped = false;
            continue;
        }
        if character == '\\' && quote == Some('"') {
            escaped = true;
            continue;
        }
        if matches!(character, '\'' | '"') {
            if quote == Some(character) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(character);
            } else {
                token.push(character);
            }
            continue;
        }
        if character.is_whitespace() && quote.is_none() {
            if !token.is_empty() {
                tokens.push(std::mem::take(&mut token));
            }
        } else {
            token.push(character);
        }
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_c_and_cpp_standards_separate() {
        let tokens = tokenize_command("clang++ --target=x86_64-test -std=c++20 -c main.cpp");
        let mut cpp = Vec::new();
        collect_tokens(ProgrammingLanguage::Cpp, &tokens, &mut cpp);
        assert!(cpp.iter().any(|item| item.value == "c++20"));
        assert!(cpp.iter().any(|item| item.value == "x86_64-test"));

        let mut c = Vec::new();
        collect_tokens(ProgrammingLanguage::C, &tokens, &mut c);
        assert!(c.iter().any(|item| item.value == "default"));
        assert!(!c.iter().any(|item| item.value == "c++20"));
    }

    #[test]
    fn tokenizer_preserves_quoted_target() {
        assert_eq!(
            tokenize_command("clang --target=\"x86_64 unknown\" -c main.c"),
            vec!["clang", "--target=x86_64 unknown", "-c", "main.c"]
        );
    }
}
