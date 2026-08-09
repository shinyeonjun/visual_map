use super::inventory::inventory_test_cases;
use crate::static_pipeline::language_ir::syntax::parse_tree;
use codebase_fact_model::analysis::ProgrammingLanguage;
use codebase_fact_model::source::SourceFileKind;

fn cases(
    language: ProgrammingLanguage,
    path: &str,
    kind: SourceFileKind,
    source: &str,
) -> Vec<super::inventory::SyntaxTestCase> {
    let tree = parse_tree(language.as_str(), path, source, "test-inventory-contract").unwrap();
    inventory_test_cases(language, path, kind, tree.root_node(), source)
}

#[test]
fn exact_runner_syntax_identifies_one_case_in_each_supported_language() {
    let fixtures = [
        (
            ProgrammingLanguage::TypeScript,
            "sample.test.ts",
            "import { test } from 'vitest';\ntest('works', () => { production(); });\n",
            "works",
        ),
        (
            ProgrammingLanguage::JavaScript,
            "sample.test.js",
            "import test from 'node:test';\ntest('works', () => { production(); });\n",
            "works",
        ),
        (
            ProgrammingLanguage::Python,
            "test_sample.py",
            "def test_works():\n    production()\n",
            "test_works",
        ),
        (
            ProgrammingLanguage::Java,
            "SampleTest.java",
            "import org.junit.jupiter.api.Test; class SampleTest { @Test void works() { production(); } }\n",
            "works",
        ),
        (
            ProgrammingLanguage::CSharp,
            "SampleTests.cs",
            "using Xunit; class SampleTests { [Fact] public void Works() { Production(); } }\n",
            "Works",
        ),
        (
            ProgrammingLanguage::C,
            "sample_test.c",
            "#include <cmocka.h>\nstatic void works(void **state) { production(); }\nint main(void) { const struct CMUnitTest tests[] = { cmocka_unit_test(works) }; }\n",
            "works",
        ),
        (
            ProgrammingLanguage::Cpp,
            "sample_test.cpp",
            "#include <gtest/gtest.h>\nTEST(Sample, Works) { production(); }\n",
            "Sample.Works",
        ),
        (
            ProgrammingLanguage::Go,
            "sample_test.go",
            "package sample\nimport \"testing\"\nfunc TestWorks(t *testing.T) { production() }\n",
            "TestWorks",
        ),
        (
            ProgrammingLanguage::Rust,
            "sample_test.rs",
            "#[test]\nfn works() { production(); }\n",
            "works",
        ),
        (
            ProgrammingLanguage::Dart,
            "sample_test.dart",
            "import 'package:test/test.dart';\nvoid main() { test('works', () { production(); }); }\n",
            "works",
        ),
    ];
    for (language, path, source, expected_name) in fixtures {
        let found = cases(language, path, SourceFileKind::Test, source);
        assert_eq!(
            found.len(),
            1,
            "{} should expose exactly one exact test case",
            language.as_str()
        );
        assert_eq!(found[0].display_name, expected_name);
    }
}

#[test]
fn names_without_runner_evidence_never_become_test_cases() {
    let fixtures = [
        (
            ProgrammingLanguage::TypeScript,
            "sample.test.ts",
            "function test(name: string, callback: () => void) { callback(); }\ntest('looks real', () => production());\n",
        ),
        (
            ProgrammingLanguage::JavaScript,
            "sample.test.js",
            "function test(name, callback) { callback(); }\ntest('looks real', () => production());\n",
        ),
        (
            ProgrammingLanguage::Python,
            "sample.py",
            "def test_looks_real():\n    production()\n",
        ),
        (
            ProgrammingLanguage::Java,
            "SampleTest.java",
            "@interface Test {} class SampleTest { @Test void looksReal() { production(); } }\n",
        ),
        (
            ProgrammingLanguage::CSharp,
            "SampleTests.cs",
            "class FactAttribute : System.Attribute {} class SampleTests { [Fact] void LooksReal() { Production(); } }\n",
        ),
        (
            ProgrammingLanguage::C,
            "sample_test.c",
            "static void looks_real(void **state) { production(); }\nint main(void) { cmocka_unit_test(looks_real); }\n",
        ),
        (
            ProgrammingLanguage::Cpp,
            "sample_test.cpp",
            "TEST(Sample, LooksReal) { production(); }\n",
        ),
        (
            ProgrammingLanguage::Go,
            "sample_test.go",
            "package sample\nfunc TestlooksReal() { production() }\n",
        ),
        (
            ProgrammingLanguage::Rust,
            "sample_test.rs",
            "fn test_looks_real() { production(); }\n",
        ),
        (
            ProgrammingLanguage::Dart,
            "sample_test.dart",
            "void test(String name, void Function() body) => body();\nvoid main() { test('looks real', () { production(); }); }\n",
        ),
    ];
    for (language, path, source) in fixtures {
        assert!(
            cases(language, path, SourceFileKind::Source, source).is_empty(),
            "{} promoted a name-only test",
            language.as_str()
        );
    }
}
