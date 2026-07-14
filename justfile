# Apollia OS - Build & Dev tasks
# Usage: just <recipe>
# Install just: cargo install just

set shell := ["bash", "-euo", "pipefail", "-c"]

# Defaults (override per command: `just <recipe> var=value`)
desktop_runners := "cpu metal"
macos_target := "aarch64-apple-darwin"

# -----------------------------------------------------------------------------
# Documentation
# -----------------------------------------------------------------------------

# Generate Rust API docs (rustdoc)
rustdoc:
    @echo "→ Génération rustdoc..."
    cargo doc --no-deps --workspace --document-private-items
    @echo "✅ rustdoc → target/doc/"

# Regenerate the site's machine references (CLI / API / SDK) from the code
docs-regen:
    bash docs/site/regen.sh

# Build the public documentation site (Docusaurus, en + fr)
docs:
    cd docs/site && npm ci && npm run build
    @echo "✅ Site → docs/site/build/"

# -----------------------------------------------------------------------------
# Rust workspace
# -----------------------------------------------------------------------------

# Full workspace build
build:
    cargo build --workspace

# Release workspace build
build-release:
    cargo build --workspace --release

# Full tests
test:
    cargo test --workspace

# Tests with Python features
test-python:
    PYO3_PYTHON=/opt/homebrew/bin/python3.13 cargo test --workspace --features python-tests

# Full lint
lint:
    cargo fmt --check
    cargo clippy --workspace -- -D warnings

# Format code
fmt:
    cargo fmt --all

# -----------------------------------------------------------------------------
# Runner sidecar
# -----------------------------------------------------------------------------

# Build debug runner for one backend and keep unsuffixed binary
runner-debug backend:
    cargo build -p apollia-runner --features local-{{backend}}

# Build release runner for one backend and keep unsuffixed binary
runner-release backend target="":
    if [ -n "{{target}}" ]; then cargo build -p apollia-runner --release --target "{{target}}" --features local-{{backend}}; else cargo build -p apollia-runner --release --features local-{{backend}}; fi

# Build + suffix debug runner binary so daemon can auto-detect it
runner-debug-suffixed backend:
    cargo build -p apollia-runner --features local-{{backend}}
    cp "target/debug/apollia-runner" "target/debug/apollia-runner-{{backend}}"
    chmod +x "target/debug/apollia-runner-{{backend}}" || true
    # On macOS, `cp` invalidates the linker-signed adhoc signature, so the
    # kernel SIGKILLs the copy at launch ("Code Signature Invalid"). Re-sign
    # adhoc to restore execution.
    if [ "$(uname)" = "Darwin" ]; then codesign --force --sign - "target/debug/apollia-runner-{{backend}}"; fi

# macOS dev defaults: Metal + CPU fallback
runners-dev-macos:
    just runner-debug-suffixed metal
    just runner-debug-suffixed cpu

# -----------------------------------------------------------------------------
# Desktop (Tauri)
# -----------------------------------------------------------------------------

# Install desktop frontend dependencies
desktop-ui-install:
    cd crates/apollia-desktop/ui && npm ci

# Run desktop in dev mode (expects runners in target/debug/)
desktop-dev:
    cd crates/apollia-desktop && cargo tauri dev

# macOS dev shortcut: ensure metal+cpu runners then start desktop
desktop-dev-macos: runners-dev-macos
    cd crates/apollia-desktop && RUST_LOG=debug cargo tauri dev

# Build desktop bundle (uses bundle-cli.sh + APOLLIA_DESKTOP_RUNNERS)
desktop-build target="{{macos_target}}" runners="{{desktop_runners}}":
    cd crates/apollia-desktop && APOLLIA_DESKTOP_RUNNERS="{{runners}}" cargo tauri build --target "{{target}}"

# Build desktop bundle for current host target
desktop-build-host runners="{{desktop_runners}}":
    cd crates/apollia-desktop && APOLLIA_DESKTOP_RUNNERS="{{runners}}" cargo tauri build

# -----------------------------------------------------------------------------
# CLI / release helpers
# -----------------------------------------------------------------------------

cli-build:
    cargo build -p apollia-cli

cli-release target="":
    if [ -n "{{target}}" ]; then cargo build -p apollia-cli --release --target "{{target}}"; else cargo build -p apollia-cli --release; fi

# Common release presets
release-macos:
    just cli-release target={{macos_target}}
    just desktop-build target={{macos_target}} runners="cpu metal"

release-linux:
    just cli-release target=x86_64-unknown-linux-gnu
    just desktop-build target=x86_64-unknown-linux-gnu runners="cpu"

release-windows:
    just cli-release target=x86_64-pc-windows-msvc
    just desktop-build target=x86_64-pc-windows-msvc runners="cpu"

# -----------------------------------------------------------------------------
# Combined tasks
# -----------------------------------------------------------------------------

# Local CI: lint + tests
ci: lint test
    @echo "✅ CI locale passée"

# Clean generated artifacts
clean:
    cargo clean
    @echo "✅ Artefacts nettoyés"
