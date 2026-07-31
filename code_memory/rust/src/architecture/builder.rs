use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::Path;

use super::*;

pub(crate) struct ArchitectureBuilder {
    root: String,
    nodes: BTreeMap<String, ArchitectureNode>,
    edges: BTreeMap<String, ArchitectureEdge>,
    evidence_keys: HashMap<String, HashSet<String>>,
    file_modules: HashMap<String, String>,
    symbol_modules: HashMap<String, String>,
    symbol_files: HashMap<String, String>,
    source_texts: HashMap<String, String>,
    source_index: SourcePathIndex,
    packages: Vec<PackageInfo>,
    php_namespace_index: PhpNamespaceIndex,
    imports: Vec<ImportUse>,
    entrypoints: Vec<(String, String, String)>,
    pub(crate) diagnostics: Vec<ArchitectureDiagnostic>,
}

impl ArchitectureBuilder {
    pub(crate) fn new(
        root: &Path,
        source_texts: HashMap<String, String>,
        packages: Vec<PackageInfo>,
    ) -> Self {
        let root_string = root.to_string_lossy().into_owned();
        let php_namespace_index = build_php_namespace_index(&source_texts);
        let source_index = SourcePathIndex::new(&source_texts);
        let mut builder = Self {
            root: root_string,
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
            evidence_keys: HashMap::new(),
            file_modules: HashMap::new(),
            symbol_modules: HashMap::new(),
            symbol_files: HashMap::new(),
            source_texts,
            source_index,
            packages,
            php_namespace_index,
            imports: Vec::new(),
            entrypoints: Vec::new(),
            diagnostics: Vec::new(),
        };
        builder.node(
            "project:root",
            "PROJECT",
            "project",
            "Project",
            None,
            None,
            None,
            false,
            BTreeMap::new(),
        );
        builder
    }

    pub(crate) fn node(
        &mut self,
        id: impl Into<String>,
        kind: impl Into<String>,
        name: impl Into<String>,
        label: impl Into<String>,
        path: Option<String>,
        language: Option<String>,
        parent_id: Option<String>,
        external: bool,
        properties: BTreeMap<String, String>,
    ) -> String {
        let id = id.into();
        self.nodes
            .entry(id.clone())
            .or_insert_with(|| ArchitectureNode {
                id: id.clone(),
                kind: kind.into(),
                name: name.into(),
                label: label.into(),
                path,
                language,
                parent_id,
                external: external.then_some(true),
                properties,
            });
        id
    }

    pub(crate) fn edge(
        &mut self,
        from: &str,
        to: &str,
        kind: &str,
        level: &str,
        properties: BTreeMap<String, String>,
        evidence: ArchitectureEvidence,
    ) -> String {
        let key = format!("{level}|{kind}|{from}|{to}");
        let id = format!("edge:{level}:{kind}:{from}:{to}");
        {
            let item = self
                .edges
                .entry(key.clone())
                .or_insert_with(|| ArchitectureEdge {
                    id: id.clone(),
                    from: from.to_string(),
                    to: to.to_string(),
                    kind: kind.to_string(),
                    level: level.to_string(),
                    properties: BTreeMap::new(),
                    evidence: Vec::new(),
                });
            for (key, value) in properties {
                item.properties.entry(key).or_insert(value);
            }
        }
        let evidence_key = format!("{}|{:?}|{:?}", evidence.path, evidence.range, evidence.note);
        // ponytail: evidence de-duplication is O(1) instead of scanning every
        // prior occurrence on a module edge; the serialized evidence stays
        // unchanged for VisualMap.
        if self
            .evidence_keys
            .entry(key.clone())
            .or_default()
            .insert(evidence_key)
        {
            self.edges
                .get_mut(&key)
                .expect("edge was inserted above")
                .evidence
                .push(evidence);
        }
        id
    }

    pub(crate) fn evidence(path: &str, line: usize, note: Option<String>) -> ArchitectureEvidence {
        ArchitectureEvidence {
            path: path.to_string(),
            range: vec![line.saturating_sub(1) as i32, 0, line as i32, 0],
            note,
        }
    }

    pub(crate) fn ensure_package_nodes(&mut self) {
        for package in self.packages.clone() {
            let id = package_id(&package);
            let mut properties = BTreeMap::new();
            properties.insert("ecosystem".to_string(), package.ecosystem.clone());
            if let Some(version) = &package.version {
                properties.insert("version".to_string(), version.clone());
            }
            self.node(
                id.clone(),
                "PACKAGE",
                package.name.clone(),
                package.name.clone(),
                Some(package.root.clone()),
                None,
                Some("project:root".to_string()),
                false,
                properties,
            );
            self.edge(
                "project:root",
                &id,
                "CONTAINS",
                "tree",
                BTreeMap::new(),
                Self::evidence(&package.root, 1, Some("package manifest".to_string())),
            );
        }
    }

    pub(crate) fn build_file_tree(
        &mut self,
        files: &[(String, String)],
        semantic_paths: &HashSet<String>,
    ) {
        self.ensure_package_nodes();
        let mut direct_file_counts: HashMap<String, usize> = HashMap::new();
        let mut directories = BTreeSet::new();
        for (path, language) in files {
            let _ = language;
            let directory = Self::file_directory(path);
            directories.insert(directory.clone());
            *direct_file_counts.entry(directory).or_default() += 1;
        }

        let mut child_directories: HashMap<String, BTreeSet<String>> = HashMap::new();
        for directory in &directories {
            if let Some(parent) = directory.rsplit_once('/').map(|(value, _)| value) {
                child_directories
                    .entry(parent.to_string())
                    .or_default()
                    .insert(directory.clone());
            }
        }
        let package_roots: HashSet<String> = self
            .packages
            .iter()
            .map(|package| package.root.clone())
            .collect();

        // ponytail: keep only structural boundaries in the overview; the file tree
        // remains complete, and a future view can expand directories if needed.
        let is_boundary = |directory: &str| {
            direct_file_counts.get(directory).copied().unwrap_or(0) >= 2
                || child_directories
                    .get(directory)
                    .is_some_and(|children| children.len() >= 2)
                || package_roots.contains(directory)
        };
        let mut module_for_path = HashMap::new();
        let mut module_state: BTreeMap<String, (String, String, String, bool, usize, String)> =
            BTreeMap::new();
        for (path, language) in files {
            let package = nearest_package(path, &self.packages);
            let package_root = package.map(|value| value.root.as_str()).unwrap_or("");
            let directory = Self::file_directory(path);
            let module_path = Self::compact_module_path(&directory, &is_boundary);
            let module_id = format!("module:{language}:{package_root}:{module_path}");
            let module_parent = package
                .map(package_id)
                .unwrap_or_else(|| "project:root".to_string());
            let entry = module_state.entry(module_id.clone()).or_insert_with(|| {
                (
                    module_path.clone(),
                    module_parent.clone(),
                    language.clone(),
                    false,
                    0,
                    path.clone(),
                )
            });
            entry.3 |= semantic_paths.contains(path);
            entry.4 += 1;
            module_for_path.insert(path.clone(), module_id);
        }
        for (
            module_id,
            (module_path, module_parent, language, has_semantics, file_count, evidence_path),
        ) in module_state
        {
            let module_name = module_path
                .rsplit('/')
                .next()
                .filter(|value| !value.is_empty())
                .unwrap_or("root")
                .to_string();
            let mut module_properties = BTreeMap::new();
            module_properties.insert(
                "semantic".to_string(),
                if has_semantics { "indexed" } else { "empty" }.to_string(),
            );
            module_properties.insert("source_files".to_string(), file_count.to_string());
            self.node(
                module_id.clone(),
                "MODULE",
                module_name.clone(),
                module_name,
                Some(module_path),
                Some(language),
                Some(module_parent.clone()),
                false,
                module_properties,
            );
            self.edge(
                &module_parent,
                &module_id,
                "CONTAINS",
                "tree",
                BTreeMap::new(),
                Self::evidence(&evidence_path, 1, Some("module boundary".to_string())),
            );
        }
        for (path, language) in files {
            let Some(module_id) = module_for_path.get(path).cloned() else {
                continue;
            };
            let file_id = format!("file:{path}");
            let mut file_properties = BTreeMap::new();
            file_properties.insert(
                "semantic".to_string(),
                if semantic_paths.contains(path) {
                    "indexed"
                } else {
                    "empty"
                }
                .to_string(),
            );
            self.node(
                file_id.clone(),
                "FILE",
                path.rsplit('/').next().unwrap_or(path),
                path.rsplit('/').next().unwrap_or(path),
                Some(path.clone()),
                Some(language.clone()),
                Some(module_id.clone()),
                false,
                file_properties,
            );
            self.edge(
                &module_id,
                &file_id,
                "CONTAINS",
                "tree",
                BTreeMap::new(),
                Self::evidence(path, 1, Some("source file".to_string())),
            );
            self.file_modules.insert(path.clone(), module_id);
        }
    }

    fn file_directory(path: &str) -> String {
        path.rsplit_once('/')
            .map(|(value, _)| value.to_string())
            .unwrap_or_default()
    }

    fn compact_module_path(directory: &str, is_boundary: &impl Fn(&str) -> bool) -> String {
        if directory.is_empty() || is_boundary(directory) {
            return if directory.is_empty() {
                "root".to_string()
            } else {
                directory.to_string()
            };
        }
        let mut current = directory;
        while let Some((parent, _)) = current.rsplit_once('/') {
            current = parent;
            if is_boundary(current) {
                return current.to_string();
            }
        }
        directory.to_string()
    }

    pub(crate) fn build_symbol_index(&mut self, documents: &[DocumentOutput]) {
        for document in documents {
            let Some(module_id) = self.file_modules.get(&document.path).cloned() else {
                continue;
            };
            for symbol in &document.symbols {
                if !symbol.symbol.is_empty() {
                    self.symbol_modules
                        .entry(symbol.symbol.clone())
                        .or_insert_with(|| module_id.clone());
                    self.symbol_files
                        .entry(symbol.symbol.clone())
                        .or_insert_with(|| document.path.clone());
                }
            }
        }
    }

    pub(crate) fn build_imports(&mut self, files: &[(String, String)]) {
        let mut local_prefixes = HashSet::new();
        for package in &self.packages {
            if !package.name.is_empty() {
                local_prefixes.insert(package.name.clone());
            }
        }
        for (path, source) in &self.source_texts {
            let language = language_for_path(path).unwrap_or("unknown");
            for line in source.lines() {
                let trimmed = line.trim();
                if let Some(value) = trimmed.strip_prefix("package ") {
                    local_prefixes.insert(value.trim_end_matches(';').trim().to_string());
                }
                if let Some(value) = trimmed.strip_prefix("namespace ") {
                    local_prefixes.insert(
                        value
                            .split(['{', ';', ' '])
                            .next()
                            .unwrap_or("")
                            .to_string(),
                    );
                }
                if language == "go" && trimmed.starts_with("module ") {
                    local_prefixes.insert(trimmed[7..].trim().to_string());
                }
                if language == "php" && trimmed.starts_with("namespace ") {
                    local_prefixes.insert(trimmed[10..].trim_end_matches(';').trim().to_string());
                }
            }
        }
        for (path, language) in files {
            let Some(source) = self.source_texts.get(path) else {
                continue;
            };
            for import in parse_imports(path, language, source) {
                let local_target = resolve_project_import(
                    &import,
                    &self.source_texts,
                    &self.packages,
                    &self.php_namespace_index,
                    &self.source_index,
                );
                if local_target.is_none()
                    && is_local_or_standard(
                        &import,
                        &local_prefixes,
                        &self.source_texts,
                        &self.source_index,
                    )
                {
                    continue;
                }
                self.imports.push(import);
            }
        }
    }

    pub(crate) fn emit_import_edges(&mut self) {
        // This is a one-shot emission stage. Consume the pending imports
        // instead of cloning every path/package/alias before building edges.
        for import in std::mem::take(&mut self.imports) {
            if let Some(target) = resolve_project_import(
                &import,
                &self.source_texts,
                &self.packages,
                &self.php_namespace_index,
                &self.source_index,
            ) {
                if import.path != target {
                    self.edge(
                        &format!("file:{}", import.path),
                        &format!("file:{target}"),
                        "IMPORTS",
                        "summary",
                        {
                            let mut properties = BTreeMap::from([
                                ("resolution".to_string(), "internal".to_string()),
                                ("source".to_string(), "project-import-resolver".to_string()),
                            ]);
                            if let Some(member) = import.member.as_deref() {
                                properties.insert("member".to_string(), member.to_string());
                            }
                            properties
                        },
                        Self::evidence(
                            &import.path,
                            import.line,
                            Some(format!("{} import", import.language)),
                        ),
                    );
                }
                continue;
            }
            let Some(module) = self.file_modules.get(&import.path).cloned() else {
                continue;
            };
            let package = normalize_external_package(&import.package, &import.language);
            if package.is_empty() {
                continue;
            }
            let ecosystem = ecosystem_for_language(&import.language);
            let external_id = format!("external:{ecosystem}:{package}");
            let mut properties = BTreeMap::new();
            properties.insert("package".to_string(), package.clone());
            properties.insert("ecosystem".to_string(), ecosystem.to_string());
            self.node(
                external_id.clone(),
                "EXTERNAL_LIBRARY",
                package.clone(),
                external_library_label(&package),
                None,
                Some(import.language.clone()),
                None,
                true,
                properties,
            );
            let mut edge_properties = BTreeMap::new();
            edge_properties.insert("import".to_string(), import.package.clone());
            edge_properties.insert("resolution".to_string(), "external".to_string());
            edge_properties.insert("source".to_string(), "lexical-import".to_string());
            self.edge(
                &module,
                &external_id,
                "USES_LIBRARY",
                "summary",
                edge_properties,
                Self::evidence(
                    &import.path,
                    import.line,
                    import.alias.clone().map(|alias| format!("alias:{alias}")),
                ),
            );
        }
    }

    pub(crate) fn emit_project_import_edges(&mut self, output: &IndexOutput) {
        for relation in &output.file_relations {
            if relation.kind != "IMPORTS" {
                continue;
            }
            let Some(from) = self.file_modules.get(&relation.from).cloned() else {
                continue;
            };
            let Some(to) = self.file_modules.get(&relation.to).cloned() else {
                continue;
            };
            if from == to {
                continue;
            }
            self.edge(
                &from,
                &to,
                "IMPORTS",
                "summary",
                relation.properties.clone(),
                ArchitectureEvidence {
                    path: relation.path.clone(),
                    range: relation.range.clone(),
                    note: Some("typescript-module-resolution".to_string()),
                },
            );
        }
    }

    pub(crate) fn emit_source_boundaries(&mut self) {
        let database_id = "data:database:unresolved".to_string();
        let file_id = "data:file:unresolved".to_string();
        self.node(
            database_id.clone(),
            "DATA_RESOURCE",
            "database",
            "Database schema (db_memory)",
            None,
            None,
            None,
            true,
            BTreeMap::from([(String::from("resolution"), String::from("db_memory"))]),
        );
        self.node(
            file_id.clone(),
            "DATA_RESOURCE",
            "file-storage",
            "File storage",
            None,
            None,
            None,
            true,
            BTreeMap::new(),
        );

        let mut dynamic = Vec::new();
        let mut database = Vec::new();
        let mut files = Vec::new();
        // ponytail: one lexical pass is intentional; semantic providers remain
        // authoritative for calls and types.
        for (path, source) in &self.source_texts {
            let Some(module) = self.file_modules.get(path).cloned() else {
                continue;
            };
            let language = language_for_path(&path).unwrap_or("unknown");
            for (line_number, line) in source.lines().enumerate() {
                if let Some(marker) = dynamic_call_marker(language, line) {
                    dynamic.push((
                        path.clone(),
                        module.clone(),
                        language.to_string(),
                        line_number + 1,
                        marker.to_string(),
                    ));
                }

                let operation = static_database_operation(line);
                if let Some(operation) = operation {
                    database.push(EdgeDraft {
                        from: module.clone(),
                        to: database_id.clone(),
                        kind: if operation == "READ" {
                            "READS"
                        } else {
                            "WRITES"
                        }
                        .to_string(),
                        level: "summary".to_string(),
                        properties: BTreeMap::from([
                            ("operation".to_string(), operation.to_string()),
                            ("resolution".to_string(), "source-candidate".to_string()),
                            (
                                "source".to_string(),
                                "lexical-database-boundary".to_string(),
                            ),
                        ]),
                        evidence: Self::evidence(
                            &path,
                            line_number + 1,
                            Some("static database boundary".to_string()),
                        ),
                    });
                }

                let read = contains_any(
                    line,
                    &[
                        "open(",
                        "readFile(",
                        "read_text(",
                        "read_to_string(",
                        "fs.read",
                    ],
                );
                let write = contains_any(
                    line,
                    &["writeFile(", "write_text(", "write_to_string(", "fs.write"],
                );
                let kind = if write {
                    "WRITES"
                } else if read {
                    "READS"
                } else {
                    ""
                };
                if !kind.is_empty() {
                    files.push(EdgeDraft {
                        from: module.clone(),
                        to: file_id.clone(),
                        kind: kind.to_string(),
                        level: "summary".to_string(),
                        properties: BTreeMap::from([
                            ("resolution".to_string(), "source-candidate".to_string()),
                            ("source".to_string(), "lexical-file-boundary".to_string()),
                        ]),
                        evidence: Self::evidence(
                            &path,
                            line_number + 1,
                            Some("static file boundary".to_string()),
                        ),
                    });
                }
            }
        }

        for (path, module, language, line_number, marker) in dynamic {
            let id = format!("dynamic:{language}:{path}:{line_number}");
            self.node(
                id.clone(),
                "DYNAMIC_BOUNDARY",
                "dynamic-call",
                format!("{language} 동적 호출"),
                Some(path.clone()),
                Some(language),
                Some(module.clone()),
                false,
                BTreeMap::from([
                    ("resolution".to_string(), "runtime-dependent".to_string()),
                    ("marker".to_string(), marker.clone()),
                ]),
            );
            self.edge(
                &module,
                &id,
                "DYNAMIC_CALL",
                "summary",
                BTreeMap::from([("resolution".to_string(), "runtime-dependent".to_string())]),
                Self::evidence(&path, line_number, Some(marker)),
            );
        }
        for draft in database.into_iter().chain(files) {
            self.edge(
                &draft.from,
                &draft.to,
                &draft.kind,
                &draft.level,
                draft.properties,
                draft.evidence,
            );
        }
        if !self.edges.values().any(|edge| edge.to == database_id) {
            self.nodes.remove(&database_id);
        }
        if !self.edges.values().any(|edge| edge.to == file_id) {
            self.nodes.remove(&file_id);
        }
    }

    pub(crate) fn emit_call_boundaries(&mut self, output: &IndexOutput) {
        for relation in &output.relations {
            let strategy = relation.strategy.as_deref().unwrap_or("unknown");
            let resolution = if relation.confidence.is_some_and(|value| value >= 0.85)
                && strategy.starts_with("provider-")
            {
                "provider"
            } else {
                "unknown"
            };
            let mut properties = BTreeMap::from([
                ("resolution".to_string(), resolution.to_string()),
                ("strategy".to_string(), strategy.to_string()),
            ]);
            if let Some(confidence) = relation.confidence {
                properties.insert("confidence".to_string(), confidence.to_string());
            }
            let from = self
                .symbol_modules
                .get(&relation.from)
                .cloned()
                .or_else(|| self.file_modules.get(&relation.path).cloned());
            let Some(from) = from else {
                continue;
            };
            let Some(to) = self.symbol_modules.get(&relation.to).cloned() else {
                continue;
            };
            let kind = match relation.kind.as_str() {
                "CALLS" => "CALLS",
                "IMPLEMENTATION" | "DEFINITION_OVERRIDE" => "IMPLEMENTS",
                "DEFINITION" => "DEFINES",
                "IMPORTS" => "IMPORTS",
                "USES_TYPE" => "USES_TYPE",
                _ => continue,
            };
            // Keep a file-level edge even when both files are in the same
            // compact module. The overview remains module-sized, while the
            // file graph still exposes the actual cross-file flow.
            let from_file = self.symbol_files.get(&relation.from).cloned().or_else(|| {
                self.file_modules
                    .contains_key(&relation.path)
                    .then(|| relation.path.clone())
            });
            let to_file = self.symbol_files.get(&relation.to).cloned();
            if let (Some(from_file), Some(to_file)) = (from_file, to_file) {
                if from_file != to_file {
                    self.edge(
                        &format!("file:{from_file}"),
                        &format!("file:{to_file}"),
                        kind,
                        "summary",
                        properties.clone(),
                        ArchitectureEvidence {
                            path: relation.path.clone(),
                            range: relation.range.clone(),
                            note: Some(format!("{strategy}:{}", relation.kind)),
                        },
                    );
                }
            }
            if from == to {
                continue;
            }
            self.edge(
                &from,
                &to,
                kind,
                "summary",
                properties,
                ArchitectureEvidence {
                    path: relation.path.clone(),
                    range: relation.range.clone(),
                    note: Some(format!("{strategy}:{}", relation.kind)),
                },
            );
        }
    }

    pub(crate) fn emit_framework_boundaries(&mut self, output: &IndexOutput) {
        for framework in &output.frameworks {
            for fact in &framework.facts {
                let Some(module) = self.file_modules.get(&fact.source_file).cloned() else {
                    continue;
                };
                let Some((kind, label)) = framework_fact_label(
                    fact.kind.as_str(),
                    fact.path.as_deref(),
                    fact.method.as_deref(),
                    &fact.symbol,
                ) else {
                    continue;
                };
                let target = fact
                    .symbol
                    .as_ref()
                    .and_then(|symbol| self.symbol_modules.get(symbol).cloned());
                let id = format!("entrypoint:{}:{}", framework.id, fact.id);
                let mut properties = fact.properties.clone();
                properties.insert("framework".to_string(), framework.id.clone());
                if fact.kind == "HTTP_ROUTE" {
                    if let Some(method) = &fact.method {
                        properties.insert("method".to_string(), method.clone());
                        properties.insert("routeMethod".to_string(), method.clone());
                    }
                    if let Some(path) = &fact.path {
                        properties.insert("routePath".to_string(), path.clone());
                    }
                }
                properties.insert(
                    "handler_resolution".to_string(),
                    if target.is_some() {
                        "resolved".to_string()
                    } else {
                        "unresolved".to_string()
                    },
                );
                self.node(
                    id.clone(),
                    kind,
                    label.clone(),
                    if fact.framework.is_empty() {
                        label.clone()
                    } else {
                        format!("{}: {label}", fact.framework)
                    },
                    Some(fact.source_file.clone()),
                    Some(framework.language.clone()),
                    Some(module.clone()),
                    false,
                    properties,
                );
                if let Some(target) = target {
                    self.edge(
                        &id,
                        &target,
                        "ENTRYPOINT_TO",
                        "summary",
                        BTreeMap::from([(String::from("framework"), framework.id.clone())]),
                        ArchitectureEvidence {
                            path: fact.source_file.clone(),
                            range: fact.source_range.clone(),
                            note: fact.evidence.first().cloned(),
                        },
                    );
                }
                self.entrypoints.push((id, kind.to_string(), label));
            }
        }
    }

    pub(crate) fn build_flows(&self) -> Vec<ArchitectureFlow> {
        let mut adjacency: HashMap<String, Vec<&ArchitectureEdge>> = HashMap::new();
        for edge in self.edges.values() {
            if edge.level == "summary" && edge.kind != "CONTAINS" {
                adjacency.entry(edge.from.clone()).or_default().push(edge);
            }
        }
        for edges in adjacency.values_mut() {
            edges.sort_by(|left, right| left.id.cmp(&right.id));
        }
        let mut flows = Vec::new();
        for (entrypoint, kind, label) in &self.entrypoints {
            let mut queue = VecDeque::from([entrypoint.clone()]);
            let mut reachable = BTreeSet::new();
            let mut nodes = BTreeSet::new();
            while let Some(node) = queue.pop_front() {
                if !reachable.insert(node.clone()) {
                    continue;
                }
                if nodes.len() < 50 {
                    nodes.insert(node.clone());
                }
                for edge in adjacency.get(&node).into_iter().flatten() {
                    if !reachable.contains(&edge.to) {
                        queue.push_back(edge.to.clone());
                    }
                }
            }
            if nodes.len() <= 1 {
                continue;
            }
            let edge_ids = self
                .edges
                .values()
                .filter(|edge| {
                    edge.level == "summary"
                        && edge.kind != "CONTAINS"
                        && nodes.contains(&edge.from)
                        && nodes.contains(&edge.to)
                })
                .map(|edge| edge.id.clone())
                .collect();
            let omitted_node_count = reachable.len().saturating_sub(nodes.len());
            flows.push(ArchitectureFlow {
                id: format!("flow:{entrypoint}"),
                kind: kind.clone(),
                label: label.clone(),
                entrypoint: entrypoint.clone(),
                node_ids: nodes.into_iter().collect(),
                edge_ids,
                truncated: omitted_node_count > 0,
                omitted_node_count,
            });
        }
        flows.sort_by(|left, right| left.id.cmp(&right.id));
        flows
    }

    pub(crate) fn finish(self) -> ArchitectureOutput {
        let flows = self.build_flows();
        ArchitectureOutput {
            schema: "code-memory.architecture-index.v1",
            project_root: self.root,
            nodes: self.nodes.into_values().collect(),
            edges: self.edges.into_values().collect(),
            flows,
            diagnostics: self.diagnostics,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flow_cap_reports_every_hidden_node_without_dangling_edges() {
        let mut builder = ArchitectureBuilder::new(Path::new("."), HashMap::new(), Vec::new());
        for index in 0..55 {
            builder.node(
                format!("node-{index}"),
                "MODULE",
                format!("node-{index}"),
                format!("node-{index}"),
                None,
                None,
                None,
                false,
                BTreeMap::new(),
            );
            if index > 0 {
                builder.edge(
                    &format!("node-{}", index - 1),
                    &format!("node-{index}"),
                    "CALLS",
                    "summary",
                    BTreeMap::new(),
                    ArchitectureEvidence {
                        path: "src/chain.java".to_string(),
                        range: Vec::new(),
                        note: None,
                    },
                );
            }
        }
        builder.entrypoints.push((
            "node-0".to_string(),
            "ENDPOINT".to_string(),
            "GET /chain".to_string(),
        ));

        let flow = builder.build_flows().pop().unwrap();
        assert_eq!(flow.node_ids.len(), 50);
        assert_eq!(flow.edge_ids.len(), 49);
        assert!(flow.truncated);
        assert_eq!(flow.omitted_node_count, 5);
    }
}
