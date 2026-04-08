#!/usr/bin/env bash
# Compile the apollia-os CLI binary and stage it for Tauri resource bundling,
# then build the Svelte frontend.  Called by `beforeBuildCommand` in
# tauri.conf.json during `cargo tauri build`.
set -euo pipefail

TARGET="${TAURI_TARGET_TRIPLE:-$(rustc -vV | grep host | awk '{print $2}')}"
REPO_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
STAGING="$(dirname "$0")/../resources"

echo "==> Building apollia-cli for target ${TARGET}..."
mkdir -p "$STAGING"
cargo build -p apollia-cli --release --target "$TARGET" --manifest-path "$REPO_ROOT/Cargo.toml"
cp "$REPO_ROOT/target/${TARGET}/release/apollia-os" "$STAGING/apollia-os"
echo "==> CLI binary staged at ${STAGING}/apollia-os"

echo "==> Building Svelte frontend..."
cd "$(dirname "$0")/../ui" && npm run build
echo "==> Done."
