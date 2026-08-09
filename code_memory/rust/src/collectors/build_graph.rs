use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use super::discovery::{find_files, read_descriptor, relative_path, stable_segment};
use super::model::{
    properties, CollectedEvidence, CollectedFact, CollectedRelation, CollectionDiagnostic,
    CollectionMode, CollectionStatus, CollectorResult, TruthClass,
};

const ID: &str = "build-graph";

struct BuildUnit {
    stable_key: String,
    ecosystem: &'static str,
    name: String,
    version: Option<String>,
    manifest: String,
    directory: String,
    dependencies: Vec<Dependency>,
}

struct Dependency {
    name: String,
    scope: &'static str,
}

type ParsedPackage = (&'static str, String, Option<String>, Vec<Dependency>);

pub(crate) fn collect(root: &Path, _providers_root: Option<&Path>) -> CollectorResult {
    let mut result = CollectorResult::new(ID, "build-graph", CollectionMode::Passive);
    let manifests: Vec<PathBuf> = find_files(root, is_primary_manifest);
    if manifests.is_empty() {
        return result;
    }

    let mut units = Vec::new();
    for manifest in manifests {
        let relative = relative_path(root, &manifest);
        result.summary.detected_by.push(relative.clone());
        match parse_unit(root, &manifest) {
            Ok(unit) => units.push(unit),
            Err(message) => result.diagnostics.push(CollectionDiagnostic {
                collector: ID,
                level: "warning",
                code: "invalid-build-descriptor",
                message,
                path: Some(relative),
            }),
        }
    }
    result.summary.detected_by.sort();
    result.summary.detected_by.dedup();
    if units.is_empty() {
        result.summary.status = CollectionStatus::Failed;
        return result;
    }

    dedupe_units(&mut units);
    let project_key = "project:root".to_string();
    result.facts.push(CollectedFact {
        stable_key: project_key.clone(),
        kind: "project".to_string(),
        name: root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project")
            .to_string(),
        path: Some(".".to_string()),
        properties: BTreeMap::new(),
    });

    let mut names: HashMap<String, Vec<String>> = HashMap::new();
    for unit in &units {
        names
            .entry(unit.name.clone())
            .or_default()
            .push(unit.stable_key.clone());
        result.facts.push(unit_fact(unit));
    }

    for unit in &units {
        let parent = nearest_parent(unit, &units)
            .map(|parent| parent.stable_key.clone())
            .unwrap_or_else(|| project_key.clone());
        result.relations.push(relation(
            parent,
            unit.stable_key.clone(),
            "CONTAINS",
            &unit.manifest,
            None,
        ));

        for dependency in &unit.dependencies {
            let target = names
                .get(&dependency.name)
                .filter(|matches| matches.len() == 1)
                .and_then(|matches| matches.first())
                .cloned()
                .unwrap_or_else(|| {
                    let key = format!(
                        "dependency:{}:{}",
                        unit.ecosystem,
                        stable_segment(&dependency.name)
                    );
                    result.facts.push(CollectedFact {
                        stable_key: key.clone(),
                        kind: "external-package".to_string(),
                        name: dependency.name.clone(),
                        path: None,
                        properties: properties(&[("ecosystem", Some(unit.ecosystem))]),
                    });
                    key
                });
            let mut dependency_relation = relation(
                unit.stable_key.clone(),
                target,
                "DEPENDS_ON",
                &unit.manifest,
                None,
            );
            dependency_relation
                .properties
                .insert("scope".to_string(), dependency.scope.to_string());
            result.relations.push(dependency_relation);
        }
    }

    result.summary.status = if result.diagnostics.is_empty() {
        CollectionStatus::Collected
    } else {
        CollectionStatus::Partial
    };
    result
}

fn is_primary_manifest(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    matches!(
        name,
        "package.json"
            | "Cargo.toml"
            | "go.mod"
            | "pyproject.toml"
            | "setup.py"
            | "requirements.txt"
            | "pom.xml"
            | "settings.gradle"
            | "settings.gradle.kts"
            | "pubspec.yaml"
            | "CMakeLists.txt"
            | "meson.build"
    ) || name.to_ascii_lowercase().ends_with(".csproj")
}

fn parse_unit(root: &Path, path: &Path) -> Result<BuildUnit, String> {
    let source = read_descriptor(path)?;
    let manifest = relative_path(root, path);
    let directory = path
        .parent()
        .map(|parent| relative_path(root, parent))
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| ".".to_string());
    let fallback_name = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .or_else(|| root.file_name().and_then(|name| name.to_str()))
        .unwrap_or("project")
        .to_string();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let (ecosystem, name, version, dependencies) = match file_name {
        "package.json" => parse_json_package(&source, "npm", &fallback_name)?,
        "Cargo.toml" => (
            "cargo",
            toml_scalar(&source, "package", "name").unwrap_or(fallback_name),
            toml_scalar(&source, "package", "version"),
            toml_dependencies(&source),
        ),
        "pyproject.toml" => (
            "python",
            toml_scalar(&source, "project", "name").unwrap_or(fallback_name),
            toml_scalar(&source, "project", "version"),
            Vec::new(),
        ),
        "go.mod" => (
            "go",
            source
                .lines()
                .find_map(|line| line.trim().strip_prefix("module ").map(str::trim))
                .filter(|value| !value.is_empty())
                .unwrap_or(&fallback_name)
                .to_string(),
            None,
            go_dependencies(&source),
        ),
        "pom.xml" => (
            "maven",
            xml_tag(&source, "artifactId").unwrap_or(fallback_name),
            xml_tag(&source, "version"),
            Vec::new(),
        ),
        "settings.gradle" | "settings.gradle.kts" => (
            "gradle",
            gradle_root_name(&source).unwrap_or(fallback_name),
            None,
            Vec::new(),
        ),
        "pubspec.yaml" => (
            "pub",
            yaml_scalar(&source, "name").unwrap_or(fallback_name),
            yaml_scalar(&source, "version"),
            yaml_dependencies(&source),
        ),
        "CMakeLists.txt" => (
            "cmake",
            cmake_project_name(&source).unwrap_or(fallback_name),
            None,
            Vec::new(),
        ),
        "meson.build" => (
            "meson",
            meson_project_name(&source).unwrap_or(fallback_name),
            None,
            Vec::new(),
        ),
        "setup.py" | "requirements.txt" => ("python", fallback_name, None, Vec::new()),
        _ if file_name.to_ascii_lowercase().ends_with(".csproj") => (
            "dotnet",
            xml_tag(&source, "AssemblyName").unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or(&fallback_name)
                    .to_string()
            }),
            xml_tag(&source, "Version"),
            Vec::new(),
        ),
        _ => return Err(format!("unsupported build descriptor: {manifest}")),
    };
    let key_path = if directory == "." { "root" } else { &directory };
    Ok(BuildUnit {
        stable_key: format!("build:{ecosystem}:{}", stable_segment(key_path)),
        ecosystem,
        name,
        version,
        manifest,
        directory,
        dependencies,
    })
}

fn parse_json_package(
    source: &str,
    ecosystem: &'static str,
    fallback_name: &str,
) -> Result<ParsedPackage, String> {
    let value: Value = serde_json::from_str(source)
        .map_err(|error| format!("invalid {ecosystem} package JSON: {error}"))?;
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(fallback_name)
        .to_string();
    let version = value
        .get("version")
        .and_then(Value::as_str)
        .map(str::to_string);
    let sections: &[(&str, &'static str)] = &[
        ("dependencies", "runtime"),
        ("optionalDependencies", "optional"),
        ("peerDependencies", "peer"),
        ("devDependencies", "development"),
    ];
    let mut dependencies = Vec::new();
    for (section, scope) in sections {
        dependencies.extend(
            value
                .get(*section)
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
                .map(|(name, _)| Dependency {
                    name: name.clone(),
                    scope,
                }),
        );
    }
    Ok((ecosystem, name, version, dependencies))
}

fn toml_scalar(source: &str, expected_section: &str, key: &str) -> Option<String> {
    let mut section = "";
    for line in source.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = line.trim_matches(['[', ']']).trim();
            continue;
        }
        if section != expected_section {
            continue;
        }
        let Some((candidate, value)) = line.split_once('=') else {
            continue;
        };
        if candidate.trim() == key {
            return quoted_scalar(value);
        }
    }
    None
}

fn toml_dependencies(source: &str) -> Vec<Dependency> {
    let mut section = "";
    let mut result = Vec::new();
    for raw in source.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = line.trim_matches(['[', ']']).trim();
            continue;
        }
        let scope = match section {
            "dependencies" | "workspace.dependencies" => "runtime",
            "dev-dependencies" => "development",
            "build-dependencies" => "build",
            _ => continue,
        };
        if let Some((name, _)) = line.split_once('=') {
            let name = name.trim().trim_matches(['\'', '"']);
            if !name.is_empty() {
                result.push(Dependency {
                    name: name.to_string(),
                    scope,
                });
            }
        }
    }
    result
}

fn go_dependencies(source: &str) -> Vec<Dependency> {
    let mut in_block = false;
    let mut result = Vec::new();
    for line in source.lines().map(str::trim) {
        if line == "require (" {
            in_block = true;
            continue;
        }
        if in_block && line == ")" {
            in_block = false;
            continue;
        }
        let value = if in_block {
            Some(line)
        } else {
            line.strip_prefix("require ")
        };
        if let Some(name) = value.and_then(|value| value.split_whitespace().next()) {
            if !name.is_empty() {
                result.push(Dependency {
                    name: name.to_string(),
                    scope: "runtime",
                });
            }
        }
    }
    result
}

fn yaml_dependencies(source: &str) -> Vec<Dependency> {
    let mut scope = None;
    let mut result = Vec::new();
    for raw in source.lines() {
        let indent = raw.len() - raw.trim_start().len();
        let line = raw.trim();
        if indent == 0 {
            scope = match line {
                "dependencies:" => Some("runtime"),
                "dev_dependencies:" => Some("development"),
                _ => None,
            };
            continue;
        }
        if indent > 0 {
            if let (Some(scope), Some((name, _))) = (scope, line.split_once(':')) {
                if !name.trim().is_empty() {
                    result.push(Dependency {
                        name: name.trim().to_string(),
                        scope,
                    });
                }
            }
        }
    }
    result
}

fn quoted_scalar(value: &str) -> Option<String> {
    let value = value.trim();
    let quote = value.chars().next()?;
    if !matches!(quote, '\'' | '"') {
        return None;
    }
    value[1..].find(quote).map(|end| value[1..=end].to_string())
}

fn xml_tag(source: &str, tag: &str) -> Option<String> {
    let start_marker = format!("<{tag}>");
    let end_marker = format!("</{tag}>");
    let start = source.find(&start_marker)? + start_marker.len();
    let end = source[start..].find(&end_marker)? + start;
    Some(source[start..end].trim().to_string()).filter(|value| !value.is_empty())
}

fn yaml_scalar(source: &str, key: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let (candidate, value) = line.split_once(':')?;
        (candidate.trim() == key)
            .then(|| value.trim().trim_matches(['\'', '"']).to_string())
            .filter(|value| !value.is_empty())
    })
}

fn gradle_root_name(source: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let line = line.trim();
        let (_, value) = line.split_once('=')?;
        line.starts_with("rootProject.name")
            .then(|| value.trim().trim_matches(['\'', '"']).to_string())
            .filter(|value| !value.is_empty())
    })
}

fn cmake_project_name(source: &str) -> Option<String> {
    let start = source.to_ascii_lowercase().find("project(")? + "project(".len();
    source[start..]
        .split(|character: char| character.is_whitespace() || character == ')')
        .next()
        .map(str::to_string)
        .filter(|value| !value.is_empty())
}

fn meson_project_name(source: &str) -> Option<String> {
    let start = source.find("project(")? + "project(".len();
    quoted_scalar(&source[start..])
}

fn dedupe_units(units: &mut Vec<BuildUnit>) {
    let mut seen = BTreeSet::new();
    units.retain(|unit| seen.insert(unit.stable_key.clone()));
    units.sort_by(|left, right| left.stable_key.cmp(&right.stable_key));
}

fn nearest_parent<'a>(unit: &BuildUnit, units: &'a [BuildUnit]) -> Option<&'a BuildUnit> {
    units
        .iter()
        .filter(|candidate| candidate.stable_key != unit.stable_key)
        .filter(|candidate| {
            candidate.directory == "."
                || (unit.directory.starts_with(&candidate.directory)
                    && unit.directory.as_bytes().get(candidate.directory.len()) == Some(&b'/'))
        })
        .max_by_key(|candidate| candidate.directory.len())
}

fn unit_fact(unit: &BuildUnit) -> CollectedFact {
    CollectedFact {
        stable_key: unit.stable_key.clone(),
        kind: "build-unit".to_string(),
        name: unit.name.clone(),
        path: Some(unit.directory.clone()),
        properties: properties(&[
            ("ecosystem", Some(unit.ecosystem)),
            ("manifest", Some(&unit.manifest)),
            ("version", unit.version.as_deref()),
            ("source_scope", source_scope(&unit.directory)),
        ]),
    }
}

fn source_scope(path: &str) -> Option<&'static str> {
    path.split('/')
        .any(|segment| {
            matches!(
                segment.to_ascii_lowercase().as_str(),
                "test" | "tests" | "fixture" | "fixtures" | "example" | "examples"
            )
        })
        .then_some("test")
}

fn relation(
    from: String,
    to: String,
    kind: &str,
    path: &str,
    line: Option<u32>,
) -> CollectedRelation {
    CollectedRelation {
        from,
        to,
        kind: kind.to_string(),
        truth_class: TruthClass::Confirmed,
        evidence_type: "BUILD_DESCRIPTOR".to_string(),
        evidence: vec![CollectedEvidence {
            path: path.to_string(),
            line,
            note: None,
        }],
        properties: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::collect;

    #[test]
    fn package_manifests_create_units_and_dependencies() {
        let root = std::env::temp_dir().join(format!(
            "code-memory-build-collector-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("packages/api")).unwrap();
        std::fs::create_dir_all(root.join("tests/fixture")).unwrap();
        std::fs::write(
            root.join("package.json"),
            r#"{"name":"workspace","dependencies":{"left-pad":"1"}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("packages/api/package.json"),
            r#"{"name":"api","dependencies":{"workspace":"*"}}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("tests/fixture/package.json"),
            r#"{"name":"fixture"}"#,
        )
        .unwrap();

        let result = collect(&root, None);
        assert_eq!(
            result
                .facts
                .iter()
                .filter(|fact| fact.kind == "build-unit")
                .count(),
            3
        );
        assert!(result
            .relations
            .iter()
            .any(|relation| relation.kind == "DEPENDS_ON" && relation.to == "build:npm:root"));
        assert!(result.facts.iter().any(|fact| {
            fact.name == "fixture"
                && fact.properties.get("source_scope").map(String::as_str) == Some("test")
        }));
        let _ = std::fs::remove_dir_all(root);
    }
}
