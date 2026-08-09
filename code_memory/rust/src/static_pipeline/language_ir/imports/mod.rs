//! Independent denominator for source-level module and package dependencies.
//!
//! The provider relation stream is not an import inventory. This module first
//! enumerates explicit syntax sites and only later resolves each site against
//! exact project metadata. One site may emit at most one internal relation;
//! imported member lists never multiply map edges.

mod inventory;
mod project_index;
mod resolver;

use codebase_fact_model::analysis::ProgrammingLanguage;

#[cfg(test)]
pub(super) use inventory::inventory_imports;
pub(super) use inventory::inventory_imports_from_root;
pub(super) use project_index::ProjectImportIndex;
pub(super) use resolver::ImportResolution;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ImportRelation {
    Imports,
    Exports,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ImportForm {
    EcmaScriptModule,
    EcmaScriptRequire,
    EcmaScriptDynamic,
    PythonModule,
    Java { static_import: bool, wildcard: bool },
    CSharp { static_import: bool, alias: bool },
    CInclude { system: bool, literal: bool },
    GoPackage,
    RustUse,
    RustExternCrate,
    DartUri { conditional: bool },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ImportSite {
    pub(super) language: ProgrammingLanguage,
    pub(super) relation: ImportRelation,
    pub(super) form: ImportForm,
    pub(super) specifier: String,
    /// Tree-sitter columns are UTF-8 byte columns.
    pub(super) utf8_range: Vec<i32>,
    /// Stored for exact parity with LSP diagnostics and human review tools.
    pub(super) utf16_range: Vec<i32>,
}

#[cfg(test)]
mod tests;
