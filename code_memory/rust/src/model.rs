use codebase_fact_model::analysis::{ProgrammingLanguage, ProviderExecutionContext};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy, Debug)]
pub(crate) enum ProviderKind {
    Scip,
    Lsp,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LanguageSpec {
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) extensions: &'static [&'static str],
    pub(crate) provider: ProviderKind,
    pub(crate) tool: &'static str,
    pub(crate) contract_language: ProgrammingLanguage,
}

pub(crate) const LANGUAGES: &[LanguageSpec] = &[
    LanguageSpec {
        id: "typescript",
        name: "TypeScript",
        extensions: &["ts", "tsx", "mts", "cts"],
        provider: ProviderKind::Scip,
        tool: "scip-typescript",
        contract_language: ProgrammingLanguage::TypeScript,
    },
    LanguageSpec {
        id: "javascript",
        name: "JavaScript",
        extensions: &["js", "jsx", "mjs", "cjs"],
        provider: ProviderKind::Scip,
        tool: "scip-typescript",
        contract_language: ProgrammingLanguage::JavaScript,
    },
    LanguageSpec {
        id: "python",
        name: "Python",
        extensions: &["py", "pyi"],
        provider: ProviderKind::Lsp,
        tool: "pyright-langserver",
        contract_language: ProgrammingLanguage::Python,
    },
    LanguageSpec {
        id: "java",
        name: "Java",
        extensions: &["java"],
        provider: ProviderKind::Lsp,
        tool: "jdtls",
        contract_language: ProgrammingLanguage::Java,
    },
    LanguageSpec {
        id: "csharp",
        name: "C#",
        extensions: &["cs"],
        provider: ProviderKind::Scip,
        tool: "scip-dotnet",
        contract_language: ProgrammingLanguage::CSharp,
    },
    LanguageSpec {
        id: "c",
        name: "C",
        extensions: &["c", "h", "inc"],
        provider: ProviderKind::Scip,
        tool: "scip-clang",
        contract_language: ProgrammingLanguage::C,
    },
    LanguageSpec {
        id: "cpp",
        name: "C++",
        extensions: &[
            "cc", "cp", "cpp", "cxx", "h", "hh", "hpp", "hxx", "inc", "inl", "ipp", "tpp",
        ],
        provider: ProviderKind::Scip,
        tool: "scip-clang",
        contract_language: ProgrammingLanguage::Cpp,
    },
    LanguageSpec {
        id: "go",
        name: "Go",
        extensions: &["go"],
        provider: ProviderKind::Lsp,
        tool: "gopls",
        contract_language: ProgrammingLanguage::Go,
    },
    LanguageSpec {
        id: "rust",
        name: "Rust",
        extensions: &["rs"],
        provider: ProviderKind::Lsp,
        tool: "rust-analyzer",
        contract_language: ProgrammingLanguage::Rust,
    },
    LanguageSpec {
        id: "dart",
        name: "Dart",
        extensions: &["dart"],
        provider: ProviderKind::Lsp,
        tool: "dart",
        contract_language: ProgrammingLanguage::Dart,
    },
];

#[derive(Clone, Serialize)]
pub(crate) struct ProviderProvenance {
    pub(crate) language: String,
    pub(crate) tool: String,
    pub(crate) origin: &'static str,
    pub(crate) status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) version: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct StageTiming {
    pub(crate) stage: &'static str,
    pub(crate) elapsed_ms: u128,
}

#[derive(Clone, Serialize)]
pub(crate) struct LanguageOutput {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) provider: &'static str,
    pub(crate) files_found: usize,
    pub(crate) files_indexed: usize,
    pub(crate) files_excluded: usize,
    pub(crate) files_missing: usize,
    pub(crate) status: &'static str,
}

#[derive(Clone, Serialize)]
pub(crate) struct FileCoverageOutput {
    pub(crate) language: String,
    pub(crate) path: String,
    pub(crate) status: &'static str,
    pub(crate) reason: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct DocumentOutput {
    pub(crate) language: String,
    pub(crate) path: String,
    pub(crate) symbols: Vec<SymbolOutput>,
    pub(crate) occurrences: Vec<OccurrenceOutput>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SymbolOutput {
    pub(crate) symbol: String,
    pub(crate) kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) display_name: Option<String>,
    pub(crate) documentation: Vec<String>,
    pub(crate) signature: Option<String>,
    pub(crate) enclosing_symbol: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct OccurrenceOutput {
    pub(crate) symbol: String,
    pub(crate) range: Vec<i32>,
    pub(crate) enclosing_range: Vec<i32>,
    pub(crate) definition: bool,
    pub(crate) import: bool,
    pub(crate) read: bool,
    pub(crate) write: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct RelationOutput {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) kind: String,
    pub(crate) path: String,
    pub(crate) range: Vec<i32>,
    #[serde(default)]
    pub(crate) confidence: Option<f64>,
    #[serde(default)]
    pub(crate) strategy: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct FileRelationOutput {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) kind: String,
    pub(crate) path: String,
    pub(crate) range: Vec<i32>,
    pub(crate) properties: BTreeMap<String, String>,
}

#[derive(Clone, Serialize)]
pub(crate) struct Diagnostic {
    pub(crate) language: String,
    pub(crate) level: &'static str,
    pub(crate) code: DiagnosticCode,
    pub(crate) message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) line: Option<u32>,
}

/// Stable machine-readable diagnostic categories shared by the engine, the
/// canonical pipeline and the desktop client. The human message is
/// intentionally not part of the classification contract.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum DiagnosticCode {
    ProviderMissing,
    #[serde(rename = "provider-failed")]
    ProviderFailed,
    IndexerFailed,
    InvalidOutput,
    EmptySemantic,
    #[serde(rename = "missing-dependency-metadata")]
    MissingDependencyMetadata,
    #[serde(rename = "missing-dependency")]
    DependencyMetadataGap,
    MissingCompileContext,
    MissingExternalTool,
    MissingLegacySdk,
    ProviderTimeout,
    ProviderStopped,
    PartialCoverage,
    #[serde(rename = "workspace-too-large")]
    LargeWorkspacePartial,
    JavaSourceFallback,
    JavaSourceFallbackFailed,
    TypescriptSourceFallback,
    ProviderDiagnostic,
    GeneratedCode,
    TestOnly,
    UnsupportedFramework,
    DynamicRegistration,
    StaleIndex,
    SnapshotIncompatible,
    DisplayLimit,
    Unknown,
    #[default]
    Internal,
}

/// Provider-decoded, project-relative semantic batch.
///
/// This is the primary result of a SCIP/LSP worker. Language IR consumes these
/// batches exactly once and seals their exact facts into Language IR.
pub(crate) struct ProviderUnitBatch {
    pub(crate) language: LanguageOutput,
    /// Canonical project-relative files the provider was asked to analyze.
    /// The scheduler fills this after rebasing a module-local provider result.
    pub(crate) source_files: Vec<String>,
    /// Canonical receipt for the root/config/source scope the provider really
    /// executed. Cached batches retain the original execution receipt.
    pub(crate) execution_context: ProviderExecutionContext,
    pub(crate) documents: Vec<DocumentOutput>,
    pub(crate) relations: Vec<RelationOutput>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) project_excluded_files: usize,
}

#[derive(Clone)]
pub(crate) struct SourceSnapshot {
    pub(crate) files: Vec<(String, String)>,
    pub(crate) file_hashes: HashMap<String, u64>,
    pub(crate) source_paths: Vec<std::path::PathBuf>,
}

#[derive(Default)]
pub(crate) struct DocumentCoverage {
    pub(crate) indexed: usize,
    pub(crate) excluded: usize,
    pub(crate) missing: usize,
}
