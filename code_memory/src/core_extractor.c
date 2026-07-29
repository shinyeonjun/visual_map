#include "code_memory/extractor.h"

#include "arena.h"
#include "compat.h"
#include "constants.h"
#include "extract_unified.h"
#include "helpers.h"
#include "lang_specs.h"

#include "tree_sitter/api.h"

#include <stdatomic.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

/*
 * This is intentionally a small driver, not a copy of the upstream product
 * lifecycle. It owns only parser setup, per-file extraction, and result
 * lifetime. LSP, preprocessing, quarantine files, SQLite, MCP, and graph
 * persistence belong to later Visual Map layers.
 */

static CBM_TLS TSParser *g_parser;
static CBM_TLS CBMLanguage g_parser_language = CBM_LANG_COUNT;
static _Atomic int g_macro_extraction = 1;
static _Atomic uint64_t g_parse_ns;
static _Atomic uint64_t g_extract_ns;
static _Atomic uint64_t g_files;

static uint64_t now_ns(void) {
    struct timespec ts;
#if defined(CLOCK_MONOTONIC)
    const int clock_id = CLOCK_MONOTONIC;
#elif defined(__APPLE__)
    const int clock_id = 6;
#else
    const int clock_id = 1;
#endif
    if (cbm_clock_gettime(clock_id, &ts) != 0) {
        return 0;
    }
    return ((uint64_t)ts.tv_sec * CBM_NSEC_PER_SEC) + (uint64_t)ts.tv_nsec;
}

static TSParser *parser_for(const TSLanguage *language, CBMLanguage lang) {
    if (!g_parser) {
        g_parser = ts_parser_new();
        if (!g_parser) {
            return NULL;
        }
        g_parser_language = CBM_LANG_COUNT;
    }
    if (g_parser_language != lang) {
        if (!ts_parser_set_language(g_parser, language)) {
            return NULL;
        }
        g_parser_language = lang;
    }
    return g_parser;
}

typedef struct {
    const char *source;
    uint32_t length;
} StringInput;

static const char *read_string(void *payload, uint32_t byte, TSPoint point,
                               uint32_t *bytes_read) {
    (void)point;
    StringInput *input = (StringInput *)payload;
    if (!input || byte >= input->length) {
        *bytes_read = 0;
        return "";
    }
    *bytes_read = input->length - byte;
    return input->source + byte;
}

static bool parse_timeout(TSParseState *state) {
    const uint64_t deadline = *(const uint64_t *)state->payload;
    return now_ns() > deadline;
}

#define GROW_ARRAY(arr, arena)                                                                    \
    do {                                                                                            \
        if ((arr)->count >= (arr)->cap) {                                                          \
            int new_cap = (arr)->cap == 0 ? CBM_SZ_32 : (arr)->cap * PAIR_LEN;                    \
            void *items = cbm_arena_alloc((arena), (size_t)new_cap * sizeof(*(arr)->items));     \
            if (!items) {                                                                          \
                return;                                                                            \
            }                                                                                      \
            if ((arr)->items && (arr)->count > 0) {                                                \
                memcpy(items, (arr)->items,                                                        \
                       (size_t)(arr)->count * sizeof(*(arr)->items));                              \
            }                                                                                        \
            (arr)->items = items;                                                                  \
            (arr)->cap = new_cap;                                                                  \
        }                                                                                            \
    } while (0)

void cbm_defs_push(CBMDefArray *arr, CBMArena *arena, CBMDefinition item) {
    GROW_ARRAY(arr, arena);
    arr->items[arr->count++] = item;
}

void cbm_calls_push(CBMCallArray *arr, CBMArena *arena, CBMCall item) {
    GROW_ARRAY(arr, arena);
    arr->items[arr->count++] = item;
}

void cbm_imports_push(CBMImportArray *arr, CBMArena *arena, CBMImport item) {
    GROW_ARRAY(arr, arena);
    arr->items[arr->count++] = item;
}

void cbm_usages_push(CBMUsageArray *arr, CBMArena *arena, CBMUsage item) {
    GROW_ARRAY(arr, arena);
    arr->items[arr->count++] = item;
}

void cbm_throws_push(CBMThrowArray *arr, CBMArena *arena, CBMThrow item) {
    GROW_ARRAY(arr, arena);
    arr->items[arr->count++] = item;
}

void cbm_rw_push(CBMRWArray *arr, CBMArena *arena, CBMReadWrite item) {
    GROW_ARRAY(arr, arena);
    arr->items[arr->count++] = item;
}

void cbm_typerefs_push(CBMTypeRefArray *arr, CBMArena *arena, CBMTypeRef item) {
    GROW_ARRAY(arr, arena);
    arr->items[arr->count++] = item;
}

void cbm_envaccess_push(CBMEnvAccessArray *arr, CBMArena *arena, CBMEnvAccess item) {
    GROW_ARRAY(arr, arena);
    arr->items[arr->count++] = item;
}

void cbm_typeassign_push(CBMTypeAssignArray *arr, CBMArena *arena, CBMTypeAssign item) {
    GROW_ARRAY(arr, arena);
    arr->items[arr->count++] = item;
}

void cbm_stringref_push(CBMStringRefArray *arr, CBMArena *arena, CBMStringRef item) {
    GROW_ARRAY(arr, arena);
    arr->items[arr->count++] = item;
}

void cbm_infrabinding_push(CBMInfraBindingArray *arr, CBMArena *arena, CBMInfraBinding item) {
    GROW_ARRAY(arr, arena);
    arr->items[arr->count++] = item;
}

void cbm_impltrait_push(CBMImplTraitArray *arr, CBMArena *arena, CBMImplTrait item) {
    GROW_ARRAY(arr, arena);
    arr->items[arr->count++] = item;
}

void cbm_resolvedcall_push(CBMResolvedCallArray *arr, CBMArena *arena, CBMResolvedCall item) {
    GROW_ARRAY(arr, arena);
    arr->items[arr->count++] = item;
}

void cbm_channels_push(CBMChannelArray *arr, CBMArena *arena, CBMChannel item) {
    GROW_ARRAY(arr, arena);
    arr->items[arr->count++] = item;
}

void cbm_alloc_init(void) {}

int cbm_init(void) {
    return 0;
}

bool cbm_index_is_quarantined(const char *rel_path) {
    (void)rel_path;
    return false;
}

const char *cbm_index_quarantine_phase(const char *rel_path) {
    (void)rel_path;
    return NULL;
}

void cbm_set_macro_extraction(int enabled) {
    atomic_store_explicit(&g_macro_extraction, enabled ? 1 : 0, memory_order_relaxed);
}

int cbm_macro_extraction_enabled(void) {
    return atomic_load_explicit(&g_macro_extraction, memory_order_relaxed);
}

void cbm_get_profile(cbm_profile_out_t out) {
    *out.parse_ns = atomic_load(&g_parse_ns);
    *out.extract_ns = atomic_load(&g_extract_ns);
    *out.files = atomic_load(&g_files);
}

uint64_t cbm_get_lsp_ns(void) { return 0; }
uint64_t cbm_get_preprocess_ns(void) { return 0; }
uint64_t cbm_get_files_preprocessed(void) { return 0; }

void cbm_reset_profile(void) {
    atomic_store(&g_parse_ns, 0);
    atomic_store(&g_extract_ns, 0);
    atomic_store(&g_files, 0);
}

void cbm_reset_thread_parser(void) {
    if (g_parser) {
        ts_parser_reset(g_parser);
    }
}

void cbm_destroy_thread_parser(void) {
    if (g_parser) {
        ts_parser_delete(g_parser);
        g_parser = NULL;
        g_parser_language = CBM_LANG_COUNT;
    }
}

void cbm_shutdown(void) {
    cbm_destroy_thread_parser();
}

CBMFileResult *cbm_extract_file(const char *source, int source_len, CBMLanguage language,
                                const char *project, const char *rel_path,
                                int64_t timeout_micros, const char **extra_defines,
                                const char **include_paths) {
    (void)extra_defines;
    (void)include_paths;

    CBMFileResult *result = calloc(CBM_ALLOC_ONE, sizeof(*result));
    if (!result) {
        return NULL;
    }
    cbm_arena_init(&result->arena);

    if (!source || source_len < 0 || !project || !rel_path) {
        result->has_error = true;
        result->error_msg = cbm_arena_strdup(&result->arena, "invalid extraction input");
        return result;
    }

    const CBMLangSpec *spec = cbm_lang_spec(language);
    const TSLanguage *ts_language = cbm_ts_language(language);
    if (!spec || !ts_language) {
        result->has_error = true;
        result->error_msg = cbm_arena_strdup(&result->arena, "unsupported language");
        return result;
    }

    TSParser *parser = parser_for(ts_language, language);
    if (!parser) {
        result->has_error = true;
        result->error_msg = cbm_arena_strdup(&result->arena, "parser initialization failed");
        return result;
    }
    ts_parser_reset(parser);

    StringInput input = {source, (uint32_t)source_len};
    TSInput ts_input = {&input, read_string, TSInputEncodingUTF8, NULL};
    TSParseOptions options = {0};
    uint64_t start = now_ns();
    uint64_t deadline = 0;
    if (timeout_micros > 0) {
        deadline = start + ((uint64_t)timeout_micros * CBM_NSEC_PER_USEC);
        options.payload = &deadline;
        options.progress_callback = parse_timeout;
    }

    TSTree *tree = ts_parser_parse_with_options(parser, NULL, ts_input, options);
    uint64_t parsed = now_ns();
    if (!tree) {
        result->has_error = true;
        result->error_msg = cbm_arena_strdup(&result->arena,
                                             timeout_micros > 0 ? "parse timeout" : "parse failed");
        return result;
    }

    result->module_qn = cbm_fqn_module_source_lang(&result->arena, project, rel_path, language);
    result->is_test_file = cbm_is_test_file(rel_path, language);

    CBMExtractCtx context = {
        .arena = &result->arena,
        .result = result,
        .source = source,
        .source_len = source_len,
        .language = language,
        .project = project,
        .rel_path = rel_path,
        .module_qn = result->module_qn,
        .root = ts_tree_root_node(tree),
    };

    cbm_extract_definitions(&context);
    cbm_extract_imports(&context);
    cbm_extract_unified(&context);

    uint64_t extracted = now_ns();
    result->imports_count = result->imports.count;
    result->cached_tree = tree;
    result->cached_lang = language;

    atomic_fetch_add(&g_parse_ns, parsed - start);
    atomic_fetch_add(&g_extract_ns, extracted - parsed);
    atomic_fetch_add(&g_files, 1);
    (void)spec;
    return result;
}

void cbm_free_result(CBMFileResult *result) {
    if (!result) {
        return;
    }
    if (result->cached_tree) {
        ts_tree_delete(result->cached_tree);
    }
    cbm_arena_destroy(&result->arena);
    free(result);
}

void cbm_free_tree(CBMFileResult *result) {
    if (result && result->cached_tree) {
        ts_tree_delete(result->cached_tree);
        result->cached_tree = NULL;
    }
}

void cbm_free_tree_ptr(TSTree *tree) {
    if (tree) {
        ts_tree_delete(tree);
    }
}

int code_memory_init(void) {
    return cbm_init();
}

CBMFileResult *code_memory_extract_file(const char *source, int source_len,
                                        CBMLanguage language, const char *project,
                                        const char *relative_path,
                                        int64_t timeout_micros) {
    return cbm_extract_file(source, source_len, language, project, relative_path,
                            timeout_micros, NULL, NULL);
}

void code_memory_free_file(CBMFileResult *result) { cbm_free_result(result); }
void code_memory_reset_thread(void) { cbm_reset_thread_parser(); }
void code_memory_destroy_thread(void) { cbm_destroy_thread_parser(); }
void code_memory_shutdown(void) { cbm_shutdown(); }
