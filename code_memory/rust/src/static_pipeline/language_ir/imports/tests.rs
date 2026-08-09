use super::*;
use codebase_fact_model::analysis::ProgrammingLanguage;

fn inventory(language: ProgrammingLanguage, path: &str, source: &str) -> Vec<ImportSite> {
    inventory_imports(language, path, source).unwrap()
}

fn values(sites: &[ImportSite]) -> Vec<(ImportRelation, ImportForm, &str)> {
    sites
        .iter()
        .map(|site| (site.relation, site.form.clone(), site.specifier.as_str()))
        .collect()
}

#[test]
fn ecmascript_inventory_covers_static_reexport_require_and_literal_dynamic_import() {
    let source = r#"
// import fake from './commented';
import type { User } from './types';
export { session } from "./session";
const helpers = require('./helpers');
async function load() { return import('./lazy'); }
const ignored = require(variable);
"#;
    assert_eq!(
        values(&inventory(
            ProgrammingLanguage::TypeScript,
            "src/main.ts",
            source
        )),
        vec![
            (
                ImportRelation::Imports,
                ImportForm::EcmaScriptModule,
                "./types"
            ),
            (
                ImportRelation::Exports,
                ImportForm::EcmaScriptModule,
                "./session"
            ),
            (
                ImportRelation::Imports,
                ImportForm::EcmaScriptRequire,
                "./helpers"
            ),
            (
                ImportRelation::Imports,
                ImportForm::EcmaScriptDynamic,
                "./lazy"
            ),
        ]
    );
}

#[test]
fn python_inventory_counts_module_specifiers_not_imported_members() {
    let source =
        "import alpha, beta.gamma as bg\nfrom .helpers import one, two\n# import ignored\n";
    assert_eq!(
        values(&inventory(
            ProgrammingLanguage::Python,
            "pkg/main.py",
            source
        )),
        vec![
            (ImportRelation::Imports, ImportForm::PythonModule, "alpha"),
            (
                ImportRelation::Imports,
                ImportForm::PythonModule,
                "beta.gamma"
            ),
            (
                ImportRelation::Imports,
                ImportForm::PythonModule,
                ".helpers"
            ),
        ]
    );
}

#[test]
fn java_and_csharp_inventory_preserve_resolution_relevant_forms() {
    let java =
        "import app.orders.Order;\nimport app.shared.*;\nimport static app.util.Keys.NAME;\n";
    assert_eq!(
        values(&inventory(ProgrammingLanguage::Java, "src/App.java", java)),
        vec![
            (
                ImportRelation::Imports,
                ImportForm::Java {
                    static_import: false,
                    wildcard: false
                },
                "app.orders.Order"
            ),
            (
                ImportRelation::Imports,
                ImportForm::Java {
                    static_import: false,
                    wildcard: true
                },
                "app.shared"
            ),
            (
                ImportRelation::Imports,
                ImportForm::Java {
                    static_import: true,
                    wildcard: false
                },
                "app.util.Keys.NAME"
            ),
        ]
    );

    let csharp =
        "global using Shop.Core;\nusing Alias = Shop.Orders.Order;\nusing static Shop.Util.Keys;\n";
    assert_eq!(
        values(&inventory(
            ProgrammingLanguage::CSharp,
            "Program.cs",
            csharp
        )),
        vec![
            (
                ImportRelation::Imports,
                ImportForm::CSharp {
                    static_import: false,
                    alias: false
                },
                "Shop.Core"
            ),
            (
                ImportRelation::Imports,
                ImportForm::CSharp {
                    static_import: false,
                    alias: true
                },
                "Shop.Orders.Order"
            ),
            (
                ImportRelation::Imports,
                ImportForm::CSharp {
                    static_import: true,
                    alias: false
                },
                "Shop.Util.Keys"
            ),
        ]
    );
}

#[test]
fn c_family_and_go_inventory_keep_literal_boundary_information() {
    let c = "#include \"local.h\"\n#include <stdio.h>\n#include HEADER_MACRO\n";
    for language in [ProgrammingLanguage::C, ProgrammingLanguage::Cpp] {
        assert_eq!(
            values(&inventory(language, "src/main.c", c)),
            vec![
                (
                    ImportRelation::Imports,
                    ImportForm::CInclude {
                        system: false,
                        literal: true
                    },
                    "local.h"
                ),
                (
                    ImportRelation::Imports,
                    ImportForm::CInclude {
                        system: true,
                        literal: true
                    },
                    "stdio.h"
                ),
                (
                    ImportRelation::Imports,
                    ImportForm::CInclude {
                        system: false,
                        literal: false
                    },
                    "HEADER_MACRO"
                ),
            ]
        );
    }

    let go = "package main\nimport (\n  \"example.com/shop/orders\"\n  alias `example.com/shop/shared`\n)\n";
    assert_eq!(
        values(&inventory(ProgrammingLanguage::Go, "main.go", go)),
        vec![
            (
                ImportRelation::Imports,
                ImportForm::GoPackage,
                "example.com/shop/orders"
            ),
            (
                ImportRelation::Imports,
                ImportForm::GoPackage,
                "example.com/shop/shared"
            ),
        ]
    );
}

#[test]
fn rust_inventory_does_not_confuse_module_declaration_with_import() {
    let source = "mod local;\nuse crate::local::Thing;\nextern crate serde as serde_crate;\n";
    assert_eq!(
        values(&inventory(ProgrammingLanguage::Rust, "src/lib.rs", source)),
        vec![
            (
                ImportRelation::Imports,
                ImportForm::RustUse,
                "crate::local::Thing"
            ),
            (
                ImportRelation::Imports,
                ImportForm::RustExternCrate,
                "serde"
            ),
        ]
    );
}

#[test]
fn dart_inventory_distinguishes_import_export_and_conditional_uri() {
    let source = "import 'src/orders.dart';\nexport 'src/public.dart';\nimport 'stub.dart' if (dart.library.io) 'io.dart';\n";
    assert_eq!(
        values(&inventory(
            ProgrammingLanguage::Dart,
            "lib/main.dart",
            source
        )),
        vec![
            (
                ImportRelation::Imports,
                ImportForm::DartUri { conditional: false },
                "src/orders.dart"
            ),
            (
                ImportRelation::Exports,
                ImportForm::DartUri { conditional: false },
                "src/public.dart"
            ),
            (
                ImportRelation::Imports,
                ImportForm::DartUri { conditional: true },
                "stub.dart"
            ),
        ]
    );
}
