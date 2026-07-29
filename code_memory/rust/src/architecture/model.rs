use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone)]
pub(crate) struct PackageInfo {
    pub(crate) root: String,
    pub(crate) ecosystem: String,
    pub(crate) name: String,
    pub(crate) version: Option<String>,
}

#[derive(Clone)]
pub(crate) struct ImportUse {
    pub(crate) path: String,
    pub(crate) language: String,
    pub(crate) package: String,
    pub(crate) alias: Option<String>,
    pub(crate) member: Option<String>,
    pub(crate) line: usize,
}

/// Immutable lookup tables for project source paths.
///
/// A suffix is kept only until ambiguity is known.  Two paths are enough to
/// distinguish a unique target from an ambiguous target, so memory stays
/// bounded while import resolution avoids scanning every source for each
/// import.
pub(crate) struct SourcePathIndex {
    exact: HashSet<String>,
    suffixes: HashMap<String, Vec<String>>,
    go_directories: HashMap<String, Vec<String>>,
    go_files: HashMap<String, Vec<String>>,
}

impl SourcePathIndex {
    pub(crate) fn new(sources: &HashMap<String, String>) -> Self {
        let mut index = Self {
            exact: HashSet::with_capacity(sources.len()),
            suffixes: HashMap::new(),
            go_directories: HashMap::new(),
            go_files: HashMap::new(),
        };
        for path in sources.keys() {
            let path = path.replace('\\', "/");
            index.exact.insert(path.clone());
            let mut start = 0;
            loop {
                let suffix = path[start..].to_string();
                let candidates = index.suffixes.entry(suffix).or_default();
                if candidates.len() < 2 {
                    candidates.push(path.clone());
                }
                let Some(separator) = path[start..].find('/') else {
                    break;
                };
                start += separator + 1;
            }
            if path.ends_with(".go") {
                let directory = path
                    .rsplit_once('/')
                    .map(|(value, _)| value)
                    .unwrap_or("")
                    .to_string();
                index
                    .go_files
                    .entry(directory.clone())
                    .or_default()
                    .push(path.clone());
                let mut directory_start = 0;
                loop {
                    let suffix = directory[directory_start..].to_string();
                    let candidates = index.go_directories.entry(suffix).or_default();
                    if candidates.len() < 2 && !candidates.contains(&directory) {
                        candidates.push(directory.clone());
                    }
                    let Some(separator) = directory[directory_start..].find('/') else {
                        break;
                    };
                    directory_start += separator + 1;
                }
            }
        }
        for files in index.go_files.values_mut() {
            files.sort();
        }
        index
    }

    pub(crate) fn exact(&self, candidate: &str) -> Option<String> {
        let candidate = candidate.replace('\\', "/");
        self.exact.contains(&candidate).then_some(candidate)
    }

    pub(crate) fn unique_suffix(&self, candidate: &str) -> Option<String> {
        let candidate = candidate.trim_matches('/').replace('\\', "/");
        let candidates = self.suffixes.get(&candidate)?;
        (candidates.len() == 1).then(|| candidates[0].clone())
    }

    pub(crate) fn unique_go_module_file(&self, module_path: &str) -> Option<String> {
        let module_path = module_path.trim_matches('/').replace('\\', "/");
        let directories = self.go_directories.get(&module_path)?;
        if directories.len() != 1 {
            return None;
        }
        self.go_files
            .get(&directories[0])
            .and_then(|files| files.first())
            .cloned()
    }
}

pub(crate) type PhpNamespaceIndex = HashMap<String, String>;

pub(crate) struct EdgeDraft {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) kind: String,
    pub(crate) level: String,
    pub(crate) properties: BTreeMap<String, String>,
    pub(crate) evidence: ArchitectureEvidence,
}

#[derive(Serialize)]
pub(crate) struct ArchitectureOutput {
    pub(crate) schema: &'static str,
    pub(crate) project_root: String,
    pub(crate) nodes: Vec<ArchitectureNode>,
    pub(crate) edges: Vec<ArchitectureEdge>,
    pub(crate) flows: Vec<ArchitectureFlow>,
    pub(crate) diagnostics: Vec<ArchitectureDiagnostic>,
}

#[derive(Serialize, Clone)]
pub(crate) struct ArchitectureNode {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) name: String,
    pub(crate) label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) external: Option<bool>,
    pub(crate) properties: BTreeMap<String, String>,
}

#[derive(Serialize, Clone)]
pub(crate) struct ArchitectureEdge {
    pub(crate) id: String,
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) kind: String,
    pub(crate) level: String,
    pub(crate) properties: BTreeMap<String, String>,
    pub(crate) evidence: Vec<ArchitectureEvidence>,
}

#[derive(Serialize, Clone, PartialEq, Eq)]
pub(crate) struct ArchitectureEvidence {
    pub(crate) path: String,
    pub(crate) range: Vec<i32>,
    pub(crate) note: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct ArchitectureFlow {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) label: String,
    pub(crate) entrypoint: String,
    pub(crate) node_ids: Vec<String>,
    pub(crate) edge_ids: Vec<String>,
    pub(crate) truncated: bool,
    pub(crate) omitted_node_count: usize,
}

#[derive(Serialize)]
pub(crate) struct ArchitectureDiagnostic {
    pub(crate) kind: String,
    pub(crate) path: Option<String>,
    pub(crate) message: String,
}
