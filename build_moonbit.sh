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

# Version check.  Recent MoonBit toolchains expose the language/compiler version
# through `moonc -v`; `moon version` reports the build-system version.
REQUIRED_MOONBIT="0.8"
if ! command -v moon >/dev/null 2>&1; then
    echo "Error: moon command not found. Install MoonBit v${REQUIRED_MOONBIT}+."
    exit 1
fi
if ! command -v moonc >/dev/null 2>&1; then
    echo "Error: moonc command not found. Install MoonBit v${REQUIRED_MOONBIT}+."
    exit 1
fi

ACTUAL_MOONBIT=$(moonc -v 2>/dev/null | head -1 | grep -oE 'v?[0-9]+\.[0-9]+(\.[0-9]+)?' | head -1 || true)
if [ -z "$ACTUAL_MOONBIT" ]; then
    ACTUAL_MOONBIT=$(moon version 2>/dev/null | head -1 | grep -oE 'v?[0-9]+\.[0-9]+(\.[0-9]+)?' | head -1 || true)
fi

if [ -z "$ACTUAL_MOONBIT" ]; then
    echo "Warning: Cannot detect MoonBit compiler version, proceeding anyway..."
else
    if ! python3 - "$ACTUAL_MOONBIT" "$REQUIRED_MOONBIT" <<'PY'
import re
import sys

actual, required = sys.argv[1], sys.argv[2]

def major_minor(value):
    match = re.search(r"(\d+)\.(\d+)", value)
    if not match:
        raise SystemExit(2)
    return int(match.group(1)), int(match.group(2))

raise SystemExit(0 if major_minor(actual) >= major_minor(required) else 1)
PY
    then
        echo "Error: MoonBit v$REQUIRED_MOONBIT+ required, found $ACTUAL_MOONBIT"
        echo "Visit https://www.moonbitlang.com/ to install or upgrade MoonBit."
        exit 1
    fi
    echo "MoonBit compiler version: $ACTUAL_MOONBIT (OK)"
fi

echo "Building MoonBit library..."
# moon build --target native compiles MoonBit to C (lib.c) then tries to link an
# executable.  For a library project (is-main: false) the link step always fails
# with "Undefined symbol: _main" — that is expected.  We only need the compiled
# C output, so ignore the linker error and compile lib.c manually below.
BUILD_STATUS=0
(cd "$MOONBIT_DIR" && moon build --target native) || BUILD_STATUS=$?

if [ ! -f "${BUILD_LIB_DIR}/lib.c" ]; then
    echo "Error: MoonBit native build did not produce ${BUILD_LIB_DIR}/lib.c"
    echo "moon build exited with status ${BUILD_STATUS}."
    echo "Run 'cd moonbit && moon check --target native' for the primary MoonBit error."
    exit 1
fi

if [ "$BUILD_STATUS" -ne 0 ]; then
    echo "MoonBit native build returned status ${BUILD_STATUS} after producing lib.c; continuing with static archive."
fi

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

if [ ! -f "${BUILD_RUNTIME_DIR}/runtime.o" ]; then
    echo "Error: MoonBit native build did not produce ${BUILD_RUNTIME_DIR}/runtime.o"
    exit 1
fi

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
