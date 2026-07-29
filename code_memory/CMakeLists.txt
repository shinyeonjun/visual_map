cmake_minimum_required(VERSION 3.20)
project(visual_map_code_memory C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_C_EXTENSIONS OFF)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

include(CTest)

set(UPSTREAM "${CMAKE_CURRENT_SOURCE_DIR}/vendor/codebase-memory")
set(CBM "${UPSTREAM}/internal/cbm")

# Grammar shims include each supported grammar's parser/scanner as one
# translation unit. Do not add parser.c files separately or static symbols
# collide. Unsupported upstream grammars are intentionally not vendored.
set(CODE_MEMORY_GRAMMAR_NAMES
    c cpp c_sharp dart go java javascript php python ruby rust typescript)
set(GRAMMAR_SHIMS)
foreach(grammar_name IN LISTS CODE_MEMORY_GRAMMAR_NAMES)
    list(APPEND GRAMMAR_SHIMS "${CBM}/grammar_${grammar_name}.c")
endforeach()
set(GRAMMAR_STUBS "${CMAKE_CURRENT_SOURCE_DIR}/src/grammar_stubs.c")

set(CODE_MEMORY_SOURCES
    src/core_extractor.c
    src/framework.c
    ${CBM}/extract_defs.c
    ${CBM}/extract_imports.c
    ${CBM}/extract_unified.c
    ${CBM}/extract_calls.c
    ${CBM}/extract_usages.c
    ${CBM}/extract_semantic.c
    ${CBM}/extract_type_refs.c
    ${CBM}/extract_type_assigns.c
    ${CBM}/extract_env_accesses.c
    ${CBM}/helpers.c
    ${CBM}/lang_specs.c
    ${CBM}/ts_runtime.c
    ${GRAMMAR_SHIMS}
    ${GRAMMAR_STUBS}
    ${UPSTREAM}/src/foundation/arena.c
    ${UPSTREAM}/src/foundation/compat.c
    ${UPSTREAM}/src/foundation/compat_fs.c
    ${UPSTREAM}/src/foundation/log.c
    ${UPSTREAM}/src/foundation/platform.c
    ${UPSTREAM}/src/discover/discover.c
    ${UPSTREAM}/src/discover/gitignore.c
    ${UPSTREAM}/src/discover/language.c
    ${UPSTREAM}/src/discover/userconfig.c
    ${UPSTREAM}/src/semantic/ast_profile.c
    ${UPSTREAM}/src/simhash/minhash.c
    ${UPSTREAM}/vendored/yyjson/yyjson.c
)

add_library(code_memory STATIC ${CODE_MEMORY_SOURCES})

find_program(CARGO_EXECUTABLE cargo)
if(CARGO_EXECUTABLE)
    add_custom_target(code_memory_language_bridge
        COMMAND "${CARGO_EXECUTABLE}" build --release --manifest-path
                "${CMAKE_CURRENT_SOURCE_DIR}/rust/Cargo.toml"
        WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
        COMMENT "Building the SCIP/native-LSP language bridge"
    )
endif()

target_include_directories(code_memory
    PUBLIC
        "${CMAKE_CURRENT_SOURCE_DIR}/include"
        "${CBM}"
        "${UPSTREAM}/src"
        "${UPSTREAM}/src/foundation"
        "${UPSTREAM}/internal/cbm/vendored/ts_runtime/include"
        "${UPSTREAM}/internal/cbm/vendored/ts_runtime/src"
        "${UPSTREAM}/vendored"
        "${UPSTREAM}"
)

if(MSVC)
    target_compile_options(code_memory PRIVATE /W4)
else()
    target_compile_options(code_memory PRIVATE -Wall -Wextra -Wno-unused-parameter)
endif()

if(WIN32)
    target_link_libraries(code_memory PUBLIC ws2_32)
else()
    target_link_libraries(code_memory PUBLIC m pthread)
endif()

if(BUILD_TESTING)
    add_executable(code_memory_extractor_smoke tests/extractor_smoke.c)
    target_link_libraries(code_memory_extractor_smoke PRIVATE code_memory)
    add_test(NAME code_memory_extractor_smoke COMMAND code_memory_extractor_smoke)
endif()
