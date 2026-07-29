#include "code_memory/extractor.h"
#include "code_memory/framework.h"

#include <assert.h>
#include <stddef.h>
#include <string.h>

static void assert_has_definition(const char *source, CBMLanguage language,
                                  const char *path) {
    CBMFileResult *result = code_memory_extract_file(source, (int)strlen(source), language,
                                                     "smoke", path, 1000000);
    assert(result != NULL);
    assert(!result->has_error);
    assert(result->defs.count > 0);
    code_memory_free_file(result);
}

static void assert_has_import(const char *source, CBMLanguage language,
                              const char *path) {
    CBMFileResult *result = code_memory_extract_file(source, (int)strlen(source), language,
                                                     "smoke", path, 1000000);
    assert(result != NULL);
    assert(!result->has_error);
    assert(result->imports.count > 0);
    code_memory_free_file(result);
}

static void assert_has_call(const char *source, CBMLanguage language,
                            const char *path) {
    CBMFileResult *result = code_memory_extract_file(source, (int)strlen(source), language,
                                                     "smoke", path, 1000000);
    assert(result != NULL);
    assert(!result->has_error);
    assert(result->calls.count > 0);
    code_memory_free_file(result);
}

static void assert_has_framework_route(const char *source, CBMLanguage language,
                                       const char *pack_id, const char *path,
                                       const char *method) {
    CBMFileResult *result = code_memory_extract_file(source, (int)strlen(source), language,
                                                     "smoke", "src/routes.ts", 1000000);
    assert(result != NULL);
    assert(!result->has_error);
    CodeMemoryFrameworkRoute route = {0};
    assert(code_memory_extract_framework_routes(result, pack_id, &route, 1) == 1);
    assert(strcmp(route.entry_point_kind, "HTTP_ROUTE") == 0);
    assert(strcmp(route.relation_kind, "HANDLES") == 0);
    assert(strcmp(route.route_path, path) == 0);
    assert(strcmp(route.route_method, method) == 0);
    assert(route.handler_symbol != NULL);
    code_memory_free_file(result);
}

int main(void) {
    assert(code_memory_init() == 0);
    assert_has_definition("export function add(a: number): number { return a + 1; }",
                          CBM_LANG_TYPESCRIPT, "src/math.ts");
    assert_has_definition("function add(a) { return a + 1; }", CBM_LANG_JAVASCRIPT,
                          "src/math.js");
    assert_has_definition("int add(int a, int b) { return a + b; }", CBM_LANG_C,
                          "src/math.c");
    assert_has_definition("int add(int a, int b) { return a + b; }", CBM_LANG_CPP,
                          "src/math.cpp");
    assert_has_definition("def add(a, b):\n    return a + b\n", CBM_LANG_PYTHON,
                          "src/math.py");
    assert_has_definition("class App { int add(int a) { return a + 1; } }", CBM_LANG_JAVA,
                          "src/App.java");
    assert_has_definition("class App { int Add(int a) { return a + 1; } }", CBM_LANG_CSHARP,
                          "src/App.cs");
    assert_has_definition("package main\nfunc Add(a int) int { return a + 1 }", CBM_LANG_GO,
                          "src/math.go");
    assert_has_definition("fn add(a: i32) -> i32 { a + 1 }", CBM_LANG_RUST,
                          "src/math.rs");
    assert_has_definition("<?php function add($a) { return $a + 1; } ?>", CBM_LANG_PHP,
                          "src/math.php");
    assert_has_definition("def add(a)\n  a + 1\nend", CBM_LANG_RUBY,
                          "src/math.rb");
    assert_has_definition("int add(int a) => a + 1;", CBM_LANG_DART,
                          "src/math.dart");
    assert_has_import("import os\n", CBM_LANG_PYTHON, "src/imports.py");
    assert_has_call("int add() { return 1; }\nint main() { return add(); }\n", CBM_LANG_C,
                    "src/main.c");
    assert_has_framework_route("class Routes { @GetMapping(\"/health\") void health() {} }\n",
                              CBM_LANG_JAVA, "java/spring-mvc", "/health", "GET");
    assert_has_framework_route("function health(req, res) {}\napp.get(\"/health\", health);\n",
                              CBM_LANG_TYPESCRIPT, "typescript/express", "/health", "GET");
    code_memory_shutdown();
    return 0;
}
