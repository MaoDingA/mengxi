#!/bin/bash
# build_moonbit.sh — Build MoonBit static library for Rust FFI
# Run from project root: ./build_moonbit.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
MOONBIT_DIR="${SCRIPT_DIR}/moonbit"
BUILD_LIB_DIR="${MOONBIT_DIR}/_build/native/debug/build/lib"
BUILD_RUNTIME_DIR="${MOONBIT_DIR}/_build/native/debug/build"
TARGET_DIR="${MOONBIT_DIR}/target"
ARCHIVE="${TARGET_DIR}/libmoonbit_core.a"

echo "Building MoonBit library..."
(cd "$MOONBIT_DIR" && moon build --target native 2>&1 || true)

# Compile generated C to object file
echo "Compiling generated C..."
cc -c -I"${HOME}/.moon/include" -g -fwrapv -fno-strict-aliasing -Wno-unused-value \
    "${BUILD_LIB_DIR}/lib.c" -o "${BUILD_LIB_DIR}/lib.o"

# Compile FFI wrapper
echo "Compiling FFI wrapper..."
mkdir -p "$TARGET_DIR"
cc -c -g "${MOONBIT_DIR}/src/lib/ffi.c" -o "${TARGET_DIR}/ffi.o"

# Archive into static library
echo "Archiving ${ARCHIVE}..."
rm -f "$ARCHIVE"
ar rcs "$ARCHIVE" \
    "${BUILD_LIB_DIR}/lib.o" \
    "${TARGET_DIR}/ffi.o" \
    "${BUILD_RUNTIME_DIR}/runtime.o"

echo "Done. Static library at ${ARCHIVE}"
