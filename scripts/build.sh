#!/usr/bin/env bash
# Build closetag: frontend (bun) -> embedded assets -> release binary.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "==> frontend: bun install + build"
(cd frontend && bun install --frozen-lockfile && bun run build)

echo "==> backend: cargo build --release"
# The gnu toolchain needs MinGW binutils (gcc/dlltool/ld) on PATH.
# This machine keeps a portable w64devkit at D:/dev/tools/w64devkit.
if [[ ":${PATH}:" != *":/d/dev/tools/w64devkit/bin:"* ]] && [[ -d /d/dev/tools/w64devkit/bin ]]; then
  echo "    (adding /d/dev/tools/w64devkit/bin to PATH)"
  export PATH="/d/dev/tools/w64devkit/bin:${PATH}"
fi
cargo build --release

echo "==> done: target/release/closetag"
