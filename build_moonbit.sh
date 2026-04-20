#!/bin/bash
# build_moonbit.sh — Build MoonBit static library for Rust FFI
# Run from project root: ./build_moonbit.sh

set -e  # Exit on error

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
MOONBIT_DIR="${SCRIPT_DIR}/moonbit"
BUILD_LIB_DIR="${MOONBIT_DIR}/_build/native/debug/build/lib"
BUILD_RUNTIME_DIR="${MOONBIT_DIR}/_build/native/debug/build"
TARGET_DIR="${MOONBIT_DIR}/target"
ARCHIVE="${TARGET_DIR}/libmoonbit_core.a"

# Version check
REQUIRED_MOONBIT="0.8"
ACTUAL_MOONBIT=$(moon --version 2>/dev/null | head -1 | grep -oE '[0-9]+\.[0-9]+' || echo "unknown")
if [ "$ACTUAL_MOONBIT" = "unknown" ]; then
    echo "Warning: Cannot detect MoonBit version, proceeding anyway..."
elif ! python3 -c "
from packaging.version import parse; parse('$ACTUAL_MOONBIT') >= parse('$REQUIRED_MOONBIT')
" 2>/dev/null; then
    echo "Error: MoonBit v$REQUIRED_MOONBIT+ required, found v$ACTUAL_MOONBIT"
    echo "Visit https://www.moonbitlang.com/ to install or upgrade MoonBit."
    exit 1
fi
echo "MoonBit version: $ACTUAL_MOONBIT (OK)"

echo "Building MoonBit library..."
# moon build --target native compiles MoonBit to C (lib.c) then tries to link an
# executable.  For a library project (is-main: false) the link step always fails
# with "Undefined symbol: _main" — that is expected.  We only need the compiled
# C output, so ignore the linker error and compile lib.c manually below.
(cd "$MOONBIT_DIR" && moon build --target native) || true

# Dynamic include path discovery
MOON_INCLUDE="$(moon env MOONBIT_LIB 2>/dev/null | sed 's|/lib$|/include|' || echo "${HOME}/.moon/include")"
if [ ! -d "$MOON_INCLUDE" ]; then
    MOON_INCLUDE="${HOME}/.moon/include"
fi
echo "MoonBit include path: ${MOON_INCLUDE}"

# Compile generated C to object file
echo "Compiling generated C..."
cc -c -I"${MOON_INCLUDE}" -g -fwrapv -fno-strict-aliasing -Wno-unused-value \
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
