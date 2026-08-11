use super::analysis_unit_planner::plan_analysis_units;
use super::source_census::{SourceCensus, SourceCensusOptions};
use codebase_fact_model::analysis::{ContextDimensionKind, ProgrammingLanguage};
use codebase_fact_model::coverage::GapCode;
use codebase_fact_model::identity::Sha256Digest;
use codebase_fact_model::source_manifest::{SourceEncoding, SourceEntryState, SourceLinkState};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn census_is_deterministic_and_records_every_non_measured_reason() {
    let project = TestProject::new("census");
    project.write(".gitignore", b"ignored/\n*.log\n");
    project.write("src/main.rs", b"pub fn main() {}\n");
    project.write("src/binary.rs", b"pub\0fn");
    project.write("src/invalid.py", &[0xff, 0xfe]);
    project.write("tests/main_test.rs", b"#[test]\nfn works() {}\n");
    project.write("ignored/private.ts", b"export const secret = true;\n");
    project.write("node_modules/pkg/index.js", b"module.exports = {};\n");
    project.write("docs/readme.md", b"not a product input\n");
    project.write("debug.log", b"ignored log\n");
    project.write(".env", b"TOKEN=never-read\n");
    project.write("asset.bin", &[1, 2, 3]);

    let first = SourceCensus::scan(&project.root).unwrap();
    let repeated = SourceCensus::scan(&project.root).unwrap();
    assert_eq!(first.manifest, repeated.manifest);
    assert_eq!(first.included_language_files().len(), 2);

    let main = file(&first, "src/main.rs");
    assert_eq!(main.state, SourceEntryState::Included);
    assert_eq!(main.line_count, Some(1));
    assert_eq!(main.non_blank_line_count, Some(1));
    assert!(main.content_digest.is_some());

    let binary = file(&first, "src/binary.rs");
    assert_eq!(binary.state, SourceEntryState::Unsupported);
    assert_eq!(binary.gap_codes, vec![GapCode::BinarySource]);
    let invalid = file(&first, "src/invalid.py");
    assert_eq!(invalid.state, SourceEntryState::Unsupported);
    assert_eq!(invalid.gap_codes, vec![GapCode::UnsupportedEncoding]);
    let secret = file(&first, ".env");
    assert_eq!(secret.state, SourceEntryState::Excluded);
    assert!(secret.gap_codes.contains(&GapCode::SensitiveFile));
    assert!(file(&first, "debug.log")
        .gap_codes
        .contains(&GapCode::VcsIgnored));
    assert_eq!(
        file(&first, "asset.bin").gap_codes,
        vec![GapCode::UnsupportedFileType]
    );

    for scope in ["ignored", "node_modules", "docs"] {
        let receipt = first
            .manifest
            .scopes
            .iter()
            .find(|item| item.path.as_str() == scope)
            .unwrap_or_else(|| panic!("missing scope receipt for {scope}"));
        assert!(!receipt.descendants_enumerated);
        assert!(!first
            .manifest
            .files
            .iter()
            .any(|item| item.path.as_str().starts_with(&format!("{scope}/"))));
    }
    assert!(first
        .manifest
        .scopes
        .iter()
        .find(|item| item.path.as_str() == "node_modules")
        .unwrap()
        .gap_codes
        .contains(&GapCode::DependencyScopeNotEnumerated));

    project.write("src/main.rs", b"pub fn changed() {}\n");
    let changed = SourceCensus::scan(&project.root).unwrap();
    assert_ne!(
        first.manifest.manifest_digest,
        changed.manifest.manifest_digest
    );
}

#[test]
fn census_parallel_measurement_is_identical_to_serial_measurement() {
    let project = TestProject::new("census-parity");
    for index in 0..48 {
        project.write(
            &format!("src/module_{index:02}.ts"),
            format!("export const value{index} = {index};\n").as_bytes(),
        );
    }
    project.write("src/binary.rs", b"pub\0fn");
    project.write("docs/readme.md", b"excluded documentation\n");

    let serial = SourceCensus::scan_with_options(
        &project.root,
        SourceCensusOptions {
            read_buffer_bytes: 8 * 1024,
            max_entries: 1_000,
            measurement_workers: 1,
        },
    )
    .unwrap();
    let parallel = SourceCensus::scan_with_options(
        &project.root,
        SourceCensusOptions {
            read_buffer_bytes: 8 * 1024,
            max_entries: 1_000,
            measurement_workers: 8,
        },
    )
    .unwrap();

    assert_eq!(parallel.manifest, serial.manifest);
}

#[test]
fn census_separates_only_explicit_source_scopes_without_guessing_folder_names() {
    let project = TestProject::new("reference-roots");
    project.write("src/current.ts", b"export const current = true;\n");
    project.write(
        "src/legacy/compatibility.ts",
        b"export const stillCurrent = true;\n",
    );
    project.write("legacy/old.ts", b"export const archived = true;\n");

    let without_rule = SourceCensus::scan(&project.root).unwrap();
    assert!(without_rule
        .manifest
        .files
        .iter()
        .any(|file| file.path.as_str() == "src/legacy/compatibility.ts"));
    assert!(without_rule
        .manifest
        .files
        .iter()
        .any(|file| file.path.as_str() == "legacy/old.ts"));

    project.write(".codebase-workspaceignore", b"legacy/\n");
    let current = SourceCensus::scan(&project.root).unwrap();
    assert!(!current
        .manifest
        .files
        .iter()
        .any(|file| file.path.as_str() == "legacy/old.ts"));
    let excluded_scope = current
        .manifest
        .scopes
        .iter()
        .find(|scope| scope.path.as_str() == "legacy")
        .expect("explicitly excluded source scope receipt");
    assert!(excluded_scope
        .gap_codes
        .contains(&GapCode::ExplicitSourceScopeExclusion));
    assert!(!excluded_scope.descendants_enumerated);
    assert_ne!(
        without_rule.manifest.manifest_digest,
        current.manifest.manifest_digest,
    );
}

#[test]
fn a_validated_preflight_manifest_reuses_the_exact_census_identity() {
    let project = TestProject::new("census-preflight");
    project.write("src/main.ts", b"export const value = 1;\n");
    let census = SourceCensus::scan(&project.root).unwrap();
    let manifest_path = project.root.join("preflight-source-manifest.json");
    fs::write(
        &manifest_path,
        serde_json::to_vec(&census.manifest).unwrap(),
    )
    .unwrap();

    let loaded = SourceCensus::load_verified_manifest(
        &project.root,
        &manifest_path,
        &census.manifest.manifest_digest,
    )
    .unwrap();
    assert_eq!(loaded.manifest, census.manifest);
    assert!(SourceCensus::load_verified_manifest(
        &project.root,
        &manifest_path,
        &Sha256Digest::of_bytes(b"different manifest"),
    )
    .is_err());
}

#[test]
fn census_streams_large_sources_and_hashes_the_complete_content() {
    let project = TestProject::new("large-source");
    let mut source = String::from("export const marker = '한글';\n\n");
    while source.len() <= 1_100_000 {
        source.push_str("// bounded streaming source measurement\n");
    }
    project.write("src/large.ts", source.as_bytes());
    let census = SourceCensus::scan_with_options(
        &project.root,
        SourceCensusOptions {
            // Deliberately split UTF-8 characters and line endings across
            // reads; the output must equal whole-buffer semantics.
            read_buffer_bytes: 7,
            max_entries: 100,
            measurement_workers: 4,
        },
    )
    .unwrap();
    let large = file(&census, "src/large.ts");
    assert_eq!(large.state, SourceEntryState::Included);
    assert_eq!(large.byte_size, source.len() as u64);
    assert_eq!(
        large.content_digest,
        Some(Sha256Digest::of_bytes(source.as_bytes()))
    );
    assert_eq!(large.line_count, Some(source.lines().count() as u64));
    assert_eq!(
        large.non_blank_line_count,
        Some(
            source
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count() as u64
        )
    );
    assert_eq!(large.encoding, SourceEncoding::Utf8);
    assert!(large.gap_codes.is_empty());
}

#[test]
fn census_counts_a_giant_single_line_without_a_line_sized_buffer() {
    let project = TestProject::new("giant-line");
    let source = vec![b'x'; 1_100_001];
    project.write("src/huge.rs", &source);
    let census = SourceCensus::scan_with_options(
        &project.root,
        SourceCensusOptions {
            read_buffer_bytes: 4 * 1024,
            max_entries: 100,
            measurement_workers: 4,
        },
    )
    .unwrap();
    let large = file(&census, "src/huge.rs");

    assert_eq!(large.state, SourceEntryState::Included);
    assert_eq!(large.line_count, Some(1));
    assert_eq!(large.non_blank_line_count, Some(1));
    assert_eq!(large.content_digest, Some(Sha256Digest::of_bytes(&source)));
}

#[test]
fn census_never_follows_a_symlink_outside_the_project_root() {
    let project = TestProject::new("symlink-root");
    let outside = TestProject::new("symlink-outside");
    outside.write("secret.rs", b"pub const SECRET: &str = \"outside\";\n");
    let link = project.root.join("escaped.rs");
    if !create_file_symlink(&outside.root.join("secret.rs"), &link) {
        return;
    }
    let census = SourceCensus::scan(&project.root).unwrap();
    let escaped = file(&census, "escaped.rs");
    assert_eq!(escaped.state, SourceEntryState::Excluded);
    assert_eq!(escaped.link_state, SourceLinkState::SymlinkEscapesRoot);
    assert!(escaped.gap_codes.contains(&GapCode::SymlinkEscapesRoot));
    assert!(escaped.content_digest.is_none());
}

#[test]
fn census_keeps_committed_build_and_generated_source_contexts() {
    let project = TestProject::new("committed-build-context");
    project.write(
        "src/Package/build/net10.0/Package.targets",
        b"<Project />\n",
    );
    project.write(
        "src/Package/generated/GeneratedModel.cs",
        b"public sealed class GeneratedModel {}\n",
    );
    project.write("rules/Strict.ruleset", b"<RuleSet />\n");

    let census = SourceCensus::scan(&project.root).unwrap();
    for expected in [
        "src/Package/build/net10.0/Package.targets",
        "src/Package/generated/GeneratedModel.cs",
        "rules/Strict.ruleset",
    ] {
        assert_eq!(
            file(&census, expected).state,
            SourceEntryState::Included,
            "committed semantic input must not be hidden by its directory name: {expected}"
        );
    }
}

#[test]
fn planner_assigns_all_ten_languages_and_uses_csharp_and_go_markers() {
    let project = TestProject::new("planner-10");
    for (path, body) in [
        (
            "ts/tsconfig.json",
            r#"{"compilerOptions":{"module":"ESNext","target":"ES2022"}}"#,
        ),
        ("ts/src/app.ts", "export const app = 1;"),
        (
            "js/jsconfig.json",
            r#"{"compilerOptions":{"module":"CommonJS","target":"ES2020"}}"#,
        ),
        ("js/src/app.js", "exports.app = 1;"),
        ("python/pyproject.toml", "[project]\nname='sample'"),
        ("python/app.py", "def app(): pass"),
        ("java/pom.xml", "<project />"),
        ("java/src/Main.java", "class Main {}"),
        (
            "dotnet/App.csproj",
            "<Project><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>",
        ),
        ("dotnet/Program.cs", "class Program {}"),
        ("native/compile_flags.txt", "-std=c++20"),
        ("native/main.c", "int main(void) { return 0; }"),
        ("native/main.cpp", "int main() { return 0; }"),
        ("native/common.h", "#pragma once"),
        ("go/go.mod", "module example.test/app\ngo 1.22"),
        ("go/main.go", "package main"),
        (
            "rust/Cargo.toml",
            "[package]\nname='sample'\nversion='0.1.0'\nedition='2021'",
        ),
        ("rust/src/lib.rs", "pub fn run() {}"),
        ("dart/pubspec.yaml", "name: sample"),
        ("dart/lib/main.dart", "void main() {}"),
    ] {
        project.write(path, body.as_bytes());
    }
    let census = SourceCensus::scan(&project.root).unwrap();
    assert_eq!(
        file(&census, "native/compile_flags.txt").file_kind,
        codebase_fact_model::source::SourceFileKind::Build
    );
    assert_eq!(
        file(&census, "native/compile_flags.txt").state,
        SourceEntryState::Included
    );
    let first = plan_analysis_units(&project.root, &census.manifest).unwrap();
    let repeated = plan_analysis_units(&project.root, &census.manifest).unwrap();
    assert_eq!(first, repeated);
    let languages = first
        .units
        .iter()
        .map(|unit| unit.language)
        .collect::<BTreeSet<_>>();
    assert_eq!(languages.len(), 10);
    assert_eq!(first.assignments.len(), 12);
    assert_eq!(unit_root(&first, ProgrammingLanguage::CSharp), "dotnet");
    assert_eq!(unit_root(&first, ProgrammingLanguage::Go), "go");
    assert_eq!(
        unit_dimension(
            &first,
            ProgrammingLanguage::TypeScript,
            ContextDimensionKind::Target,
        ),
        Some("es2022")
    );
    assert_eq!(
        unit_dimension(
            &first,
            ProgrammingLanguage::CSharp,
            ContextDimensionKind::TargetFramework,
        ),
        Some("net8.0")
    );
    assert_eq!(
        unit_dimension(
            &first,
            ProgrammingLanguage::Go,
            ContextDimensionKind::LanguageVersion,
        ),
        Some("1.22")
    );
    assert_eq!(
        unit_dimension(
            &first,
            ProgrammingLanguage::Rust,
            ContextDimensionKind::LanguageVersion,
        ),
        Some("edition-2021")
    );
    assert!(first.assignments.iter().any(|assignment| {
        assignment.path.as_str() == "native/common.h"
            && assignment.language == ProgrammingLanguage::C
    }));
    assert!(first.assignments.iter().any(|assignment| {
        assignment.path.as_str() == "native/common.h"
            && assignment.language == ProgrammingLanguage::Cpp
    }));

    project.write("go/go.mod", b"module example.test/changed\ngo 1.23");
    let changed_census = SourceCensus::scan(&project.root).unwrap();
    let changed = plan_analysis_units(&project.root, &changed_census.manifest).unwrap();
    assert_ne!(first.config_digest, changed.config_digest);
}

#[test]
fn csharp_root_solution_is_the_execution_authority_for_all_projects() {
    let project = TestProject::new("csharp-root-solution");
    for (path, body) in [
        (
            "Workspace.slnx",
            "<Solution><Project Path=\"src/App/App.csproj\" /><Project Path=\"test/App.Tests/App.Tests.csproj\" /></Solution>",
        ),
        ("global.json", r#"{"sdk":{"version":"10.0.100"}}"#),
        ("Directory.Build.props", "<Project />"),
        ("src/App/App.csproj", "<Project />"),
        ("src/App/Program.cs", "class Program {}"),
        ("test/Directory.Build.props", "<Project />"),
        ("test/App.Tests/App.Tests.csproj", "<Project />"),
        ("test/App.Tests/ProgramTests.cs", "class ProgramTests {}"),
    ] {
        project.write(path, body.as_bytes());
    }

    let census = SourceCensus::scan(&project.root).unwrap();
    let plan = plan_analysis_units(&project.root, &census.manifest).unwrap();
    let csharp = plan
        .units
        .iter()
        .filter(|unit| unit.language == ProgrammingLanguage::CSharp)
        .collect::<Vec<_>>();

    assert_eq!(csharp.len(), 1);
    assert_eq!(csharp[0].root.as_str(), ".");
    for expected in [
        "Workspace.slnx",
        "global.json",
        "Directory.Build.props",
        "src/App/App.csproj",
        "test/Directory.Build.props",
        "test/App.Tests/App.Tests.csproj",
    ] {
        assert!(
            csharp[0]
                .context
                .config_files
                .iter()
                .any(|path| path.as_str() == expected),
            "missing C# execution context {expected}"
        );
    }
    assert_eq!(
        plan.assignments
            .iter()
            .filter(|assignment| assignment.language == ProgrammingLanguage::CSharp)
            .count(),
        2
    );
}

#[test]
fn planner_marks_missing_project_metadata_instead_of_guessing() {
    let project = TestProject::new("planner-gap");
    project.write("src/app.py", b"def app(): pass\n");
    let census = SourceCensus::scan(&project.root).unwrap();
    let plan = plan_analysis_units(&project.root, &census.manifest).unwrap();
    assert_eq!(plan.assignments.len(), 1);
    assert_eq!(plan.gaps.len(), 1);
    assert_eq!(plan.gaps[0].code, GapCode::MissingProjectMetadata);
}

#[test]
fn dart_nested_analysis_options_create_an_exact_execution_boundary() {
    let project = TestProject::new("dart-analysis-options-boundary");
    project.write("pubspec.yaml", b"name: sample\n");
    project.write("lib/root.dart", b"void root() {}\n");
    project.write(
        "test_data/experiments/nnbd/analysis_options.yaml",
        b"analyzer:\n  enable-experiment:\n    - non-nullable\n",
    );
    project.write(
        "test_data/experiments/nnbd/case.dart",
        b"void experiment() {}\n",
    );

    let census = SourceCensus::scan(&project.root).unwrap();
    let plan = plan_analysis_units(&project.root, &census.manifest).unwrap();
    let dart_roots = plan
        .units
        .iter()
        .filter(|unit| unit.language == ProgrammingLanguage::Dart)
        .map(|unit| unit.root.as_str())
        .collect::<BTreeSet<_>>();

    assert_eq!(
        dart_roots,
        BTreeSet::from([".", "test_data/experiments/nnbd"])
    );
    let nested = plan
        .units
        .iter()
        .find(|unit| unit.root.as_str() == "test_data/experiments/nnbd")
        .unwrap();
    assert!(nested
        .context
        .config_files
        .iter()
        .any(|path| path.as_str().ends_with("analysis_options.yaml")));
}

fn file<'a>(
    census: &'a SourceCensus,
    path: &str,
) -> &'a codebase_fact_model::source_manifest::SourceManifestFile {
    census
        .manifest
        .files
        .iter()
        .find(|item| item.path.as_str() == path)
        .unwrap_or_else(|| panic!("missing manifest file {path}"))
}

fn unit_root(
    plan: &codebase_fact_model::analysis_plan::AnalysisPlan,
    language: ProgrammingLanguage,
) -> &str {
    plan.units
        .iter()
        .find(|unit| unit.language == language)
        .unwrap()
        .root
        .as_str()
}

fn unit_dimension(
    plan: &codebase_fact_model::analysis_plan::AnalysisPlan,
    language: ProgrammingLanguage,
    kind: ContextDimensionKind,
) -> Option<&str> {
    plan.units
        .iter()
        .find(|unit| unit.language == language)?
        .context
        .dimensions
        .iter()
        .find(|dimension| dimension.kind == kind)
        .map(|dimension| dimension.value.as_str())
}

struct TestProject {
    root: PathBuf,
}

impl TestProject {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "codebase-workspace-static-pipeline-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn write(&self, relative: &str, bytes: &[u8]) {
        let path = self
            .root
            .join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, bytes).unwrap();
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[cfg(unix)]
fn create_file_symlink(target: &Path, link: &Path) -> bool {
    std::os::unix::fs::symlink(target, link).is_ok()
}

#[cfg(windows)]
fn create_file_symlink(target: &Path, link: &Path) -> bool {
    std::os::windows::fs::symlink_file(target, link).is_ok()
}
