use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use codebase_fact_model::analysis::{AnalysisUnit, ProgrammingLanguage};
use codebase_fact_model::analysis_plan::AnalysisPlan;
use codebase_fact_model::fact_graph::FactNodeKind;
use codebase_fact_model::identity::AnalysisUnitId;
use codebase_fact_model::language_ir::IrEndpoint;
use codebase_fact_model::source::RepositoryPath;
use codebase_fact_model::source_manifest::{SourceEntryState, SourceManifest, SourceManifestFile};
use tree_sitter::Node;

use crate::static_pipeline::language_ir::source_coordinates::SourceCoordinates;
use crate::static_pipeline::language_ir::syntax::{node_text, parse_tree};
use crate::FileRelationOutput;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StructureTarget {
    pub(super) unit_id: AnalysisUnitId,
    pub(super) kind: FactNodeKind,
    pub(super) qualified_name: String,
}

impl StructureTarget {
    pub(super) fn endpoint(&self) -> IrEndpoint {
        IrEndpoint::Structure {
            unit_id: self.unit_id.clone(),
            kind: self.kind,
            qualified_name: self.qualified_name.clone(),
        }
    }
}

pub(in crate::static_pipeline::language_ir) struct ProjectImportIndex {
    pub(super) included_files: BTreeSet<RepositoryPath>,
    pub(super) language_files: BTreeMap<ProgrammingLanguage, BTreeSet<RepositoryPath>>,
    /// Exact candidate universe for source-level path resolution. Languages
    /// that intentionally interoperate (TS/JS and C/C++) share a closed set;
    /// unrelated manifest/config files can never become import targets.
    pub(super) resolution_files: BTreeMap<ProgrammingLanguage, BTreeSet<RepositoryPath>>,
    pub(super) owners: BTreeMap<(ProgrammingLanguage, RepositoryPath), AnalysisUnitId>,
    pub(super) unit_roots: BTreeMap<AnalysisUnitId, RepositoryPath>,
    pub(super) project_model_files: BTreeSet<RepositoryPath>,
    pub(super) project_model_targets:
        BTreeMap<(ProgrammingLanguage, RepositoryPath, String), Vec<RepositoryPath>>,
    pub(super) python_modules: BTreeMap<String, Vec<RepositoryPath>>,
    pub(super) python_packages: BTreeMap<String, Vec<StructureTarget>>,
    pub(super) python_source_modules: BTreeMap<RepositoryPath, Vec<String>>,
    pub(super) java_types: BTreeMap<String, Vec<RepositoryPath>>,
    pub(super) java_packages: BTreeMap<String, Vec<StructureTarget>>,
    pub(super) csharp_types: BTreeMap<String, Vec<RepositoryPath>>,
    pub(super) csharp_namespaces: BTreeMap<String, Vec<StructureTarget>>,
    pub(super) go_packages: BTreeMap<String, Vec<StructureTarget>>,
    pub(super) rust_modules: BTreeMap<(AnalysisUnitId, String), Vec<RepositoryPath>>,
    pub(super) rust_source_modules: BTreeMap<RepositoryPath, Vec<String>>,
    pub(super) rust_crates: BTreeMap<String, Vec<RepositoryPath>>,
    pub(super) dart_package_files: BTreeMap<String, Vec<RepositoryPath>>,
    pub(super) metadata_failed_files: BTreeSet<(ProgrammingLanguage, RepositoryPath)>,
}

impl ProjectImportIndex {
    pub(in crate::static_pipeline::language_ir) fn build(
        project_root: &Path,
        manifest: &SourceManifest,
        plan: &AnalysisPlan,
        file_relations: &[FileRelationOutput],
        project_model_files: &[String],
    ) -> Result<Self, String> {
        let manifest_files = manifest
            .files
            .iter()
            .map(|file| (file.path.clone(), file))
            .collect::<BTreeMap<_, _>>();
        let included_files = manifest
            .files
            .iter()
            .filter(|file| file.state == SourceEntryState::Included)
            .map(|file| file.path.clone())
            .collect::<BTreeSet<_>>();
        let mut language_files = BTreeMap::<ProgrammingLanguage, BTreeSet<RepositoryPath>>::new();
        let mut owners = BTreeMap::new();
        for assignment in &plan.assignments {
            if assignment.unit_ids.len() != 1 {
                return Err(format!(
                    "import index requires one Analysis Plan owner for {}/{}",
                    assignment.language.as_str(),
                    assignment.path
                ));
            }
            language_files
                .entry(assignment.language)
                .or_default()
                .insert(assignment.path.clone());
            owners.insert(
                (assignment.language, assignment.path.clone()),
                assignment.unit_ids[0].clone(),
            );
        }
        let unit_roots = plan
            .units
            .iter()
            .map(|unit| (unit.id.clone(), unit.root.clone()))
            .collect::<BTreeMap<_, _>>();
        let project_model_files = project_model_files
            .iter()
            .filter_map(|path| RepositoryPath::parse(path).ok())
            .collect::<BTreeSet<_>>();
        let project_model_targets = project_model_targets(file_relations, &owners, &included_files);
        let resolution_files = resolution_files(&language_files);

        let mut index = Self {
            included_files,
            language_files,
            resolution_files,
            owners,
            unit_roots,
            project_model_files,
            project_model_targets,
            python_modules: BTreeMap::new(),
            python_packages: BTreeMap::new(),
            python_source_modules: BTreeMap::new(),
            java_types: BTreeMap::new(),
            java_packages: BTreeMap::new(),
            csharp_types: BTreeMap::new(),
            csharp_namespaces: BTreeMap::new(),
            go_packages: BTreeMap::new(),
            rust_modules: BTreeMap::new(),
            rust_source_modules: BTreeMap::new(),
            rust_crates: BTreeMap::new(),
            dart_package_files: BTreeMap::new(),
            metadata_failed_files: BTreeSet::new(),
        };
        index.build_python_index();
        index.build_java_index(project_root, &manifest_files)?;
        index.build_csharp_index(project_root, &manifest_files)?;
        index.build_go_index(project_root, plan, &manifest_files)?;
        index.build_rust_index(project_root, plan, &manifest_files)?;
        index.build_dart_index(project_root, plan, &manifest_files)?;
        index.canonicalize();
        Ok(index)
    }

    pub(super) fn owner(
        &self,
        language: ProgrammingLanguage,
        path: &RepositoryPath,
    ) -> Option<&AnalysisUnitId> {
        self.owners.get(&(language, path.clone()))
    }

    pub(in crate::static_pipeline::language_ir) fn metadata_failed(
        &self,
        language: ProgrammingLanguage,
        path: &RepositoryPath,
    ) -> bool {
        self.metadata_failed_files
            .contains(&(language, path.clone()))
    }

    fn build_python_index(&mut self) {
        let Some(files) = self.language_files.get(&ProgrammingLanguage::Python) else {
            return;
        };
        for path in files {
            let Some(unit_id) = self.owner(ProgrammingLanguage::Python, path).cloned() else {
                continue;
            };
            let Some(root) = self.unit_roots.get(&unit_id) else {
                continue;
            };
            let mut modules = python_module_names(root, path);
            modules.sort();
            modules.dedup();
            for module in &modules {
                self.python_modules
                    .entry(module.clone())
                    .or_default()
                    .push(path.clone());
                for package in module_parent_names(module, file_stem(path) == Some("__init__")) {
                    self.python_packages
                        .entry(package.clone())
                        .or_default()
                        .push(StructureTarget {
                            unit_id: unit_id.clone(),
                            kind: FactNodeKind::Package,
                            qualified_name: package,
                        });
                }
            }
            self.python_source_modules.insert(path.clone(), modules);
        }
        prefer_python_implementation_files(&mut self.python_modules);
    }

    fn build_java_index(
        &mut self,
        project_root: &Path,
        manifest_files: &BTreeMap<RepositoryPath, &SourceManifestFile>,
    ) -> Result<(), String> {
        let files = self
            .language_files
            .get(&ProgrammingLanguage::Java)
            .cloned()
            .unwrap_or_default();
        for path in files {
            let Some(manifest_file) = manifest_files.get(&path).copied() else {
                continue;
            };
            let coordinates = match SourceCoordinates::load(project_root, manifest_file) {
                Ok(value) => value,
                Err(_) => {
                    self.metadata_failed_files
                        .insert((ProgrammingLanguage::Java, path.clone()));
                    continue;
                }
            };
            let tree = match parse_tree("java", path.as_str(), coordinates.text(), "import-index") {
                Ok(value) => value,
                Err(_) => {
                    self.metadata_failed_files
                        .insert((ProgrammingLanguage::Java, path.clone()));
                    continue;
                }
            };
            let root = tree.root_node();
            let package = direct_named_child(root, "package_declaration")
                .and_then(|node| {
                    first_direct_named_child(node, &["identifier", "scoped_identifier"])
                })
                .map(|node| node_text(node, coordinates.text()).trim().to_string())
                .unwrap_or_default();
            let Some(unit_id) = self.owner(ProgrammingLanguage::Java, &path).cloned() else {
                continue;
            };
            if !package.is_empty() {
                self.java_packages
                    .entry(package.clone())
                    .or_default()
                    .push(StructureTarget {
                        unit_id,
                        kind: FactNodeKind::Namespace,
                        qualified_name: package.clone(),
                    });
            }
            let mut cursor = root.walk();
            for child in root.named_children(&mut cursor) {
                if !matches!(
                    child.kind(),
                    "class_declaration"
                        | "interface_declaration"
                        | "enum_declaration"
                        | "record_declaration"
                        | "annotation_type_declaration"
                ) {
                    continue;
                }
                let Some(name) = child.child_by_field_name("name") else {
                    continue;
                };
                let name = node_text(name, coordinates.text()).trim();
                if name.is_empty() {
                    continue;
                }
                let qualified = qualify(&package, name);
                self.java_types
                    .entry(qualified)
                    .or_default()
                    .push(path.clone());
            }
        }
        Ok(())
    }

    fn build_csharp_index(
        &mut self,
        project_root: &Path,
        manifest_files: &BTreeMap<RepositoryPath, &SourceManifestFile>,
    ) -> Result<(), String> {
        let files = self
            .language_files
            .get(&ProgrammingLanguage::CSharp)
            .cloned()
            .unwrap_or_default();
        for path in files {
            let Some(manifest_file) = manifest_files.get(&path).copied() else {
                continue;
            };
            let coordinates = match SourceCoordinates::load(project_root, manifest_file) {
                Ok(value) => value,
                Err(_) => {
                    self.metadata_failed_files
                        .insert((ProgrammingLanguage::CSharp, path.clone()));
                    continue;
                }
            };
            let tree = match parse_tree("csharp", path.as_str(), coordinates.text(), "import-index")
            {
                Ok(value) => value,
                Err(_) => {
                    self.metadata_failed_files
                        .insert((ProgrammingLanguage::CSharp, path.clone()));
                    continue;
                }
            };
            let Some(unit_id) = self.owner(ProgrammingLanguage::CSharp, &path).cloned() else {
                continue;
            };
            let file_namespace =
                direct_named_child(tree.root_node(), "file_scoped_namespace_declaration")
                    .and_then(|node| node.child_by_field_name("name"))
                    .map(|name| node_text(name, coordinates.text()).trim().to_string());
            if let Some(namespace) = file_namespace.as_deref() {
                self.push_csharp_namespace(namespace, &unit_id);
            }
            self.visit_csharp_declarations(
                tree.root_node(),
                file_namespace.as_deref().unwrap_or(""),
                &unit_id,
                &path,
                coordinates.text(),
            );
        }
        Ok(())
    }

    fn visit_csharp_declarations(
        &mut self,
        node: Node<'_>,
        namespace: &str,
        unit_id: &AnalysisUnitId,
        path: &RepositoryPath,
        source: &str,
    ) {
        if node.kind() == "file_scoped_namespace_declaration" {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.child_by_field_name("name").is_none()
                    && !matches!(
                        child.kind(),
                        "class_declaration"
                            | "interface_declaration"
                            | "struct_declaration"
                            | "record_declaration"
                            | "enum_declaration"
                    )
                {
                    continue;
                }
                self.visit_csharp_declarations(child, namespace, unit_id, path, source);
            }
            return;
        }
        if node.kind() == "namespace_declaration" {
            let Some(name) = node.child_by_field_name("name") else {
                return;
            };
            let nested = qualify(namespace, node_text(name, source).trim());
            self.push_csharp_namespace(&nested, unit_id);
            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.named_children(&mut cursor) {
                    self.visit_csharp_declarations(child, &nested, unit_id, path, source);
                }
            }
            return;
        }
        let is_type = matches!(
            node.kind(),
            "class_declaration"
                | "interface_declaration"
                | "struct_declaration"
                | "record_declaration"
                | "enum_declaration"
        );
        let child_namespace = if is_type {
            node.child_by_field_name("name")
                .map(|name| qualify(namespace, node_text(name, source).trim()))
                .unwrap_or_else(|| namespace.to_string())
        } else {
            namespace.to_string()
        };
        if is_type && !child_namespace.is_empty() {
            self.csharp_types
                .entry(child_namespace.clone())
                .or_default()
                .push(path.clone());
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit_csharp_declarations(child, &child_namespace, unit_id, path, source);
        }
    }

    fn push_csharp_namespace(&mut self, namespace: &str, unit_id: &AnalysisUnitId) {
        if namespace.is_empty() {
            return;
        }
        self.csharp_namespaces
            .entry(namespace.to_string())
            .or_default()
            .push(StructureTarget {
                unit_id: unit_id.clone(),
                kind: FactNodeKind::Namespace,
                qualified_name: namespace.to_string(),
            });
    }

    fn build_go_index(
        &mut self,
        project_root: &Path,
        plan: &AnalysisPlan,
        manifest_files: &BTreeMap<RepositoryPath, &SourceManifestFile>,
    ) -> Result<(), String> {
        for unit in plan
            .units
            .iter()
            .filter(|unit| unit.language == ProgrammingLanguage::Go)
        {
            let Some((module_root, module_name)) =
                unit_config_text(project_root, unit, manifest_files, "go.mod").and_then(
                    |(path, text)| parse_go_module(&text).map(|name| (parent_path(&path), name)),
                )
            else {
                continue;
            };
            let files = self
                .language_files
                .get(&ProgrammingLanguage::Go)
                .cloned()
                .unwrap_or_default();
            let mut package_names = BTreeSet::new();
            for path in files {
                if self.owner(ProgrammingLanguage::Go, &path) != Some(&unit.id) {
                    continue;
                }
                let Some(relative) = strip_repository_prefix(&path, &module_root) else {
                    continue;
                };
                let directory = relative
                    .rsplit_once('/')
                    .map(|(directory, _)| directory)
                    .unwrap_or("");
                let qualified = if directory.is_empty() {
                    module_name.clone()
                } else {
                    format!("{module_name}/{directory}")
                };
                package_names.insert(qualified);
            }
            for qualified_name in package_names {
                self.go_packages
                    .entry(qualified_name.clone())
                    .or_default()
                    .push(StructureTarget {
                        unit_id: unit.id.clone(),
                        kind: FactNodeKind::Package,
                        qualified_name,
                    });
            }
        }
        Ok(())
    }

    fn build_rust_index(
        &mut self,
        project_root: &Path,
        plan: &AnalysisPlan,
        manifest_files: &BTreeMap<RepositoryPath, &SourceManifestFile>,
    ) -> Result<(), String> {
        let files = self
            .language_files
            .get(&ProgrammingLanguage::Rust)
            .cloned()
            .unwrap_or_default();
        for unit in plan
            .units
            .iter()
            .filter(|unit| unit.language == ProgrammingLanguage::Rust)
        {
            for path in &files {
                if self.owner(ProgrammingLanguage::Rust, path) != Some(&unit.id) {
                    continue;
                }
                let Some(module) = rust_module_name(&unit.root, path) else {
                    continue;
                };
                self.rust_modules
                    .entry((unit.id.clone(), module.clone()))
                    .or_default()
                    .push(path.clone());
                self.rust_source_modules
                    .entry(path.clone())
                    .or_default()
                    .push(module);
            }
            if let Some((_, text)) =
                unit_config_text(project_root, unit, manifest_files, "Cargo.toml")
            {
                if let Some(crate_name) = parse_toml_package_name(&text) {
                    let library = join_repository_path(&unit.root, "src/lib.rs");
                    let main = join_repository_path(&unit.root, "src/main.rs");
                    let target = library
                        .filter(|path| self.included_files.contains(path))
                        .or_else(|| main.filter(|path| self.included_files.contains(path)));
                    if let Some(target) = target {
                        self.rust_crates
                            .entry(crate_name.replace('-', "_"))
                            .or_default()
                            .push(target);
                    }
                }
            }
        }
        Ok(())
    }

    fn build_dart_index(
        &mut self,
        project_root: &Path,
        plan: &AnalysisPlan,
        manifest_files: &BTreeMap<RepositoryPath, &SourceManifestFile>,
    ) -> Result<(), String> {
        let files = self
            .language_files
            .get(&ProgrammingLanguage::Dart)
            .cloned()
            .unwrap_or_default();
        for unit in plan
            .units
            .iter()
            .filter(|unit| unit.language == ProgrammingLanguage::Dart)
        {
            let Some((pubspec_path, text)) =
                unit_config_text(project_root, unit, manifest_files, "pubspec.yaml").or_else(
                    || unit_config_text(project_root, unit, manifest_files, "pubspec.yml"),
                )
            else {
                continue;
            };
            let Some(package_name) = parse_yaml_top_level_name(&text) else {
                continue;
            };
            let package_root = parent_path(&pubspec_path);
            let library_root =
                join_repository_path(&package_root, "lib").unwrap_or_else(RepositoryPath::root);
            for path in &files {
                if self.owner(ProgrammingLanguage::Dart, path) != Some(&unit.id) {
                    continue;
                }
                let Some(relative) = strip_repository_prefix(path, &library_root) else {
                    continue;
                };
                let uri = format!("package:{package_name}/{relative}");
                self.dart_package_files
                    .entry(uri)
                    .or_default()
                    .push(path.clone());
            }
        }
        Ok(())
    }

    fn canonicalize(&mut self) {
        canonicalize_path_map(&mut self.project_model_targets);
        canonicalize_path_map(&mut self.python_modules);
        canonicalize_structure_map(&mut self.python_packages);
        canonicalize_path_map(&mut self.java_types);
        canonicalize_structure_map(&mut self.java_packages);
        canonicalize_path_map(&mut self.csharp_types);
        canonicalize_structure_map(&mut self.csharp_namespaces);
        canonicalize_structure_map(&mut self.go_packages);
        canonicalize_path_map(&mut self.rust_modules);
        canonicalize_values(&mut self.rust_source_modules);
        canonicalize_path_map(&mut self.rust_crates);
        canonicalize_path_map(&mut self.dart_package_files);
    }
}

fn resolution_files(
    language_files: &BTreeMap<ProgrammingLanguage, BTreeSet<RepositoryPath>>,
) -> BTreeMap<ProgrammingLanguage, BTreeSet<RepositoryPath>> {
    let mut result = language_files.clone();
    let ecmascript = [
        ProgrammingLanguage::TypeScript,
        ProgrammingLanguage::JavaScript,
    ]
    .into_iter()
    .flat_map(|language| language_files.get(&language).into_iter().flatten().cloned())
    .collect::<BTreeSet<_>>();
    result.insert(ProgrammingLanguage::TypeScript, ecmascript.clone());
    result.insert(ProgrammingLanguage::JavaScript, ecmascript);

    let c_family = [ProgrammingLanguage::C, ProgrammingLanguage::Cpp]
        .into_iter()
        .flat_map(|language| language_files.get(&language).into_iter().flatten().cloned())
        .collect::<BTreeSet<_>>();
    result.insert(ProgrammingLanguage::C, c_family.clone());
    result.insert(ProgrammingLanguage::Cpp, c_family);
    result
}

fn project_model_targets(
    relations: &[FileRelationOutput],
    owners: &BTreeMap<(ProgrammingLanguage, RepositoryPath), AnalysisUnitId>,
    included_files: &BTreeSet<RepositoryPath>,
) -> BTreeMap<(ProgrammingLanguage, RepositoryPath, String), Vec<RepositoryPath>> {
    let mut targets = BTreeMap::<_, Vec<_>>::new();
    for relation in relations
        .iter()
        .filter(|relation| relation.kind == "IMPORTS")
    {
        let (Ok(from), Ok(to)) = (
            RepositoryPath::parse(&relation.from),
            RepositoryPath::parse(&relation.to),
        ) else {
            continue;
        };
        if !included_files.contains(&to) {
            continue;
        }
        let Some(specifier) = relation.properties.get("specifier") else {
            continue;
        };
        for language in [
            ProgrammingLanguage::TypeScript,
            ProgrammingLanguage::JavaScript,
        ] {
            if owners.contains_key(&(language, from.clone())) {
                targets
                    .entry((language, from.clone(), specifier.clone()))
                    .or_default()
                    .push(to.clone());
            }
        }
    }
    targets
}

fn python_module_names(root: &RepositoryPath, path: &RepositoryPath) -> Vec<String> {
    let Some(relative) = strip_repository_prefix(path, root) else {
        return Vec::new();
    };
    let Some(without_extension) = relative
        .strip_suffix(".py")
        .or_else(|| relative.strip_suffix(".pyi"))
    else {
        return Vec::new();
    };
    let without_init = without_extension
        .strip_suffix("/__init__")
        .unwrap_or(without_extension);
    let mut candidates = BTreeSet::new();
    if !without_init.is_empty() && without_init != "__init__" {
        candidates.insert(without_init.replace('/', "."));
    }
    if let Some(src_relative) = without_init.strip_prefix("src/") {
        if !src_relative.is_empty() {
            candidates.insert(src_relative.replace('/', "."));
        }
    }
    candidates.into_iter().collect()
}

fn module_parent_names(module: &str, include_self: bool) -> Vec<String> {
    let parts = module.split('.').collect::<Vec<_>>();
    let end = if include_self {
        parts.len()
    } else {
        parts.len().saturating_sub(1)
    };
    (1..=end).map(|length| parts[..length].join(".")).collect()
}

fn prefer_python_implementation_files(modules: &mut BTreeMap<String, Vec<RepositoryPath>>) {
    for paths in modules.values_mut() {
        let implementation_stems = paths
            .iter()
            .filter_map(|path| path.as_str().strip_suffix(".py").map(str::to_string))
            .collect::<BTreeSet<_>>();
        paths.retain(|path| {
            path.as_str()
                .strip_suffix(".pyi")
                .is_none_or(|stem| !implementation_stems.contains(stem))
        });
    }
}

fn rust_module_name(root: &RepositoryPath, path: &RepositoryPath) -> Option<String> {
    let relative = strip_repository_prefix(path, root)?;
    let source = relative.strip_prefix("src/")?;
    let without_extension = source.strip_suffix(".rs")?;
    if matches!(without_extension, "lib" | "main") {
        return Some("crate".to_string());
    }
    let module = without_extension
        .strip_suffix("/mod")
        .unwrap_or(without_extension)
        .replace('/', "::");
    (!module.is_empty()).then(|| format!("crate::{module}"))
}

fn unit_config_text(
    project_root: &Path,
    unit: &AnalysisUnit,
    manifest_files: &BTreeMap<RepositoryPath, &SourceManifestFile>,
    file_name: &str,
) -> Option<(RepositoryPath, String)> {
    unit.context
        .config_files
        .iter()
        .filter(|path| path.as_str().rsplit('/').next() == Some(file_name))
        .find_map(|path| {
            let manifest_file = manifest_files.get(path).copied()?;
            let coordinates = SourceCoordinates::load(project_root, manifest_file).ok()?;
            Some((path.clone(), coordinates.text().to_string()))
        })
}

fn parse_go_module(source: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let line = line.split("//").next().unwrap_or(line).trim();
        line.strip_prefix("module")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn parse_toml_package_name(source: &str) -> Option<String> {
    let mut in_package = false;
    for line in source.lines() {
        let line = line.split('#').next().unwrap_or(line).trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() == "name" {
            let name = value.trim().trim_matches(['\'', '"']);
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

fn parse_yaml_top_level_name(source: &str) -> Option<String> {
    source.lines().find_map(|line| {
        if line.starts_with(char::is_whitespace) {
            return None;
        }
        let line = line.split('#').next().unwrap_or(line);
        let (key, value) = line.split_once(':')?;
        if key.trim() != "name" {
            return None;
        }
        let name = value.trim().trim_matches(['\'', '"']);
        (!name.is_empty()).then(|| name.to_string())
    })
}

fn qualify(prefix: &str, name: &str) -> String {
    match (prefix.is_empty(), name.is_empty()) {
        (true, _) => name.to_string(),
        (_, true) => prefix.to_string(),
        _ => format!("{prefix}.{name}"),
    }
}

fn file_stem(path: &RepositoryPath) -> Option<&str> {
    path.as_str()
        .rsplit('/')
        .next()
        .and_then(|file| file.split('.').next())
}

pub(super) fn parent_path(path: &RepositoryPath) -> RepositoryPath {
    path.as_str()
        .rsplit_once('/')
        .and_then(|(parent, _)| RepositoryPath::parse(parent).ok())
        .unwrap_or_else(RepositoryPath::root)
}

pub(super) fn strip_repository_prefix<'a>(
    path: &'a RepositoryPath,
    root: &RepositoryPath,
) -> Option<&'a str> {
    if root.is_root() {
        return Some(path.as_str());
    }
    path.as_str()
        .strip_prefix(root.as_str())?
        .strip_prefix('/')
        .or_else(|| (path == root).then_some(""))
}

pub(super) fn join_repository_path(root: &RepositoryPath, suffix: &str) -> Option<RepositoryPath> {
    let value = if root.is_root() {
        suffix.to_string()
    } else if suffix.is_empty() {
        root.as_str().to_string()
    } else {
        format!("{}/{suffix}", root.as_str())
    };
    RepositoryPath::parse(value).ok()
}

fn direct_named_child<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == kind);
    found
}

fn first_direct_named_child<'tree>(node: Node<'tree>, kinds: &[&str]) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| kinds.contains(&child.kind()));
    found
}

fn canonicalize_path_map<K: Ord>(map: &mut BTreeMap<K, Vec<RepositoryPath>>) {
    for values in map.values_mut() {
        values.sort();
        values.dedup();
    }
}

fn canonicalize_values<K: Ord, V: Ord>(map: &mut BTreeMap<K, Vec<V>>) {
    for values in map.values_mut() {
        values.sort();
        values.dedup();
    }
}

fn canonicalize_structure_map<K: Ord>(map: &mut BTreeMap<K, Vec<StructureTarget>>) {
    for values in map.values_mut() {
        values.sort_by(|left, right| {
            (&left.unit_id, left.kind, left.qualified_name.as_str()).cmp(&(
                &right.unit_id,
                right.kind,
                right.qualified_name.as_str(),
            ))
        });
        values.dedup();
    }
}
