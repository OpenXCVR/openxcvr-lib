# Optional native GNU/Linux bridge. The base C targets never link this archive.
# Fail closed until other platforms/ABIs have their own qualified recipe.
if(CMAKE_CROSSCOMPILING OR NOT CMAKE_SYSTEM_NAME STREQUAL "Linux"
        OR NOT CMAKE_C_COMPILER_ID MATCHES "^(GNU|Clang)$")
    message(FATAL_ERROR "OPENXCVR_C_RICH requires native GNU/Linux with GCC or Clang; cross builds and other platforms are unsupported")
endif()
if(CMAKE_CONFIGURATION_TYPES)
    message(FATAL_ERROR "OPENXCVR_C_RICH requires a single-config generator and a fixed CMAKE_BUILD_TYPE")
endif()
if(NOT CMAKE_BUILD_TYPE MATCHES "^(|Debug|Release|RelWithDebInfo|MinSizeRel)$")
    message(FATAL_ERROR "OPENXCVR_C_RICH: unsupported CMAKE_BUILD_TYPE '${CMAKE_BUILD_TYPE}'")
endif()
find_program(OPENXCVR_CARGO NAMES cargo)
find_program(OPENXCVR_RUSTC NAMES rustc)
if(NOT OPENXCVR_CARGO OR NOT OPENXCVR_RUSTC)
    message(FATAL_ERROR "OPENXCVR_C_RICH requires Cargo and rustc when building from source")
endif()
execute_process(COMMAND "${OPENXCVR_RUSTC}" -vV
    OUTPUT_VARIABLE rust_version RESULT_VARIABLE rust_rc)
string(REGEX MATCH "host: ([^\n\r]+)" rust_host_line "${rust_version}")
set(OPENXCVR_RICH_RUST_TARGET "${CMAKE_MATCH_1}")
if(NOT rust_rc EQUAL 0 OR NOT OPENXCVR_RICH_RUST_TARGET MATCHES "^(x86_64|aarch64)-unknown-linux-gnu$")
    message(FATAL_ERROR "OPENXCVR_C_RICH: unsupported Rust host '${OPENXCVR_RICH_RUST_TARGET}'; requires a 64-bit native GNU/Linux ABI")
endif()
# Include flags (e.g. -m32, --target) when interrogating the actual C driver.
string(TOUPPER "${CMAKE_BUILD_TYPE}" rich_config)
separate_arguments(rich_c_flags UNIX_COMMAND "${CMAKE_C_FLAGS} ${CMAKE_C_FLAGS_${rich_config}}")
separate_arguments(rich_c_arg1 UNIX_COMMAND "${CMAKE_C_COMPILER_ARG1}")
set(rich_target_flag "")
if(CMAKE_C_COMPILER_TARGET)
    string(REGEX REPLACE "^(x86_64|aarch64)-(pc-)?linux-gnu$" "\\1-unknown-linux-gnu"
        declared_c_target "${CMAKE_C_COMPILER_TARGET}")
    if(NOT declared_c_target STREQUAL OPENXCVR_RICH_RUST_TARGET)
        message(FATAL_ERROR "OPENXCVR_C_RICH C/Rust target mismatch: declared C '${CMAKE_C_COMPILER_TARGET}', Rust '${OPENXCVR_RICH_RUST_TARGET}'; cross/mismatched targets are unsupported")
    endif()
    set(rich_target_flag "--target=${CMAKE_C_COMPILER_TARGET}")
endif()
execute_process(COMMAND "${CMAKE_C_COMPILER}" ${rich_c_arg1} ${rich_c_flags} ${rich_target_flag} -dumpmachine
    OUTPUT_VARIABLE c_target OUTPUT_STRIP_TRAILING_WHITESPACE RESULT_VARIABLE c_rc)
# GCC omits the vendor; Clang commonly reports 'pc'. Both mean this GNU ABI.
string(REGEX REPLACE "^(x86_64|aarch64)-(pc-)?linux-gnu$" "\\1-unknown-linux-gnu" c_target "${c_target}")
if(NOT c_rc EQUAL 0 OR NOT c_target STREQUAL OPENXCVR_RICH_RUST_TARGET)
    message(FATAL_ERROR "OPENXCVR_C_RICH C/Rust target mismatch: C '${c_target}', Rust '${OPENXCVR_RICH_RUST_TARGET}'; cross/mismatched targets are unsupported")
endif()
# A driver triple alone misses ABI-changing flags such as -m32, -mx32,
# -fshort-enums and -fpack-struct. Check the public repr(C) layout too.
set(rich_saved_includes "${CMAKE_REQUIRED_INCLUDES}")
set(CMAKE_REQUIRED_INCLUDES "${CMAKE_CURRENT_SOURCE_DIR}/include")
set(rich_saved_flags "${CMAKE_REQUIRED_FLAGS}")
set(CMAKE_REQUIRED_FLAGS "${CMAKE_REQUIRED_FLAGS} ${CMAKE_C_FLAGS_${rich_config}}")
unset(OPENXCVR_RICH_ABI_OK CACHE)
check_c_source_compiles("
#include <features.h>
#include <openxcvr/application_records.h>
#if !defined(__GLIBC__) || (!defined(__x86_64__) && !defined(__aarch64__))
#error unsupported C ABI
#endif
#if __BYTE_ORDER__ != __ORDER_LITTLE_ENDIAN__
#error unsupported endianness
#endif
typedef char ptr_width[sizeof(void*) == 8 ? 1 : -1];
typedef char status_width[sizeof(openxcvr_status) == 4 ? 1 : -1];
typedef char request_layout[sizeof(openxcvr_application_request) == 48 && offsetof(openxcvr_application_request, lower) == 16 ? 1 : -1];
typedef char record_layout[sizeof(openxcvr_application_record) == 48 && offsetof(openxcvr_application_record, host_meaning) == 32 ? 1 : -1];
typedef char sparse_range_layout[sizeof(openxcvr_application_range) == 6 ? 1 : -1];
typedef char supplied_range_layout[sizeof(openxcvr_application_supplied_range) == 24 && offsetof(openxcvr_application_supplied_range, bytes) == 8 ? 1 : -1];
typedef char sparse_request_layout[sizeof(openxcvr_application_sparse_request) == 32 && offsetof(openxcvr_application_sparse_request, ranges) == 16 ? 1 : -1];
typedef char footprint_request_layout[sizeof(openxcvr_application_footprint_request) == 16 ? 1 : -1];
typedef char footprint_result_layout[sizeof(openxcvr_application_footprint_result) == 32 && offsetof(openxcvr_application_footprint_result, ranges) == 24 ? 1 : -1];
typedef char result_layout[sizeof(openxcvr_application_result) == 32 && offsetof(openxcvr_application_result, records) == 24 ? 1 : -1];
int main(void) { return 0; }
" OPENXCVR_RICH_ABI_OK)
set(CMAKE_REQUIRED_INCLUDES "${rich_saved_includes}")
set(CMAKE_REQUIRED_FLAGS "${rich_saved_flags}")
if(NOT OPENXCVR_RICH_ABI_OK)
    message(FATAL_ERROR "OPENXCVR_C_RICH C/Rust ABI mismatch: requires the native 64-bit little-endian GNU ABI and default C struct/enum layout")
endif()

set(OPENXCVR_RICH_CARGO_TARGET_DIR "${CMAKE_CURRENT_BINARY_DIR}/rust-target"
    CACHE PATH "Cargo target directory for the rich-record bridge")
get_filename_component(OPENXCVR_RICH_CARGO_TARGET_DIR "${OPENXCVR_RICH_CARGO_TARGET_DIR}" ABSOLUTE)
set(rich_profile dev)
set(rich_profile_dir debug)
set(rich_opt 0)
set(rich_debug 2)
if(CMAKE_BUILD_TYPE MATCHES "^(Release|RelWithDebInfo|MinSizeRel)$")
    set(rich_profile release)
    set(rich_profile_dir release)
    set(rich_opt 3)
    set(rich_debug 0)
    if(CMAKE_BUILD_TYPE STREQUAL "RelWithDebInfo")
        set(rich_debug 2)
    elseif(CMAKE_BUILD_TYPE STREQUAL "MinSizeRel")
        set(rich_opt s)
    endif()
endif()
string(TOUPPER "${rich_profile}" rich_profile_env)
string(TOUPPER "${OPENXCVR_RICH_RUST_TARGET}" rich_target_env)
string(REPLACE "-" "_" rich_target_env "${rich_target_env}")
set(rich_output "${OPENXCVR_RICH_CARGO_TARGET_DIR}/${OPENXCVR_RICH_RUST_TARGET}/${rich_profile_dir}")
set(OPENXCVR_RICH_ARCHIVE "${rich_output}/libopenxcvr_rich_ffi.a")
set(rich_build_args --lib)
set(rich_byproducts "${OPENXCVR_RICH_ARCHIVE}")
if(BUILD_TESTING)
    list(APPEND rich_build_args --bin application-records-reference)
    list(APPEND rich_byproducts "${rich_output}/application-records-reference")
endif()
add_custom_target(openxcvr_rich_build
    COMMAND ${CMAKE_COMMAND} -E env
        "CARGO_TARGET_DIR=${OPENXCVR_RICH_CARGO_TARGET_DIR}"
        "CARGO_PROFILE_${rich_profile_env}_OPT_LEVEL=${rich_opt}"
        "CARGO_PROFILE_${rich_profile_env}_DEBUG=${rich_debug}"
        "CARGO_ENCODED_RUSTFLAGS=-Crelocation-model=pic"
        "RUSTC=${OPENXCVR_RUSTC}"
        "CARGO_TARGET_${rich_target_env}_LINKER=${CMAKE_C_COMPILER}"
        "${OPENXCVR_CARGO}" build --target "${OPENXCVR_RICH_RUST_TARGET}"
        --profile "${rich_profile}" --manifest-path "${CMAKE_CURRENT_SOURCE_DIR}/../../Cargo.toml"
        -p openxcvr-rich-ffi ${rich_build_args}
    BYPRODUCTS ${rich_byproducts}
    VERBATIM
)
add_library(openxcvr_rich_ffi STATIC IMPORTED)
set_target_properties(openxcvr_rich_ffi PROPERTIES IMPORTED_LOCATION "${OPENXCVR_RICH_ARCHIVE}")
add_dependencies(openxcvr_rich_ffi openxcvr_rich_build)

# Ask this Rust toolchain for std's native static-link dependencies, rather
# than baking one host's list into pkg-config. The bridge has only Rust deps.
file(WRITE "${CMAKE_CURRENT_BINARY_DIR}/rich-native-deps.rs" "
#[no_mangle]
pub unsafe extern \"C\" fn openxcvr_rich_abi_probe(a: *const u64, b: *mut u64) -> i32 {
    unsafe { *b = *a; }
    17
}
")
execute_process(COMMAND "${OPENXCVR_RUSTC}" --crate-name rich_native_deps
    --crate-type staticlib --target "${OPENXCVR_RICH_RUST_TARGET}" --print native-static-libs
    "${CMAKE_CURRENT_BINARY_DIR}/rich-native-deps.rs"
    -o "${CMAKE_CURRENT_BINARY_DIR}/librich_native_deps.a"
    OUTPUT_VARIABLE rich_native_stdout ERROR_VARIABLE rich_native_stderr RESULT_VARIABLE rich_native_rc)
string(REGEX MATCH "native-static-libs: ([^\n\r]+)" rich_native_line "${rich_native_stdout}${rich_native_stderr}")
set(OPENXCVR_RICH_NATIVE_LIBS "${CMAKE_MATCH_1}")
if(NOT rich_native_rc EQUAL 0 OR NOT OPENXCVR_RICH_NATIVE_LIBS)
    message(FATAL_ERROR "OPENXCVR_C_RICH cannot determine Rust native static-link dependencies: ${rich_native_stderr}")
endif()
separate_arguments(rich_native_libs UNIX_COMMAND "${OPENXCVR_RICH_NATIVE_LIBS}")
# Layout does not detect every calling-convention flag (e.g. -mabi=ms).
# Native-only qualification must also link and execute across the C/Rust ABI.
include(CheckCSourceRuns)
set(rich_saved_libraries "${CMAKE_REQUIRED_LIBRARIES}")
set(CMAKE_REQUIRED_LIBRARIES "${CMAKE_CURRENT_BINARY_DIR}/librich_native_deps.a" ${rich_native_libs})
set(CMAKE_REQUIRED_FLAGS "${rich_saved_flags} ${CMAKE_C_FLAGS_${rich_config}}")
unset(OPENXCVR_RICH_CALL_ABI_OK CACHE)
check_c_source_runs("
#include <stdint.h>
extern int32_t openxcvr_rich_abi_probe(const uint64_t *, uint64_t *);
int main(void) {
    uint64_t input = UINT64_C(0x123456789abcdef0), output = 0;
    return openxcvr_rich_abi_probe(&input, &output) == 17 && output == input ? 0 : 1;
}
" OPENXCVR_RICH_CALL_ABI_OK)
set(CMAKE_REQUIRED_LIBRARIES "${rich_saved_libraries}")
set(CMAKE_REQUIRED_FLAGS "${rich_saved_flags}")
if(NOT OPENXCVR_RICH_CALL_ABI_OK)
    message(FATAL_ERROR "OPENXCVR_C_RICH C/Rust ABI mismatch: native calling-convention/link/run probe failed")
endif()
add_library(openxcvr_c_rich STATIC src/application_records.c)
add_library(openxcvr_c_rich_shared SHARED src/application_records.c)
foreach(target openxcvr_c_rich openxcvr_c_rich_shared)
    set_target_properties(${target} PROPERTIES OUTPUT_NAME openxcvr_c_rich POSITION_INDEPENDENT_CODE ON)
    target_compile_features(${target} PUBLIC c_std_99)
    target_compile_options(${target} PRIVATE -Wall -Wextra -Werror)
    target_include_directories(${target} PUBLIC
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include> $<INSTALL_INTERFACE:include>)
endforeach()
target_link_libraries(openxcvr_c_rich PUBLIC openxcvr_rich_ffi ${rich_native_libs})
target_link_libraries(openxcvr_c_rich_shared PRIVATE openxcvr_rich_ffi ${rich_native_libs})
configure_file("${CMAKE_CURRENT_SOURCE_DIR}/openxcvr-rich.pc.in"
    "${CMAKE_CURRENT_BINARY_DIR}/openxcvr-c-rich.pc" @ONLY)
install(TARGETS openxcvr_c_rich openxcvr_c_rich_shared ARCHIVE DESTINATION lib LIBRARY DESTINATION lib)
install(FILES "${OPENXCVR_RICH_ARCHIVE}" DESTINATION lib)
install(FILES include/openxcvr/application_records.h DESTINATION include/openxcvr)
install(FILES "${CMAKE_CURRENT_BINARY_DIR}/openxcvr-c-rich.pc" DESTINATION lib/pkgconfig)

if(BUILD_TESTING)
    add_executable(openxcvr_c_application_records tests/application_records.c)
    target_link_libraries(openxcvr_c_application_records PRIVATE openxcvr_c_rich)
    target_compile_options(openxcvr_c_application_records PRIVATE -Wall -Wextra -Werror -UNDEBUG)
    add_test(NAME openxcvr_c_application_records COMMAND openxcvr_c_application_records)
    find_package(Python3 COMPONENTS Interpreter REQUIRED)
    add_test(NAME openxcvr_c_application_records_parity
        COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/application_records_parity.py
            $<TARGET_FILE:openxcvr_c_application_records> "${rich_output}/application-records-reference")
    add_test(NAME openxcvr_c_application_sparse
        COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/application_sparse.py
            $<TARGET_FILE:openxcvr_c_rich_shared> "${rich_output}/application-records-reference")
    add_test(NAME openxcvr_c_rich_configure_refusals
        COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/rich_configure_refusals.py
            "${CMAKE_COMMAND}" "${CMAKE_CURRENT_SOURCE_DIR}" "${CMAKE_CURRENT_BINARY_DIR}"
            "${CMAKE_C_COMPILER}")
    add_test(NAME openxcvr_c_installed_application_records
        COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/installed_application_records.py
            "${CMAKE_CURRENT_BINARY_DIR}" "${CMAKE_CURRENT_SOURCE_DIR}"
            "${CMAKE_C_COMPILER}" "${CMAKE_SHARED_LIBRARY_SUFFIX}")
endif()
message(STATUS "Rich bridge: ${OPENXCVR_RICH_RUST_TARGET}, profile ${rich_profile} (opt=${rich_opt}, debug=${rich_debug}), ${OPENXCVR_RICH_ARCHIVE}")
