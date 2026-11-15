#!/usr/bin/env bash
# Argument regressions only: no macOS tools, build, signing or model download is needed.
# Run with: bash packaging/macos/test-arguments.sh
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEST_WORK="$(mktemp -d "${TMPDIR:-/tmp}/pdfcraft-macos-arguments.XXXXXX")"
trap 'rm -rf -- "$TEST_WORK"' EXIT

# env.sh must not read a checkout's version or Git state, or create a shared output directory.
export PDFCRAFT_VERSION=0.0.0-test PDFCRAFT_BUILD_SHA=argument-test
export PDFCRAFT_BUILD_DATE=2000-01-01 PDFCRAFT_BUILD_LABEL=
export DIST="$TEST_WORK/dist" CARGO_TARGET_DIR="$TEST_WORK/target"
EXPECTED='error: --arch needs universal, aarch64 or x86_64'

check_missing_arch() {
  local status=0
  "$BASH" "$HERE/package.sh" "$@" >"$TEST_WORK/stdout" 2>"$TEST_WORK/stderr" || status=$?
  if [[ "$status" -ne 2 ]] || ! grep -Fxq "$EXPECTED" "$TEST_WORK/stderr"; then
    cat "$TEST_WORK/stderr" >&2
    echo "expected missing --arch value diagnostic and exit 2, got $status" >&2
    exit 1
  fi
}

check_missing_arch --arch
check_missing_arch --skip-build --arch
check_missing_arch --arch --skip-build
for arch in universal aarch64 x86_64; do
  "$BASH" "$HERE/package.sh" --arch "$arch" --help >"$TEST_WORK/help"
done
echo 'macOS packaging argument checks passed'
