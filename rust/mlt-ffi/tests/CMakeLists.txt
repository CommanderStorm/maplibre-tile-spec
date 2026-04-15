cmake_minimum_required(VERSION 3.28)
project(mlt-ffi-test LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# Resolve paths once relative to the source tree.
cmake_path(SET FFI_INCLUDE NORMALIZE "${CMAKE_CURRENT_SOURCE_DIR}/../../mlt-ffi/generated-c")
cmake_path(SET RUST_RELEASE NORMALIZE "${CMAKE_CURRENT_SOURCE_DIR}/../../target/release")
cmake_path(SET REPO_ROOT NORMALIZE "${CMAKE_CURRENT_SOURCE_DIR}/../../..")
cmake_path(SET GTEST_ROOT NORMALIZE "${CMAKE_CURRENT_SOURCE_DIR}/../../../cpp/vendor/googletest")

# ── Google Test (vendored from cpp/) ──────────────────────────────────────
set(BUILD_GMOCK OFF CACHE BOOL "" FORCE)
set(INSTALL_GTEST OFF CACHE BOOL "" FORCE)
set(gtest_force_shared_crt ON CACHE BOOL "" FORCE)

add_subdirectory("${GTEST_ROOT}" "${CMAKE_CURRENT_BINARY_DIR}/googletest" EXCLUDE_FROM_ALL SYSTEM)

# ── Test executable ───────────────────────────────────────────────────────
add_executable(mlt-ffi-test test_ffi.cpp)
target_include_directories(mlt-ffi-test PRIVATE "${FFI_INCLUDE}")

# Link the Rust cdylib
find_library(MLT_FFI_LIB mlt_ffi PATHS "${RUST_RELEASE}" REQUIRED NO_DEFAULT_PATH)
target_link_libraries(mlt-ffi-test PRIVATE "${MLT_FFI_LIB}" gtest_main)

# Set RPATH so the shared library is found at runtime
set_target_properties(mlt-ffi-test PROPERTIES
    BUILD_RPATH "${RUST_RELEASE}"
)

# ── CTest discovery ──────────────────────────────────────────────────────
include(CTest)
include(GoogleTest)
gtest_discover_tests(mlt-ffi-test
    WORKING_DIRECTORY "${REPO_ROOT}"
    DISCOVERY_MODE PRE_TEST
)
