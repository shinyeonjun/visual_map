use super::{literal_assignment_in_section, push_dimension};
use codebase_fact_model::analysis::{ContextDimension, ContextDimensionKind};

pub(super) fn collect(name: &str, text: &str, output: &mut Vec<ContextDimension>) {
    match name {
        "pyrightconfig.json" => collect_pyright_json(text, output),
        "pyproject.toml" => collect_pyproject(text, output),
        "pyvenv.cfg" => collect_venv(text, output),
        _ => {}
    }
}

fn collect_pyright_json(text: &str, output: &mut Vec<ContextDimension>) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return;
    };
    if let Some(version) = value
        .get("pythonVersion")
        .and_then(serde_json::Value::as_str)
    {
        push_dimension(output, ContextDimensionKind::LanguageVersion, version);
    }
    if let Some(platform) = value
        .get("pythonPlatform")
        .and_then(serde_json::Value::as_str)
    {
        push_dimension(output, ContextDimensionKind::Platform, platform);
    }
}

fn collect_pyproject(text: &str, output: &mut Vec<ContextDimension>) {
    if let Some(version) = literal_assignment_in_section(text, "tool.pyright", "pythonVersion") {
        push_dimension(output, ContextDimensionKind::LanguageVersion, version);
    }
    if let Some(platform) = literal_assignment_in_section(text, "tool.pyright", "pythonPlatform") {
        push_dimension(output, ContextDimensionKind::Platform, platform);
    }
}

fn collect_venv(text: &str, output: &mut Vec<ContextDimension>) {
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case("version") {
            push_dimension(output, ContextDimensionKind::LanguageVersion, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_pyright_version_and_platform_without_using_package_constraint() {
        let mut output = Vec::new();
        collect(
            "pyproject.toml",
            "[project]\nrequires-python='>=3.8'\n[tool.pyright]\npythonVersion='3.12'\npythonPlatform='All'\n",
            &mut output,
        );
        assert!(output.iter().any(|item| item.value == "3.12"));
        assert!(output.iter().any(|item| item.value == "all"));
        assert!(!output.iter().any(|item| item.value.contains(">=")));
    }
}
