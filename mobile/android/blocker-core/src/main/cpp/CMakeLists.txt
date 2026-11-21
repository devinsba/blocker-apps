# Builds libblocker_jni.so: the C JNI shim (blocker_jni.c) statically linked
# against the Rust core (libblocker_ffi.a, built per ABI by
# scripts/build-android.sh).
cmake_minimum_required(VERSION 3.22.1)
project(blocker_jni C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# This file lives at <repo>/mobile/android/blocker-core/src/main/cpp.
get_filename_component(_default_repo_root
  "${CMAKE_CURRENT_SOURCE_DIR}/../../../../../.." ABSOLUTE)
set(BLOCKER_REPO_ROOT "${_default_repo_root}" CACHE PATH "blocker-apps repository root")
set(BLOCKER_FFI_INCLUDE_DIR "${BLOCKER_REPO_ROOT}/crates/blocker-ffi/include"
  CACHE PATH "Directory containing blocker.h")

# scripts/build-android.sh copies libblocker_ffi.a here, one directory per
# Android ABI (arm64-v8a, armeabi-v7a, x86_64, x86).
get_filename_component(_default_lib_root
  "${CMAKE_CURRENT_SOURCE_DIR}/../jniLibs-static" ABSOLUTE)
set(BLOCKER_FFI_LIB_ROOT "${_default_lib_root}" CACHE PATH
  "Directory with <abi>/libblocker_ffi.a")
set(BLOCKER_FFI_STATIC_LIB "${BLOCKER_FFI_LIB_ROOT}/${ANDROID_ABI}/libblocker_ffi.a")

if(NOT EXISTS "${BLOCKER_FFI_INCLUDE_DIR}/blocker.h")
  message(FATAL_ERROR "blocker.h not found in ${BLOCKER_FFI_INCLUDE_DIR}")
endif()
if(NOT EXISTS "${BLOCKER_FFI_STATIC_LIB}")
  message(FATAL_ERROR
    "Missing ${BLOCKER_FFI_STATIC_LIB}.\n"
    "Build the Rust core for Android first:  ANDROID_NDK_HOME=... scripts/build-android.sh")
endif()

add_library(blocker_ffi STATIC IMPORTED)
set_target_properties(blocker_ffi PROPERTIES IMPORTED_LOCATION "${BLOCKER_FFI_STATIC_LIB}")

add_library(blocker_jni SHARED blocker_jni.c)
target_include_directories(blocker_jni PRIVATE "${BLOCKER_FFI_INCLUDE_DIR}")
target_compile_options(blocker_jni PRIVATE -Wall -Wextra -Werror -fvisibility=hidden)

find_library(log-lib log)
target_link_libraries(blocker_jni PRIVATE blocker_ffi ${log-lib} dl m)
# Only JNI_OnLoad and Java_* are exported (JNIEXPORT); drop unused Rust code.
# 16 KB page alignment: required on 16 KB-page devices (Android 15+) and by
# Google Play for new apps; NDK r28+ does this by default, r27 does not.
target_link_options(blocker_jni PRIVATE
  "-Wl,--gc-sections" "-Wl,--exclude-libs,ALL" "-Wl,-z,max-page-size=16384")
