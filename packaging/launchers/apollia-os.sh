#!/usr/bin/env bash
#
# Apollia OS launcher — Linux / macOS.
#
# Ce script est inclus dans chaque archive de release (linux-* et macos-*).
# Il garantit que apollia-os trouve l'interprète Python 3.13 bundlé même si
# l'utilisateur n'a pas Python installé sur son système.
#
# Placement attendu (extrait de l'archive) :
#   apollia-os/
#   ├── apollia-os.sh        ← ce launcher
#   ├── apollia-os           ← le binaire
#   └── python/
#       ├── bin/python3.13   ← interprète bundlé
#       └── lib/...
#
# Usage :
#   ./apollia-os.sh start         # démarre le daemon
#   ./apollia-os.sh run <agent>   # toute commande apollia-os
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"

# 1. PYO3_PYTHON pointe vers l'interprète bundlé. Le binaire compilé via PyO3
#    avec linkage dynamique ouvre libpython3.13 relative à cet exécutable.
export PYO3_PYTHON="${HERE}/python/bin/python3.13"

if [[ ! -x "$PYO3_PYTHON" ]]; then
    echo "error: bundled Python missing at $PYO3_PYTHON" >&2
    echo "       l'archive est incomplète — re-télécharger depuis apollia.fr/download" >&2
    exit 1
fi

# 2. LD_LIBRARY_PATH (Linux) / DYLD_LIBRARY_PATH (macOS) pour que le loader
#    trouve libpython3.13.so / .dylib.
case "$(uname -s)" in
    Linux)
        export LD_LIBRARY_PATH="${HERE}/python/lib:${LD_LIBRARY_PATH:-}"
        ;;
    Darwin)
        # macOS : install_name du dylib a été ré-écrit côté packaging vers
        # @executable_path/../Resources/python/lib/… — fonctionne dans le bundle
        # Tauri, mais pour la CLI standalone on prepend explicitement.
        export DYLD_LIBRARY_PATH="${HERE}/python/lib:${DYLD_LIBRARY_PATH:-}"
        ;;
esac

# 3. APOLLIA_PYTHON_BUNDLE_DIR : variable optionnelle lue par le runtime pour
#    initialiser les venvs par agent en pointant pip vers l'interprète bundlé.
export APOLLIA_PYTHON_BUNDLE_DIR="${HERE}/python"

# 4. Exécute le binaire avec tous les arguments transmis.
exec "${HERE}/apollia-os" "$@"
