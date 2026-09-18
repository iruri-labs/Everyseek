#ifndef EVERYTHING_CORE_H
#define EVERYTHING_CORE_H
typedef struct EMEngine EMEngine;
/* UTF-8 input, caller-owned handle, Rust-owned result strings. Thread-safe
   requests; close only after all callers release their Swift RustIndex reference. */
EMEngine *em_open(const char *database_path, char **error);
char *em_request(const EMEngine *engine, const char *json);
void em_cancel_search(const EMEngine *engine);
void em_close(EMEngine *engine);
void em_string_free(char *string);
#endif
