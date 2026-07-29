use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy)]
pub(crate) enum ProviderKind {
    Scip,
    Lsp,
}

#[derive(Clone, Copy)]
pub(crate) struct LanguageSpec {
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) extensions: &'static [&'static str],
    pub(crate) provider: ProviderKind,
    pub(crate) tool: &'static str,
}

pub(crate) const LANGUAGES: &[LanguageSpec] = &[
    LanguageSpec {
        id: "typescript",
        name: "TypeScript",
        extensions: &["ts", "tsx", "mts", "cts"],
        provider: ProviderKind::Scip,
        tool: "scip-typescript",
    },
    LanguageSpec {
        id: "javascript",
        name: "JavaScript",
        extensions: &["js", "jsx", "mjs", "cjs"],
        provider: ProviderKind::Scip,
        tool: "scip-typescript",
    },
    LanguageSpec {
        id: "python",
        name: "Python",
        extensions: &["py", "pyi"],
        provider: ProviderKind::Lsp,
        tool: "pyright-langserver",
    },
    LanguageSpec {
        id: "java",
        name: "Java",
        extensions: &["java"],
        provider: ProviderKind::Lsp,
        tool: "jdtls",
    },
    LanguageSpec {
        id: "csharp",
        name: "C#",
        extensions: &["cs"],
        provider: ProviderKind::Scip,
        tool: "scip-dotnet",
    },
    LanguageSpec {
        id: "c",
        name: "C",
        extensions: &["c", "h", "inc"],
        provider: ProviderKind::Scip,
        tool: "scip-clang",
    },
    LanguageSpec {
        id: "cpp",
        name: "C++",
        extensions: &[
            "cc", "cp", "cpp", "cxx", "h", "hh", "hpp", "hxx", "inc", "inl", "ipp", "tpp",
        ],
        provider: ProviderKind::Scip,
        tool: "scip-clang",
    },
    LanguageSpec {
        id: "go",
        name: "Go",
        extensions: &["go"],
        provider: ProviderKind::Lsp,
        tool: "gopls",
    },
    LanguageSpec {
        id: "rust",
        name: "Rust",
        extensions: &["rs"],
        provider: ProviderKind::Lsp,
        tool: "rust-analyzer",
    },
    LanguageSpec {
        id: "php",
        name: "PHP",
        extensions: &["php"],
        provider: ProviderKind::Scip,
        tool: "scip-php",
    },
    LanguageSpec {
        id: "ruby",
        name: "Ruby",
        extensions: &["rb", "rake", "gemspec"],
        provider: ProviderKind::Lsp,
        tool: "ruby-lsp",
    },
    LanguageSpec {
        id: "dart",
        name: "Dart",
        extensions: &["dart"],
        provider: ProviderKind::Lsp,
        tool: "dart",
    },
];

#[derive(Serialize)]
pub(crate) struct IndexOutput {
    pub(crate) schema: &'static str,
    pub(crate) project_root: String,
    pub(crate) languages: Vec<LanguageOutput>,
    pub(crate) coverage: Vec<FileCoverageOutput>,
    pub(crate) documents: Vec<DocumentOutput>,
    pub(crate) relations: Vec<RelationOutput>,
    pub(crate) file_relations: Vec<FileRelationOutput>,
    pub(crate) project_model_files: Vec<String>,
    pub(crate) frameworks: Vec<crate::frameworks::FrameworkOutput>,
    pub(crate) framework_relations: Vec<crate::frameworks::FrameworkRelation>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) timings: Vec<StageTiming>,
    pub(crate) analysis_units: Vec<AnalysisUnitOutput>,
}

#[derive(Serialize)]
pub(crate) struct AnalysisUnitOutput {
    pub(crate) id: String,
    pub(crate) language: String,
    pub(crate) root: String,
    pub(crate) files_found: usize,
    pub(crate) files_indexed: usize,
    pub(crate) files_excluded: usize,
    pub(crate) files_missing: usize,
    pub(crate) status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct StageTiming {
    pub(crate) stage: &'static str,
    pub(crate) elapsed_ms: u128,
}

#[derive(Serialize)]
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

#[derive(Serialize)]
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
    pub(crate) message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) line: Option<u32>,
}

pub(crate) struct LanguageAnalysis {
    pub(crate) language: LanguageOutput,
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
