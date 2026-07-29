#ifndef CODE_MEMORY_FRAMEWORK_H
#define CODE_MEMORY_FRAMEWORK_H

#include <stdint.h>

#include "cbm.h"

typedef struct {
    const char *pack_id;
    const char *entry_point_kind;
    const char *handler_symbol;
    const char *route_path;
    const char *route_method;
    const char *relation_kind;
    uint32_t source_line;
} CodeMemoryFrameworkRoute;

/* Convert provider-independent AST route facts into Visual Map's route shape. */
int code_memory_extract_framework_routes(const CBMFileResult *result, const char *pack_id,
                                         CodeMemoryFrameworkRoute *out, int capacity);

#endif /* CODE_MEMORY_FRAMEWORK_H */
