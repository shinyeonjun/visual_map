#include "code_memory/framework.h"

#include <stddef.h>
#include <string.h>

static const char *route_method(const char *callee) {
    if (!callee) {
        return NULL;
    }
    const char *name = strrchr(callee, '.');
    const char *scope = strrchr(callee, ':');
    if (scope && (!name || scope > name)) {
        name = scope;
        if (name[1] == ':') {
            name++;
        }
    }
    name = name ? name + 1 : callee;
    if (strcmp(name, "get") == 0 || strcmp(name, "Get") == 0 ||
        strcmp(name, "MapGet") == 0) {
        return "GET";
    }
    if (strcmp(name, "post") == 0 || strcmp(name, "Post") == 0 ||
        strcmp(name, "MapPost") == 0) {
        return "POST";
    }
    if (strcmp(name, "put") == 0 || strcmp(name, "Put") == 0 ||
        strcmp(name, "MapPut") == 0) {
        return "PUT";
    }
    if (strcmp(name, "delete") == 0 || strcmp(name, "Delete") == 0 ||
        strcmp(name, "MapDelete") == 0) {
        return "DELETE";
    }
    if (strcmp(name, "patch") == 0 || strcmp(name, "Patch") == 0 ||
        strcmp(name, "MapPatch") == 0) {
        return "PATCH";
    }
    if (strcmp(name, "route") == 0 || strcmp(name, "HandleFunc") == 0 ||
        strcmp(name, "handle") == 0 || strcmp(name, "Handle") == 0) {
        return "ANY";
    }
    return NULL;
}

static const char *resolve_handler(const CBMFileResult *result, const char *name) {
    if (!name || !name[0]) {
        return NULL;
    }
    for (int i = 0; i < result->defs.count; i++) {
        const CBMDefinition *definition = &result->defs.items[i];
        if (definition->name && strcmp(definition->name, name) == 0) {
            return definition->qualified_name;
        }
    }
    return name;
}

int code_memory_extract_framework_routes(const CBMFileResult *result, const char *pack_id,
                                         CodeMemoryFrameworkRoute *out, int capacity) {
    if (!result || !pack_id || !pack_id[0] || !out || capacity <= 0) {
        return 0;
    }

    int count = 0;
    for (int i = 0; i < result->defs.count && count < capacity; i++) {
        const CBMDefinition *definition = &result->defs.items[i];
        if (!definition->route_path || !definition->route_method || !definition->qualified_name) {
            continue;
        }

        out[count++] = (CodeMemoryFrameworkRoute){
            .pack_id = pack_id,
            .entry_point_kind = "HTTP_ROUTE",
            .handler_symbol = definition->qualified_name,
            .route_path = definition->route_path,
            .route_method = definition->route_method,
            .relation_kind = "HANDLES",
            .source_line = definition->start_line,
        };
    }

    for (int i = 0; i < result->calls.count && count < capacity; i++) {
        const CBMCall *call = &result->calls.items[i];
        const char *method = route_method(call->callee_name);
        if (!method || !call->first_string_arg || call->first_string_arg[0] != '/' ||
            !call->second_arg_name) {
            continue;
        }
        const char *handler = resolve_handler(result, call->second_arg_name);
        if (!handler) {
            continue;
        }
        out[count++] = (CodeMemoryFrameworkRoute){
            .pack_id = pack_id,
            .entry_point_kind = "HTTP_ROUTE",
            .handler_symbol = handler,
            .route_path = call->first_string_arg,
            .route_method = method,
            .relation_kind = "HANDLES",
            .source_line = (uint32_t)call->start_line,
        };
    }

    /* ponytail: route facts are the shared first adapter; UI/RPC/event-specific
     * facts need their own AST evidence before being added here. */
    return count;
}
