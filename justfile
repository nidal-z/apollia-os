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

# Generate all SVGs from .puml files
diagrams:
    @echo "→ Génération des diagrammes PlantUML..."
    @mkdir -p docs/book/src/appendix-a-diagrams
    @if command -v plantuml > /dev/null 2>&1; then \
        plantuml -tsvg -o "$(pwd)/book/src/appendix-a-diagrams/" docs/diagrams/*.puml && \
        echo "✅ SVGs générés dans docs/book/src/appendix-a-diagrams/"; \
    elif [ -f ~/.local/bin/plantuml.jar ]; then \
        java -jar ~/.local/bin/plantuml.jar -tsvg -o "$(pwd)/book/src/appendix-a-diagrams/" docs/diagrams/*.puml && \
        echo "✅ SVGs générés depuis plantuml.jar"; \
    else \
        echo "⚠️  plantuml non trouvé - installez-le avec: brew install plantuml"; \
        echo "   Les SVGs existants sont conservés."; \
    fi

# Generate Rust API docs (rustdoc)
rustdoc:
    @echo "→ Génération rustdoc..."
    cargo doc --no-deps --workspace --document-private-items
    @echo "✅ rustdoc → target/doc/"

# Generate ADR index automatically
adr-index:
    @echo "→ Génération de l'index ADR..."
    @{ \
        echo "# Décisions Architecturales (ADR)"; \
        echo ""; \
        echo "> Registre de toutes les décisions significatives."; \
        echo ""; \
        echo "| ADR | Titre | Statut |"; \
        echo "|---|---|---|"; \
        for f in docs/adr/ADR-[0-9]*.md; do \
            num=$$(basename "$$f" .md | grep -o 'ADR-[0-9]*'); \
            title=$$(grep '^# ' "$$f" | head -1 | sed 's/^# //'); \
            slug=$$(basename "$$f" .md | tr '[:upper:]' '[:lower:]'); \
            echo "| $$num | [$$title](./$$slug.md) | Accepté |"; \
        done; \
    } > docs/book/src/decisions/index.md
    @echo "✅ Index ADR → docs/book/src/decisions/index.md"

# Build mdBook
book:
    @echo "→ Build mdBook..."
    @command -v mdbook > /dev/null 2>&1 || (echo "❌ mdbook non installé: cargo install mdbook" && exit 1)
    mdbook build docs/book/
    @echo "✅ Book → target/book/"

# Full docs build (order: diagrams -> adr-index -> book)
docs: diagrams adr-index book
    @echo ""
    @echo "✅ Documentation complète générée"
    @echo "   Book : target/book/index.html"
    @echo "   API  : target/doc/apollia_core/index.html"

# Hot-reload docs server
dev:
    @echo "→ Démarrage du serveur de dev..."
    @command -v mdbook > /dev/null 2>&1 || (echo "❌ mdbook non installé: cargo install mdbook" && exit 1)
    mdbook serve docs/book/ --open

# Check broken includes in docs/book/src/
check-includes:
    @python3 scripts/check-includes.py

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
# Runner sidecar (ADR-113)
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

# Local CI: lint + tests + docs
ci: lint test docs check-includes
    @echo "✅ CI locale passée"

# Clean generated artifacts (keep SVGs)
clean:
    cargo clean
    rm -rf target/book/
    @echo "✅ Artefacts nettoyés"

# Full clean including generated SVGs
clean-all: clean
    rm -f docs/book/src/appendix-a-diagrams/*.svg
    @echo "✅ Nettoyage complet"
