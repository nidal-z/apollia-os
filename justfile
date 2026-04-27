# Apollia OS — Build & Dev tasks
# Usage: just <recette>
# Installer just: cargo install just

# ─── Documentation ───────────────────────────────────────────────────────────

# Générer tous les SVGs depuis les fichiers .puml
diagrams:
    @echo "→ Génération des diagrammes PlantUML..."
    @mkdir -p book/src/appendix-a-diagrams
    @if command -v plantuml > /dev/null 2>&1; then \
        plantuml -tsvg -o "$(pwd)/book/src/appendix-a-diagrams/" docs/diagrams/*.puml && \
        echo "✅ SVGs générés dans book/src/appendix-a-diagrams/"; \
    elif [ -f ~/.local/bin/plantuml.jar ]; then \
        java -jar ~/.local/bin/plantuml.jar -tsvg -o "$(pwd)/book/src/appendix-a-diagrams/" docs/diagrams/*.puml && \
        echo "✅ SVGs générés depuis plantuml.jar"; \
    else \
        echo "⚠️  plantuml non trouvé — installez-le avec: brew install plantuml"; \
        echo "   Les SVGs existants sont conservés."; \
    fi

# Générer la documentation API Rust (rustdoc)
rustdoc:
    @echo "→ Génération rustdoc..."
    cargo doc --no-deps --workspace --document-private-items
    @echo "✅ rustdoc → target/doc/"

# Générer l'index des ADRs automatiquement
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
    } > book/src/decisions/index.md
    @echo "✅ Index ADR → book/src/decisions/index.md"

# Builder le book mdBook
book:
    @echo "→ Build mdBook..."
    @command -v mdbook > /dev/null 2>&1 || (echo "❌ mdbook non installé: cargo install mdbook" && exit 1)
    mdbook build book/
    @echo "✅ Book → target/book/"

# Build complet de la documentation (ordre: diagrams → adr-index → book)
docs: diagrams adr-index book
    @echo ""
    @echo "✅ Documentation complète générée"
    @echo "   Book : target/book/index.html"
    @echo "   API  : target/doc/apollia_core/index.html"

# Serveur de développement avec hot-reload
dev:
    @echo "→ Démarrage du serveur de dev..."
    @command -v mdbook > /dev/null 2>&1 || (echo "❌ mdbook non installé: cargo install mdbook" && exit 1)
    mdbook serve book/ --open

# Vérifier les includes cassés dans book/src/
check-includes:
    @python3 scripts/check-includes.py

# ─── Rust ────────────────────────────────────────────────────────────────────

# Build complet du workspace
build:
    cargo build --workspace

# Tests complets
test:
    cargo test --workspace

# Tests avec features Python
test-python:
    PYO3_PYTHON=/opt/homebrew/bin/python3.13 cargo test --workspace --features python-tests

# Lint complet
lint:
    cargo fmt --check
    cargo clippy --workspace -- -D warnings

# Formater le code
fmt:
    cargo fmt --all

# Build Desktop app with bundled CLI (production .dmg)
build-desktop:
    cd crates/apollia-desktop && cargo tauri build

# ─── Tâches combinées ────────────────────────────────────────────────────────

# CI locale : lint + tests + doc
ci: lint test docs check-includes
    @echo "✅ CI locale passée"

# Nettoyage des artefacts générés (SVGs préservés)
clean:
    cargo clean
    rm -rf target/book/
    @echo "✅ Artefacts nettoyés"

# Nettoyage complet incluant les SVGs générés
clean-all: clean
    rm -f book/src/appendix-a-diagrams/*.svg
    @echo "✅ Nettoyage complet"
