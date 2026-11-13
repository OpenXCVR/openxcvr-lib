cmake_minimum_required(VERSION 3.16)

project(openxcvr_c VERSION 0.2.0 LANGUAGES C)

# Keep portable ISO C stdio and its error semantics, not Microsoft's _s APIs.
# This disables CRT-only deprecation annotations, not compiler warnings, and
# stays private to this directory rather than propagating to SDK consumers.
if(CMAKE_C_COMPILER_ID STREQUAL "MSVC" OR CMAKE_C_SIMULATE_ID STREQUAL "MSVC")
    add_compile_definitions(_CRT_SECURE_NO_WARNINGS)
endif()

# Internal behaviour tables: pure-data tables kmod
# may vendor; they are NOT compiled into the library targets (no new
# public ABI), they get their own compile gate.
file(GLOB OPENXCVR_C_FSM_GENERATED_SOURCES CONFIGURE_DEPENDS
    generated/fsm/*.c
)

file(GLOB OPENXCVR_C_GENERATED_SOURCES CONFIGURE_DEPENDS
    generated/pages/*.c
)

add_library(openxcvr_c STATIC
    src/openxcvr.c
    ${OPENXCVR_C_GENERATED_SOURCES}
)

# Consumers (the userland tools) link this archive into shared objects.
set_target_properties(openxcvr_c PROPERTIES POSITION_INDEPENDENT_CODE ON)

add_library(openxcvr_c_shared SHARED
    src/openxcvr.c
    ${OPENXCVR_C_GENERATED_SOURCES}
)

foreach(target openxcvr_c openxcvr_c_shared)
    target_compile_features(${target} PUBLIC c_std_99)
    target_include_directories(${target}
        PUBLIC
            $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
            $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/generated>
            $<INSTALL_INTERFACE:include>
    )

    set_target_properties(${target} PROPERTIES
        OUTPUT_NAME openxcvr_c
        C_STANDARD 99
        C_STANDARD_REQUIRED YES
        C_EXTENSIONS NO
    )

endforeach()

# The Windows import library keeps openxcvr_c.lib; the static archive
# already ships as openxcvr_c_static.lib. Distinct outputs also work in Ninja.
if(WIN32)
    set_target_properties(openxcvr_c PROPERTIES OUTPUT_NAME openxcvr_c_static)
endif()

# F16 conversion calls fma(3); optical power conversion calls log10(3).
# Whether these need libm to link is platform-specific (some toolchains resolve it
# from libc alone, others - including glibc here - need -lm), so probe:
# first without extra libraries, then with libm; if neither links, the
# platform cannot build the library. The probe keeps volatile operands
# so constant folding cannot hide a link requirement.
include(CheckCSourceCompiles)
set(OPENXCVR_C_MATH_PROBE "
#include <math.h>
int main(int argc, char **argv) { volatile double a = (double)argc, b = 2.0, c = 3.0; (void)argv; return (int)(fma(a, b, c) + log10(a)); }
")
set(OPENXCVR_C_MATH_LIBS "")
check_c_source_compiles("${OPENXCVR_C_MATH_PROBE}" OPENXCVR_C_FMA_LOG10_IN_LIBC)
if(NOT OPENXCVR_C_FMA_LOG10_IN_LIBC)
    # Save and restore the caller's (or a parent script's) check
    # variables: an unset() here would erase a value an embedding set
    # before including this file.
    set(OPENXCVR_C_SAVED_REQUIRED_LIBRARIES "${CMAKE_REQUIRED_LIBRARIES}")
    set(CMAKE_REQUIRED_LIBRARIES m)
    check_c_source_compiles("${OPENXCVR_C_MATH_PROBE}" OPENXCVR_C_FMA_LOG10_IN_LIBM)
    set(CMAKE_REQUIRED_LIBRARIES "${OPENXCVR_C_SAVED_REQUIRED_LIBRARIES}")
    if(NOT OPENXCVR_C_FMA_LOG10_IN_LIBM)
        message(FATAL_ERROR
            "openxcvr_c requires fma(3) and log10(3): they link in neither libc nor libm on this platform")
    endif()
    set(OPENXCVR_C_MATH_LIBS "m")
endif()
# The observation objects enter the final targets through
# $<TARGET_OBJECTS:openxcvr_catalog_obs>, so the dependency goes on the
# final targets, not the object library: PUBLIC on the static archive's
# usage requirements, PRIVATE (DT_NEEDED) on the shared library.
target_link_libraries(openxcvr_c PUBLIC ${OPENXCVR_C_MATH_LIBS})
target_link_libraries(openxcvr_c_shared PRIVATE ${OPENXCVR_C_MATH_LIBS})

# pkg-config: Libs.private names the detected math library so
# `pkg-config --static --libs` closes the archive's fma reference;
# Libs stays -L/-lopenxcvr_c, so dynamic consumers never repeat -lm.
# Set before every configure_file of openxcvr.pc.in (the installed .pc
# and the installed-consumer tests' prefix-named copies).
if(OPENXCVR_C_MATH_LIBS STREQUAL "m")
    set(OPENXCVR_C_LIBS_PRIVATE "-lm")
else()
    set(OPENXCVR_C_LIBS_PRIVATE "")
endif()

include(CTest)
if(BUILD_TESTING)
    find_package(Python3 COMPONENTS Interpreter REQUIRED)
    add_executable(openxcvr_c_parity tests/parity.c)
    target_link_libraries(openxcvr_c_parity PRIVATE openxcvr_c)
    # Forward slashes keep configured Windows paths valid C string literals.
    file(TO_CMAKE_PATH "${Python3_EXECUTABLE}" parity_python)
    target_compile_definitions(openxcvr_c_parity PRIVATE
        OPENXCVR_PARITY_PYTHON="${parity_python}"
        OPENXCVR_PARITY_DRIVER="${CMAKE_CURRENT_SOURCE_DIR}/tests/parity.py"
        OPENXCVR_PARITY_ROOT="${CMAKE_CURRENT_SOURCE_DIR}/../.."
    )
    add_test(NAME openxcvr_c_parity COMMAND openxcvr_c_parity)
    add_test(NAME openxcvr_c_parity_driver
        COMMAND "${Python3_EXECUTABLE}"
            "${CMAKE_CURRENT_SOURCE_DIR}/tests/test_parity.py"
            $<TARGET_FILE:openxcvr_c_parity> "${CMAKE_CURRENT_SOURCE_DIR}/../..")
endif()

add_executable(openxcvr_c_identifier_parity
    tests/identifier_parity.c
)
target_link_libraries(openxcvr_c_identifier_parity PRIVATE openxcvr_c)

# Links shared_link.c against the static archive. On this GCC, which
# defaults to PIE, POSITION_INDEPENDENT_CODE OFF still links; an object
# built -fno-pic does not (R_X86_64_32 against .rodata).
add_library(openxcvr_c_shared_link SHARED
    tests/shared_link.c
)
target_link_libraries(openxcvr_c_shared_link PRIVATE openxcvr_c)

add_test(
    NAME openxcvr_c_identifier_parity
    COMMAND openxcvr_c_identifier_parity
        ${CMAKE_CURRENT_SOURCE_DIR}/tests/dispatch_table.txt
)
# Install to a temp prefix under the build dir, then link shared_link.c into a
# shared object against `pkg-config --cflags --libs openxcvr-c` of that prefix.
# The build dir is never reconfigured: a .pc naming the temp prefix is generated
# here, beside the real one, and copied over the installed .pc by the test.
set(OPENXCVR_C_INSTALL_PREFIX "${CMAKE_CURRENT_BINARY_DIR}/prefix")
set(OPENXCVR_C_F16_INSTALL_PREFIX "${CMAKE_CURRENT_BINARY_DIR}/prefix-f16")
function(openxcvr_c_configure_test_pc out_file install_prefix)
    # Function scope: the real CMAKE_INSTALL_PREFIX is untouched.
    set(CMAKE_INSTALL_PREFIX "${install_prefix}")
    configure_file(
        ${CMAKE_CURRENT_SOURCE_DIR}/openxcvr.pc.in
        ${out_file}
        @ONLY
    )
endfunction()
openxcvr_c_configure_test_pc(
    "${CMAKE_CURRENT_BINARY_DIR}/test-pc/openxcvr-c.pc"
    "${OPENXCVR_C_INSTALL_PREFIX}"
)
openxcvr_c_configure_test_pc(
    "${CMAKE_CURRENT_BINARY_DIR}/test-pc-f16/openxcvr-c.pc"
    "${OPENXCVR_C_F16_INSTALL_PREFIX}"
)
file(WRITE "${CMAKE_CURRENT_BINARY_DIR}/installed_shared_link.cmake" [=[cmake_minimum_required(VERSION 3.16)
foreach(v SOURCE_DIR BUILD_DIR INSTALL_PREFIX TEST_PC C_COMPILER SHARED_LIBRARY_SUFFIX)
    if(NOT DEFINED ${v})
        message(FATAL_ERROR "${v} is required")
    endif()
endforeach()
file(REMOVE_RECURSE "${INSTALL_PREFIX}")
file(REMOVE "${BUILD_DIR}/libopenxcvr_c_installed_link${SHARED_LIBRARY_SUFFIX}")
if(EXISTS "${INSTALL_PREFIX}")
    message(FATAL_ERROR "could not clear ${INSTALL_PREFIX}")
endif()
execute_process(
    COMMAND ${CMAKE_COMMAND} --install "${BUILD_DIR}" --prefix "${INSTALL_PREFIX}"
    RESULT_VARIABLE install_rc
)
if(NOT install_rc EQUAL 0)
    message(FATAL_ERROR "cmake --install failed: ${install_rc}")
endif()
# The install rules must have placed the libraries and files the consumer uses.
foreach(f lib/libopenxcvr_c.a lib/libopenxcvr_c${SHARED_LIBRARY_SUFFIX} lib/pkgconfig/openxcvr-c.pc include/openxcvr/openxcvr.h)
    if(NOT EXISTS "${INSTALL_PREFIX}/${f}")
        message(FATAL_ERROR "cmake --install did not install ${f}")
    endif()
endforeach()
# Always overwrite: the .pc just installed names the configured prefix.
configure_file("${TEST_PC}" "${INSTALL_PREFIX}/lib/pkgconfig/openxcvr-c.pc" COPYONLY)
# PKG_CONFIG_LIBDIR replaces the default search path, so only the temp prefix
# is visible; the resolved prefix is asserted before anything is compiled.
set(PKG_CONFIG_COMMAND pkg-config)
if(APPLE)
    find_program(PKG_CONFIG_EXECUTABLE NAMES pkg-config)
    if(NOT PKG_CONFIG_EXECUTABLE)
        message(FATAL_ERROR "pkg-config is required for openxcvr_c_installed_shared_link; add it to PATH")
    endif()
    set(PKG_CONFIG_COMMAND "${PKG_CONFIG_EXECUTABLE}")
endif()
set(ENV{PKG_CONFIG_LIBDIR} "${INSTALL_PREFIX}/lib/pkgconfig")
unset(ENV{PKG_CONFIG_PATH})
execute_process(
    COMMAND ${PKG_CONFIG_COMMAND} --variable=prefix openxcvr-c
    OUTPUT_VARIABLE pc_prefix
    OUTPUT_STRIP_TRAILING_WHITESPACE
    RESULT_VARIABLE pc_rc
)
if(NOT pc_rc EQUAL 0 OR NOT pc_prefix STREQUAL INSTALL_PREFIX)
    message(FATAL_ERROR "pkg-config resolved openxcvr-c at '${pc_prefix}', not ${INSTALL_PREFIX}")
endif()
execute_process(
    COMMAND ${PKG_CONFIG_COMMAND} --cflags --libs openxcvr-c
    OUTPUT_VARIABLE pc_flags
    OUTPUT_STRIP_TRAILING_WHITESPACE
    RESULT_VARIABLE pc_rc
)
if(NOT pc_rc EQUAL 0)
    message(FATAL_ERROR "pkg-config --cflags --libs openxcvr-c failed: ${pc_rc}")
endif()
separate_arguments(pc_flag_list UNIX_COMMAND "${pc_flags}")
# An empty or wrong Libs: line must fail rather than leave the classifier
# unresolved. ld64 defaults to error for dylibs; request it explicitly too.
set(shared_link_flags -shared -fPIC -Wl,--no-undefined)
if(APPLE)
    set(shared_link_flags -dynamiclib -fPIC -Wl,-undefined,error)
endif()
execute_process(
    COMMAND "${C_COMPILER}" ${shared_link_flags}
        "${SOURCE_DIR}/tests/shared_link.c"
        -o "${BUILD_DIR}/libopenxcvr_c_installed_link${SHARED_LIBRARY_SUFFIX}"
        ${pc_flag_list}
    RESULT_VARIABLE link_rc
)
if(NOT link_rc EQUAL 0)
    message(FATAL_ERROR "shared link against installed openxcvr-c failed: ${link_rc}")
endif()
]=])
add_test(
    NAME openxcvr_c_installed_shared_link
    COMMAND ${CMAKE_COMMAND}
        "-DSOURCE_DIR=${CMAKE_CURRENT_SOURCE_DIR}"
        "-DBUILD_DIR=${CMAKE_CURRENT_BINARY_DIR}"
        "-DINSTALL_PREFIX=${OPENXCVR_C_INSTALL_PREFIX}"
        "-DTEST_PC=${CMAKE_CURRENT_BINARY_DIR}/test-pc/openxcvr-c.pc"
        "-DC_COMPILER=${CMAKE_C_COMPILER}"
        "-DSHARED_LIBRARY_SUFFIX=${CMAKE_SHARED_LIBRARY_SUFFIX}"
        -P "${CMAKE_CURRENT_BINARY_DIR}/installed_shared_link.cmake"
)

# Installed F16 consumer (Stage 2 D5/A2): an executable — not only a
# shared object, whose undefined symbols the linker allows — that calls
# openxcvr_cmis_f16_to_double and an F16 observation. Installed to its
# own temp prefix and built from that prefix twice: statically against
# libopenxcvr_c.a named by path with the Libs.private flags from
# `pkg-config --static --libs` (-lopenxcvr_c and -L removed, so the
# shared library cannot satisfy it), and dynamically with
# `pkg-config --cflags --libs` only (no -lm), an rpath into the prefix,
# and the dynamic-loader search-path variables unset. On ELF platforms
# where the fma probe needed libm, the installed shared library itself
# must list libm as NEEDED.
file(WRITE "${CMAKE_CURRENT_BINARY_DIR}/installed_f16_consumer.cmake" [=[cmake_minimum_required(VERSION 3.16)
foreach(v SOURCE_DIR BUILD_DIR INSTALL_PREFIX TEST_PC C_COMPILER SHARED_LIBRARY_SUFFIX MATH_LIBS)
    if(NOT DEFINED ${v})
        message(FATAL_ERROR "${v} is required")
    endif()
endforeach()
file(REMOVE_RECURSE "${INSTALL_PREFIX}")
file(REMOVE "${BUILD_DIR}/openxcvr_c_installed_f16_static")
file(REMOVE "${BUILD_DIR}/openxcvr_c_installed_f16_dynamic")
if(EXISTS "${INSTALL_PREFIX}")
    message(FATAL_ERROR "could not clear ${INSTALL_PREFIX}")
endif()
execute_process(
    COMMAND ${CMAKE_COMMAND} --install "${BUILD_DIR}" --prefix "${INSTALL_PREFIX}"
    RESULT_VARIABLE install_rc
)
if(NOT install_rc EQUAL 0)
    message(FATAL_ERROR "cmake --install failed: ${install_rc}")
endif()
# The install rules must have placed the library surfaces the consumer uses.
foreach(f lib/libopenxcvr_c.a lib/libopenxcvr_c${SHARED_LIBRARY_SUFFIX} lib/pkgconfig/openxcvr-c.pc include/openxcvr/openxcvr.h include/openxcvr/observation.h)
    if(NOT EXISTS "${INSTALL_PREFIX}/${f}")
        message(FATAL_ERROR "cmake --install did not install ${f}")
    endif()
endforeach()
# Always overwrite: the .pc just installed names the configured prefix.
configure_file("${TEST_PC}" "${INSTALL_PREFIX}/lib/pkgconfig/openxcvr-c.pc" COPYONLY)
# PKG_CONFIG_LIBDIR replaces the default search path, so only the temp
# prefix is visible; the resolved prefix is asserted before anything is
# compiled.
set(PKG_CONFIG_COMMAND pkg-config)
if(APPLE)
    find_program(PKG_CONFIG_EXECUTABLE NAMES pkg-config)
    if(NOT PKG_CONFIG_EXECUTABLE)
        message(FATAL_ERROR "pkg-config is required for openxcvr_c_installed_f16_consumer; add it to PATH")
    endif()
    set(PKG_CONFIG_COMMAND "${PKG_CONFIG_EXECUTABLE}")
endif()
set(ENV{PKG_CONFIG_LIBDIR} "${INSTALL_PREFIX}/lib/pkgconfig")
unset(ENV{PKG_CONFIG_PATH})
execute_process(
    COMMAND ${PKG_CONFIG_COMMAND} --variable=prefix openxcvr-c
    OUTPUT_VARIABLE pc_prefix
    OUTPUT_STRIP_TRAILING_WHITESPACE
    RESULT_VARIABLE pc_rc
)
if(NOT pc_rc EQUAL 0 OR NOT pc_prefix STREQUAL INSTALL_PREFIX)
    message(FATAL_ERROR "pkg-config resolved openxcvr-c at '${pc_prefix}', not ${INSTALL_PREFIX}")
endif()

# On ELF platforms where the fma probe needed libm, the installed shared
# library itself must carry the libm dependency (DT_NEEDED): a private
# link on openxcvr_c_shared, not a flag consumers must repeat. The ELF
# check reads the installed file's magic, not a configure-time guess.
set(shared_lib "${INSTALL_PREFIX}/lib/libopenxcvr_c${SHARED_LIBRARY_SUFFIX}")
file(READ "${shared_lib}" shared_magic LIMIT 4 HEX)
if(shared_magic STREQUAL "7f454c46" AND MATH_LIBS STREQUAL "m")
    set(needed_dump "")
    find_program(READELF_EXECUTABLE NAMES readelf)
    find_program(OBJDUMP_EXECUTABLE NAMES objdump)
    if(READELF_EXECUTABLE)
        execute_process(
            COMMAND "${READELF_EXECUTABLE}" -d "${shared_lib}"
            OUTPUT_VARIABLE needed_dump
            RESULT_VARIABLE needed_rc
        )
        if(NOT needed_rc EQUAL 0)
            message(FATAL_ERROR "readelf -d failed on ${shared_lib}")
        endif()
    elseif(OBJDUMP_EXECUTABLE)
        execute_process(
            COMMAND "${OBJDUMP_EXECUTABLE}" -p "${shared_lib}"
            OUTPUT_VARIABLE needed_dump
            RESULT_VARIABLE needed_rc
        )
        if(NOT needed_rc EQUAL 0)
            message(FATAL_ERROR "objdump -p failed on ${shared_lib}")
        endif()
    else()
        message(FATAL_ERROR
            "neither readelf nor objdump found: cannot verify libm NEEDED on ${shared_lib}")
    endif()
    if(NOT needed_dump MATCHES "NEEDED.*libm")
        message(FATAL_ERROR "${shared_lib} does not list libm as NEEDED")
    endif()
endif()

# Static: the installed archive named by path, plus the flags
# `pkg-config --static --libs` carries beyond the library itself
# (-lopenxcvr_c and -L removed), so the shared library cannot satisfy
# the reference and Libs.private must close it.
execute_process(
    COMMAND ${PKG_CONFIG_COMMAND} --static --libs openxcvr-c
    OUTPUT_VARIABLE pc_static
    OUTPUT_STRIP_TRAILING_WHITESPACE
    RESULT_VARIABLE pc_static_rc
)
if(NOT pc_static_rc EQUAL 0)
    message(FATAL_ERROR "pkg-config --static --libs openxcvr-c failed: ${pc_static_rc}")
endif()
separate_arguments(static_flag_list UNIX_COMMAND "${pc_static}")
set(static_lib_flags "")
foreach(flag IN LISTS static_flag_list)
    if(NOT flag STREQUAL "-lopenxcvr_c" AND NOT flag MATCHES "^-L")
        list(APPEND static_lib_flags "${flag}")
    endif()
endforeach()
execute_process(
    COMMAND "${C_COMPILER}" -Wall -Wextra -Werror
        "${SOURCE_DIR}/tests/installed_f16_consumer.c"
        -o "${BUILD_DIR}/openxcvr_c_installed_f16_static"
        -I${INSTALL_PREFIX}/include
        "${INSTALL_PREFIX}/lib/libopenxcvr_c.a"
        ${static_lib_flags}
    RESULT_VARIABLE static_link_rc
)
if(NOT static_link_rc EQUAL 0)
    message(FATAL_ERROR "static link against installed openxcvr-c failed: ${static_link_rc}")
endif()
execute_process(
    COMMAND "${BUILD_DIR}/openxcvr_c_installed_f16_static"
    RESULT_VARIABLE static_run_rc
)
if(NOT static_run_rc EQUAL 0)
    message(FATAL_ERROR "installed static F16 consumer failed: ${static_run_rc}")
endif()

# Dynamic: `pkg-config --cflags --libs` only — those flags must not name
# -lm — with an rpath into the installed prefix, then run with the
# dynamic-loader search-path variables unset (the rpath locates it).
execute_process(
    COMMAND ${PKG_CONFIG_COMMAND} --cflags --libs openxcvr-c
    OUTPUT_VARIABLE pc_flags
    OUTPUT_STRIP_TRAILING_WHITESPACE
    RESULT_VARIABLE pc_rc
)
if(NOT pc_rc EQUAL 0)
    message(FATAL_ERROR "pkg-config --cflags --libs openxcvr-c failed: ${pc_rc}")
endif()
separate_arguments(pc_flag_list UNIX_COMMAND "${pc_flags}")
foreach(flag IN LISTS pc_flag_list)
    if(flag STREQUAL "-lm")
        message(FATAL_ERROR "pkg-config --cflags --libs openxcvr-c must not name -lm (got '${pc_flags}')")
    endif()
endforeach()
execute_process(
    COMMAND "${C_COMPILER}" -Wall -Wextra -Werror
        "${SOURCE_DIR}/tests/installed_f16_consumer.c"
        -o "${BUILD_DIR}/openxcvr_c_installed_f16_dynamic"
        ${pc_flag_list}
        "-Wl,-rpath,${INSTALL_PREFIX}/lib"
    RESULT_VARIABLE dynamic_link_rc
)
if(NOT dynamic_link_rc EQUAL 0)
    message(FATAL_ERROR "dynamic link against installed openxcvr-c failed: ${dynamic_link_rc}")
endif()
unset(ENV{LD_LIBRARY_PATH})
unset(ENV{DYLD_LIBRARY_PATH})
execute_process(
    COMMAND "${BUILD_DIR}/openxcvr_c_installed_f16_dynamic"
    RESULT_VARIABLE dynamic_run_rc
)
if(NOT dynamic_run_rc EQUAL 0)
    message(FATAL_ERROR "installed dynamic F16 consumer failed: ${dynamic_run_rc}")
endif()
]=])
add_test(
    NAME openxcvr_c_installed_f16_consumer
    COMMAND ${CMAKE_COMMAND}
        "-DSOURCE_DIR=${CMAKE_CURRENT_SOURCE_DIR}"
        "-DBUILD_DIR=${CMAKE_CURRENT_BINARY_DIR}"
        "-DINSTALL_PREFIX=${OPENXCVR_C_F16_INSTALL_PREFIX}"
        "-DTEST_PC=${CMAKE_CURRENT_BINARY_DIR}/test-pc-f16/openxcvr-c.pc"
        "-DC_COMPILER=${CMAKE_C_COMPILER}"
        "-DSHARED_LIBRARY_SUFFIX=${CMAKE_SHARED_LIBRARY_SUFFIX}"
        "-DMATH_LIBS=${OPENXCVR_C_MATH_LIBS}"
        -P "${CMAKE_CURRENT_BINARY_DIR}/installed_f16_consumer.cmake"
)

configure_file(
    ${CMAKE_CURRENT_SOURCE_DIR}/openxcvr.pc.in
    ${CMAKE_CURRENT_BINARY_DIR}/openxcvr-c.pc
    @ONLY
)

install(TARGETS openxcvr_c openxcvr_c_shared
    ARCHIVE DESTINATION lib
    LIBRARY DESTINATION lib
    RUNTIME DESTINATION bin
)
install(DIRECTORY include/ DESTINATION include
    PATTERN "application_records.h" EXCLUDE
)
# openxcvr.h includes openxcvr_generated.h from this tree. Without it the
# installed prefix does not compile against the .pc Cflags.
install(DIRECTORY generated/ DESTINATION include
    FILES_MATCHING PATTERN "*.h"
)
install(FILES ${CMAKE_CURRENT_BINARY_DIR}/openxcvr-c.pc DESTINATION lib/pkgconfig)

add_executable(openxcvr_c_fsm_tables
    tests/fsm_tables.c
    ${OPENXCVR_C_FSM_GENERATED_SOURCES}
)
target_compile_features(openxcvr_c_fsm_tables PRIVATE c_std_99)
target_include_directories(openxcvr_c_fsm_tables PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}/generated
)
add_test(NAME openxcvr_c_fsm_tables COMMAND openxcvr_c_fsm_tables)

add_executable(openxcvr_c_identity_ranges tests/identity_ranges.c)
target_link_libraries(openxcvr_c_identity_ranges PRIVATE openxcvr_c)
# Assertions are the test oracle even in release builds.
target_compile_options(openxcvr_c_identity_ranges PRIVATE -UNDEBUG)
add_test(NAME openxcvr_c_identity_ranges COMMAND openxcvr_c_identity_ranges)

# Revision-bound DPSM name projection (schema-owned since the enum revision
# window work): assertion-based native tests against the generated header's
# factory, plus a header-only translation unit that must compile freestanding
# under strict warnings with no projection call at all.
add_executable(openxcvr_c_dpsm_projection tests/dpsm_projection.c)
target_link_libraries(openxcvr_c_dpsm_projection PRIVATE openxcvr_c)
target_compile_options(openxcvr_c_dpsm_projection PRIVATE -Wall -Wextra -Werror -UNDEBUG)
add_test(NAME openxcvr_c_dpsm_projection COMMAND openxcvr_c_dpsm_projection)
add_library(openxcvr_c_dpsm_projection_header OBJECT tests/dpsm_projection_header.c)
target_compile_options(openxcvr_c_dpsm_projection_header PRIVATE
    -Wall -Wextra -Werror -ffreestanding
)
target_include_directories(openxcvr_c_dpsm_projection_header PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}/generated
)

# Public-normalizer DPSM revision-context behavior through the actual public
# entrypoint openxcvr_decode_cmis_json (unsupported present contexts render
# Unknown<n> with the snapshot unchanged; missing Page 11h/flat-memory keep
# the existing null blocks; the Page 01h prerequisite error is unchanged).
add_executable(openxcvr_c_dpsm_context tests/dpsm_context.c)
target_link_libraries(openxcvr_c_dpsm_context PRIVATE openxcvr_c)
target_compile_options(openxcvr_c_dpsm_context PRIVATE -Wall -Wextra -Werror -UNDEBUG)
add_test(NAME openxcvr_c_dpsm_context COMMAND openxcvr_c_dpsm_context)
set_tests_properties(openxcvr_c_dpsm_context PROPERTIES
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/../..
)

# Revision-qualified DPSM terminal predicate and expected active terminal
# helper (Issue #1 Ask 2 C parity with the Rust adapter): the exhaustive
# 384-cell terminal matrix and 24-cell expected-terminal coherence matrix
# against literal spec oracles, fail-closed error/sentinel checks, and the
# generated page01h.tx_bias_current_scaling_factor scaling read through the
# public openxcvr_decode_cmis_json entrypoint.
add_executable(openxcvr_c_dpsm_terminal tests/dpsm_terminal.c)
target_link_libraries(openxcvr_c_dpsm_terminal PRIVATE openxcvr_c)
target_compile_options(openxcvr_c_dpsm_terminal PRIVATE -Wall -Wextra -Werror -UNDEBUG)
add_test(NAME openxcvr_c_dpsm_terminal COMMAND openxcvr_c_dpsm_terminal)
set_tests_properties(openxcvr_c_dpsm_terminal PROPERTIES
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/../..
)

# CMIS module temperature formatting precision (round-trip test for n/256.0).
add_executable(openxcvr_c_cmis_temperature tests/cmis_temperature.c)
target_link_libraries(openxcvr_c_cmis_temperature PRIVATE openxcvr_c)
target_compile_options(openxcvr_c_cmis_temperature PRIVATE -Wall -Wextra -Werror -UNDEBUG)
add_test(NAME openxcvr_c_cmis_temperature COMMAND openxcvr_c_cmis_temperature)
set_tests_properties(openxcvr_c_cmis_temperature PROPERTIES
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/../..
)


add_executable(openxcvr_c_media_interface tests/media_interface.c)
target_link_libraries(openxcvr_c_media_interface PRIVATE openxcvr_c)
target_compile_options(openxcvr_c_media_interface PRIVATE -Wall -Wextra -Werror -UNDEBUG)
add_test(NAME openxcvr_c_media_interface COMMAND openxcvr_c_media_interface)
set_tests_properties(openxcvr_c_media_interface PROPERTIES
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/../..
)

add_executable(openxcvr_c_application_numbering tests/application_numbering.c)
target_link_libraries(openxcvr_c_application_numbering PRIVATE openxcvr_c)
target_compile_options(openxcvr_c_application_numbering PRIVATE -Wall -Wextra -Werror -UNDEBUG)
add_test(NAME openxcvr_c_application_numbering COMMAND openxcvr_c_application_numbering)
set_tests_properties(openxcvr_c_application_numbering PROPERTIES
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/../..
)

# Catalog resolution and basic-observation parity (P-OBS basic/catalog).
# The catalog/observation implementation units and the generated catalog
# tables are built with strict warnings; the legacy sources keep their own
# warning posture.
file(GLOB OPENXCVR_CATALOG_GENERATED_SOURCES CONFIGURE_DEPENDS
    generated/catalog/*.c
)
add_library(openxcvr_catalog_obs OBJECT
    src/catalog.c
    src/observation.c
    src/optical_power.c
    ${OPENXCVR_CATALOG_GENERATED_SOURCES}
)
# Linked into the shared target as well: build PIC like the other sources.
set_target_properties(openxcvr_catalog_obs PROPERTIES POSITION_INDEPENDENT_CODE ON)
target_compile_options(openxcvr_catalog_obs PRIVATE -Wall -Wextra -Werror)
target_include_directories(openxcvr_catalog_obs PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}/include
    ${CMAKE_CURRENT_SOURCE_DIR}/generated
)
foreach(target openxcvr_c openxcvr_c_shared)
    target_sources(${target} PRIVATE $<TARGET_OBJECTS:openxcvr_catalog_obs>)
endforeach()
add_executable(openxcvr_c_catalog_observation tests/catalog_observation.c)
target_link_libraries(openxcvr_c_catalog_observation PRIVATE openxcvr_c)
target_compile_options(openxcvr_c_catalog_observation PRIVATE -Wall -Wextra -Werror -UNDEBUG)
add_test(NAME openxcvr_c_catalog_observation COMMAND openxcvr_c_catalog_observation)

# CMIS F16 numeric conversion: exhaustive digest against the exact
# reference, per-code strtod oracle and shared goldens, mirroring
# crates/openxcvr-core/tests/cmis_f16.rs.
add_executable(openxcvr_c_cmis_f16 tests/cmis_f16.c)
target_link_libraries(openxcvr_c_cmis_f16 PRIVATE openxcvr_c)
target_compile_options(openxcvr_c_cmis_f16 PRIVATE -Wall -Wextra -Werror -UNDEBUG)
add_test(NAME openxcvr_c_cmis_f16 COMMAND openxcvr_c_cmis_f16)

# Stage 1 supplied-data admission parity: Page 14h selector-content gating
# and CMIS reported-revision admission on the generic observe path. Loads
# the credo capture fixtures from the repo root.
add_executable(openxcvr_c_page14h_admission tests/page14h_admission.c)
target_link_libraries(openxcvr_c_page14h_admission PRIVATE openxcvr_c)
target_compile_options(openxcvr_c_page14h_admission PRIVATE -Wall -Wextra -Werror -UNDEBUG)
add_test(NAME openxcvr_c_page14h_admission COMMAND openxcvr_c_page14h_admission)
set_tests_properties(openxcvr_c_page14h_admission PROPERTIES
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/../..
)

# Stage 1b field read plan prelude: the openxcvr_field_read_prelude
# contract against catalog-resolved entries. Opens no fixtures; until the
# implementation lands it exercises the link stubs and fails at the first
# prelude call.
add_executable(openxcvr_c_field_read_prelude tests/field_read_prelude.c)
target_link_libraries(openxcvr_c_field_read_prelude PRIVATE openxcvr_c)
target_compile_options(openxcvr_c_field_read_prelude PRIVATE -Wall -Wextra -Werror -UNDEBUG)
add_test(NAME openxcvr_c_field_read_prelude COMMAND openxcvr_c_field_read_prelude)

# Stage 1b field read remaining: the openxcvr_field_read_remaining
# contract against catalog-resolved entries and synthetic observation
# bytes. Opens no fixtures; until the implementation lands it exercises
# the link stubs and fails at the first remaining call.
add_executable(openxcvr_c_field_read_remaining tests/field_read_remaining.c)
target_link_libraries(openxcvr_c_field_read_remaining PRIVATE openxcvr_c)
target_compile_options(openxcvr_c_field_read_remaining PRIVATE -Wall -Wextra -Werror -UNDEBUG)
add_test(NAME openxcvr_c_field_read_remaining COMMAND openxcvr_c_field_read_remaining)

# SFF supplied wire-value readers (design R2/R3 lane C-a): fixture values,
# negative i16be, NULL/overflow/truncation failure paths with untouched
# outputs, and exact mid-page supplied subranges.
add_executable(openxcvr_c_supplied_readers tests/supplied_readers.c)
target_link_libraries(openxcvr_c_supplied_readers PRIVATE openxcvr_c)
target_compile_options(openxcvr_c_supplied_readers PRIVATE -Wall -Wextra -Werror -UNDEBUG)
add_test(NAME openxcvr_c_supplied_readers COMMAND openxcvr_c_supplied_readers)
set_tests_properties(openxcvr_c_supplied_readers PROPERTIES
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/../..
)

# Implicit sub-byte width regression: CMIS Page 01h media_lane_count_app
# (u4, implicit width) must mask high bits (raw 0xA5 -> 5, not 165) in all
# seven application slots; host_lane_count_app guards the explicit-width path.
add_executable(openxcvr_c_implicit_subbyte_widths tests/implicit_subbyte_widths.c)
target_link_libraries(openxcvr_c_implicit_subbyte_widths PRIVATE openxcvr_c)
target_compile_options(openxcvr_c_implicit_subbyte_widths PRIVATE -Wall -Wextra -Werror -UNDEBUG)
add_test(NAME openxcvr_c_implicit_subbyte_widths COMMAND openxcvr_c_implicit_subbyte_widths)

# CMIS Page 14h u64le: wire bytes 80 00 00 00 00 00 00 01 must decode
# little-endian. A swapped order or a truncated shift fails the assertion.
add_executable(openxcvr_c_page14h_u64 tests/page14h_u64.c)
target_link_libraries(openxcvr_c_page14h_u64 PRIVATE openxcvr_c)
target_compile_options(openxcvr_c_page14h_u64 PRIVATE -Wall -Wextra -Werror -UNDEBUG)
add_test(NAME openxcvr_c_page14h_u64 COMMAND openxcvr_c_page14h_u64)

# Revision-checked CMIS Page 01h/11h contract (Task 2).
add_executable(openxcvr_c_checked_pages tests/checked_pages.c)
target_link_libraries(openxcvr_c_checked_pages PRIVATE openxcvr_c)
target_compile_options(openxcvr_c_checked_pages PRIVATE -Wall -Wextra -Werror -UNDEBUG)
add_test(NAME openxcvr_c_checked_pages COMMAND openxcvr_c_checked_pages)

# Task 3 consumer evidence: differential, installed consumers, C++
# linkage and kernel-style portability. Everything below is test-only and
# gated on BUILD_TESTING so a library-only configure (BUILD_TESTING=OFF)
# stays C-only and needs neither Python, C++ nor the sibling kmod.
if(BUILD_TESTING)
    # Canonical C projection runner for the C/Rust differential.
    add_executable(openxcvr_c_checked_page_runner tests/checked_page_runner.c)
    target_link_libraries(openxcvr_c_checked_page_runner PRIVATE openxcvr_c)
    target_compile_options(openxcvr_c_checked_page_runner PRIVATE
        -Wall -Wextra -Werror -UNDEBUG
    )

    # Opt-in tracked gate: the deterministic differential against the
    # cargo-built Rust reference example. Default OFF keeps the default
    # C build Cargo-free; enabling it without a reference fails configure.
    option(OPENXCVR_C_CHECKED_DIFFERENTIAL
        "Wire the C/Rust revision-checked page differential into CTest" OFF)
    set(OPENXCVR_C_CHECKED_REFERENCE "" CACHE FILEPATH
        "cargo-built c-page-revision-reference example for the differential")
    if(OPENXCVR_C_CHECKED_DIFFERENTIAL)
        if(OPENXCVR_C_CHECKED_REFERENCE AND NOT EXISTS "${OPENXCVR_C_CHECKED_REFERENCE}")
            message(FATAL_ERROR
                "OPENXCVR_C_CHECKED_REFERENCE does not exist: ${OPENXCVR_C_CHECKED_REFERENCE}")
        endif()
        set(OPENXCVR_C_CHECKED_REFERENCE_PATH "")
        foreach(candidate
            "${OPENXCVR_C_CHECKED_REFERENCE}"
            "$ENV{CARGO_TARGET_DIR}/debug/examples/c-page-revision-reference"
            "${CMAKE_CURRENT_SOURCE_DIR}/../../target/debug/examples/c-page-revision-reference")
            if(candidate AND NOT OPENXCVR_C_CHECKED_REFERENCE_PATH AND EXISTS "${candidate}")
                set(OPENXCVR_C_CHECKED_REFERENCE_PATH "${candidate}")
            endif()
        endforeach()
        if(NOT OPENXCVR_C_CHECKED_REFERENCE_PATH)
            message(FATAL_ERROR
                "OPENXCVR_C_CHECKED_DIFFERENTIAL=ON needs the cargo-built reference:
 build it with: cargo build -p openxcvr-cmis --example c-page-revision-reference
 or set OPENXCVR_C_CHECKED_REFERENCE")
        endif()
        find_package(Python3 COMPONENTS Interpreter REQUIRED)
        add_test(NAME openxcvr_c_checked_page_differential
            COMMAND ${Python3_EXECUTABLE}
                ${CMAKE_CURRENT_SOURCE_DIR}/tests/checked_page_differential.py
                $<TARGET_FILE:openxcvr_c_checked_page_runner>
                "${OPENXCVR_C_CHECKED_REFERENCE_PATH}"
        )
    endif()

    find_package(Python3 COMPONENTS Interpreter REQUIRED)

    # Installed static/shared consumers of both new APIs, through the
    # installed headers and pkg-config, into a scratch prefix under the
    # build directory only.
    add_test(NAME openxcvr_c_installed_checked_consumer
        COMMAND ${Python3_EXECUTABLE}
            ${CMAKE_CURRENT_SOURCE_DIR}/tests/installed_checked_consumer.py
            "${CMAKE_CURRENT_BINARY_DIR}" "${CMAKE_CURRENT_SOURCE_DIR}"
            "${CMAKE_C_COMPILER}" "${CMAKE_SHARED_LIBRARY_SUFFIX}"
    )

    # Kernel-style portability of the generated revision sources. The
    # sibling the kernel module is optional: without it (a clean CI checkout)
    # the freestanding -Wdeclaration-after-statement gate still runs.
    set(OPENXCVR_C_KMOD_PLUGINS
        "${CMAKE_CURRENT_SOURCE_DIR}/../../../the kernel module/plugins/cmis"
        CACHE PATH "Sibling the kernel module plugins/cmis for the kernel-recipe gate")
    if(EXISTS "${OPENXCVR_C_KMOD_PLUGINS}/openxcvr_decoders.c")
        set(OPENXCVR_C_KMOD_PLUGINS_ARG "${OPENXCVR_C_KMOD_PLUGINS}")
    else()
        set(OPENXCVR_C_KMOD_PLUGINS_ARG "-")
    endif()
    add_test(NAME openxcvr_c_checked_page_portability
        COMMAND ${Python3_EXECUTABLE}
            ${CMAKE_CURRENT_SOURCE_DIR}/tests/checked_page_portability.py
            "${CMAKE_C_COMPILER}" "${CMAKE_CURRENT_SOURCE_DIR}"
            "${OPENXCVR_C_KMOD_PLUGINS_ARG}"
            "${CMAKE_CURRENT_BINARY_DIR}/checked-portability"
    )

    # C++ linkage smoke for both checked APIs. The library stays C-only;
    # a missing C++ compiler skips this one test with a warning instead of
    # failing the (still C-only) library build.
    include(CheckLanguage)
    check_language(CXX)
    if(CMAKE_CXX_COMPILER)
        enable_language(CXX)
        add_executable(openxcvr_c_checked_linkage_cpp tests/checked_linkage.cpp)
        target_link_libraries(openxcvr_c_checked_linkage_cpp PRIVATE openxcvr_c)
        target_compile_options(openxcvr_c_checked_linkage_cpp PRIVATE
            -Wall -Wextra -Werror
        )
        add_test(NAME openxcvr_c_checked_linkage_cpp
            COMMAND openxcvr_c_checked_linkage_cpp
        )
    else()
        message(WARNING
            "No C++ compiler found: openxcvr_c_checked_linkage_cpp smoke skipped")
    endif()
endif()

# The standalone C binding remains independent of Cargo and the rich ABI.
option(OPENXCVR_C_RICH "Build the optional Rust-backed rich-record C component" OFF)
if(OPENXCVR_C_RICH)
    include(cmake/rich.cmake)
endif()

# Pure optical display conversion; shared numerical vectors with Rust.
add_executable(openxcvr_c_optical_dbm tests/optical_dbm.c)
target_link_libraries(openxcvr_c_optical_dbm PRIVATE openxcvr_c)
target_compile_options(openxcvr_c_optical_dbm PRIVATE -Wall -Wextra -Werror -UNDEBUG)
add_test(NAME openxcvr_c_optical_dbm COMMAND openxcvr_c_optical_dbm
    ${CMAKE_CURRENT_SOURCE_DIR}/../../tests/fixtures/optical-dbm.tsv)
