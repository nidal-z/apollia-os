#!/usr/bin/env bash
#
# reinstall-agents.sh — Désinstalle puis réinstalle tous les agents/packages
# locaux dans le daemon Apollia OS en cours d'exécution.
#
# Use case : tu modifies le SDK (sdk/apollia/...) ou un agent (agents/...) et
# tu veux re-tester en environnement propre, sans devoir te retaper "apollia
# agent install ..." pour chaque package. Le script :
#
#   1. Pré-nettoie les __pycache__ dans agents/ et le venv SDK source
#      (sinon PyO3 + pip peuvent réutiliser du bytecode périmé).
#   2. Désinstalle tous les packages installés via `agent package uninstall`
#      (cascade : sous-agents + triggers + venvs ~/.apollia/agents/packages/<name>).
#   3. Balaie les agents standalone éventuellement orphelins via `agent uninstall`.
#   4. Réinstalle chaque package via `agent install <dir>` (re-provisionne
#      automatiquement le venv pip avec les `packages` du manifest, et
#      réinstalle le SDK local en editable).
#
# Ce script NE touche PAS aux données utilisateur (mémoire agents, chat,
# profil). Pour ça → scripts/reset-agent-cache.sh.
#
# Les system agents (apollia-guide, onboarding-agent) sont bundled dans
# le binaire à la compilation et ne sont pas installables via CLI. Ils sont
# rafraîchis à chaque `apollia-os start`.
#
# Le daemon doit tourner (`apollia-os start`).
#
# Usage :
#   ./scripts/reinstall-agents.sh [options]
#
# Options :
#   --install-only        Skip phase uninstall (réinstalle seulement).
#   --uninstall-only      Skip phase install (désinstalle seulement).
#   --skip-tests          Passe --skip-tests à `agent install` (plus rapide,
#                         saute la suite eval de l'agent).
#   --only NAME[,NAME...] Limite aux packages listés (par nom dossier sans
#                         le préfixe agents/).
#   --release             Utilise target/release/apollia-os (défaut: debug).
#   --keep-pycache        Skip pré-nettoyage des __pycache__.
#   -h, --help            Cette aide.
#
# Env :
#   APOLLIA_BIN     Override du binaire (par défaut target/debug/apollia-os).
#   APOLLIA_SOCKET  Override du socket (par défaut /tmp/apollia.sock).

set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
SOCKET="${APOLLIA_SOCKET:-/tmp/apollia.sock}"
APOLLIA="${APOLLIA_BIN:-$REPO/target/debug/apollia-os}"

# Packages installables (chemins relatifs au repo root, chacun avec agent.toml).
PACKAGES=(
    "agents/archive-worker"
    "agents/chart-worker"
    "agents/docx-worker"
    "agents/pdf-worker"
    "agents/xlsx-worker"
    "agents/veille-ia"
    "agents/_test_forge/email-triage"
    "agents/_test_forge/code-review-multi"
    "agents/_test_forge/markdown-summarizer"
)

# ─── Args ──────────────────────────────────────────────────────────────────────
do_uninstall=true
do_install=true
skip_tests=false
keep_pycache=false
filter=""

while [ $# -gt 0 ]; do
    case "$1" in
        --install-only)   do_uninstall=false ;;
        --uninstall-only) do_install=false ;;
        --skip-tests)     skip_tests=true ;;
        --keep-pycache)   keep_pycache=true ;;
        --release)        APOLLIA="$REPO/target/release/apollia-os" ;;
        --only)
            shift
            filter="$1"
            ;;
        -h|--help)
            sed -n '2,44p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "Unknown arg: $1" >&2
            exit 1
            ;;
    esac
    shift
done

# ─── Prerequisites ─────────────────────────────────────────────────────────────
if [ ! -x "$APOLLIA" ]; then
    echo "ERROR: binary not found or not executable: $APOLLIA" >&2
    echo "       run 'cargo build' (or 'cargo build --release' + --release)" >&2
    exit 1
fi

if [ ! -S "$SOCKET" ]; then
    echo "ERROR: daemon socket absent ($SOCKET) — start it with:" >&2
    echo "       $APOLLIA start" >&2
    exit 1
fi

apollia() { "$APOLLIA" --socket "$SOCKET" "$@"; }

# Apply --only filter.
selected=()
if [ -n "$filter" ]; then
    IFS=',' read -ra wanted <<< "$filter"
    for pkg in "${PACKAGES[@]}"; do
        for w in "${wanted[@]}"; do
            if [ "$(basename "$pkg")" = "$w" ]; then
                selected+=("$pkg")
                break
            fi
        done
    done
    if [ ${#selected[@]} -eq 0 ]; then
        echo "ERROR: --only matched 0 packages (available: $(printf '%s ' "${PACKAGES[@]}" | xargs -n1 basename | tr '\n' ' '))" >&2
        exit 1
    fi
else
    selected=("${PACKAGES[@]}")
fi

echo "=== Apollia — reinstall agents ==="
echo "Binary : $APOLLIA"
echo "Socket : $SOCKET"
echo "Targets: $(printf '%s ' "${selected[@]}" | xargs -n1 basename | tr '\n' ' ')"
echo

# ─── [1/4] Pré-nettoyage __pycache__ ──────────────────────────────────────────
if ! $keep_pycache; then
    echo "[1/4] Wiping __pycache__ in agents/ and sdk/..."
    find "$REPO/agents" "$REPO/sdk" -type d -name "__pycache__" \
        -exec rm -rf {} + 2>/dev/null || true
else
    echo "[1/4] (skipped — --keep-pycache)"
fi

# ─── [2/4] Désinstallation ─────────────────────────────────────────────────────
if $do_uninstall; then
    echo "[2/4] Uninstalling packages..."

    # Liste les packages actuellement installés via JSON.
    installed_pkgs=$(apollia agent package list --json 2>/dev/null \
        | /usr/bin/python3 -c '
import json, sys
try:
    data = json.load(sys.stdin)
    for p in data.get("packages", []):
        print(p["name"])
except Exception:
    pass
' || true)

    # Si --only, restreint au filtre.
    if [ -n "$filter" ]; then
        wanted_names=""
        for pkg in "${selected[@]}"; do
            wanted_names="$wanted_names $(basename "$pkg")"
        done
        filtered_pkgs=""
        for p in $installed_pkgs; do
            for w in $wanted_names; do
                # Le nom du package TOML peut différer du dossier — on garde
                # tout match qui correspond.
                if [ "$p" = "$w" ]; then
                    filtered_pkgs="$filtered_pkgs $p"
                fi
            done
        done
        installed_pkgs="$filtered_pkgs"
    fi

    if [ -z "$installed_pkgs" ]; then
        echo "      (no installed packages match)"
    else
        for pkg_name in $installed_pkgs; do
            echo "      → uninstall package: $pkg_name"
            apollia agent package uninstall "$pkg_name" --quiet 2>&1 \
                | sed 's/^/        /' || echo "        WARN: failed (continuing)"
        done
    fi

    # Standalone leftovers (agents installés directement sans package wrapper).
    # Probablement aucun pour notre layout, mais on balaie par sécurité.
    if [ -z "$filter" ]; then
        leftover_agents=$(apollia agent list --json 2>/dev/null \
            | /usr/bin/python3 -c '
import json, sys
try:
    data = json.load(sys.stdin)
    # Ne touche pas aux system agents bundled (apollia-guide, onboarding-agent).
    system = {"apollia-guide", "onboarding-agent"}
    for a in data.get("agents", []):
        if a.get("installed") and a["name"] not in system:
            print(a["name"])
except Exception:
    pass
' || true)
        if [ -n "$leftover_agents" ]; then
            echo "      Leftover standalone agents:"
            for name in $leftover_agents; do
                echo "      → uninstall agent: $name"
                apollia agent uninstall "$name" --quiet 2>&1 \
                    | sed 's/^/        /' || true
            done
        fi
    fi
else
    echo "[2/4] (skipped — --uninstall-only off)"
fi

# ─── [3/4] Installation ────────────────────────────────────────────────────────
if $do_install; then
    echo "[3/4] Installing packages..."
    install_flags=()
    $skip_tests && install_flags+=(--skip-tests)

    failed=()
    for pkg_path in "${selected[@]}"; do
        abs_path="$REPO/$pkg_path"
        if [ ! -d "$abs_path" ]; then
            echo "      SKIP $pkg_path : directory missing"
            continue
        fi
        echo "      → install $(basename "$pkg_path")"
        if apollia agent install "${install_flags[@]}" "$abs_path" --quiet 2>&1 \
            | sed 's/^/        /'; then
            :
        else
            failed+=("$pkg_path")
        fi
    done

    if [ ${#failed[@]} -gt 0 ]; then
        echo
        echo "FAILED installs:"
        printf '  %s\n' "${failed[@]}"
        exit 1
    fi
else
    echo "[3/4] (skipped — --install-only off)"
fi

# ─── [4/4] Récap ───────────────────────────────────────────────────────────────
echo "[4/4] Current state:"
apollia agent package list 2>&1 | sed 's/^/      /' || true

echo
echo "Done."
