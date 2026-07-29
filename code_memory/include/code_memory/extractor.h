#ifndef CODE_MEMORY_EXTRACTOR_H
#define CODE_MEMORY_EXTRACTOR_H

#include <stdint.h>

#include "cbm.h"

/*
 * Temporary compatibility boundary around the extracted upstream AST engine.
 * The returned CBMFileResult remains upstream-shaped on purpose; the next
 * layer will convert it to Visual Map's canonical graph facts.
 */
int code_memory_init(void);

CBMFileResult *code_memory_extract_file(const char *source, int source_len,
                                        CBMLanguage language, const char *project,
                                        const char *relative_path,
                                        int64_t timeout_micros);

void code_memory_free_file(CBMFileResult *result);
void code_memory_reset_thread(void);
void code_memory_destroy_thread(void);
void code_memory_shutdown(void);

#endif /* CODE_MEMORY_EXTRACTOR_H */
