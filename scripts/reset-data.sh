#!/usr/bin/env bash
# reset-data.sh — Remet ~/.apollia/ dans l'état d'une installation fraîche.
#
# Usage :
#   ./scripts/reset-data.sh          # Reset complet (conserve models/)
#   ./scripts/reset-data.sh --all    # Reset complet incluant models/
#   ./scripts/reset-data.sh --keep-config  # Conserve apollia.toml
#
# Conservé par défaut :
#   - models/  (GGUF/Whisper — téléchargements lourds)
#
# Supprimé :
#   - Toutes les bases SQLite (17 DBs + WAL + SHM)
#   - Tous les agents installés (y compris onboarding-agent, re-extrait au prochain lancement)
#   - Mémoire, rapports, pipelines, venvs
#   - Configuration (apollia.toml, api-token, mcp-registry.json, .onboarded)
#   - Socket Unix (/tmp/apollia.sock)
#   - Coûts session (session_costs.jsonl)

set -euo pipefail

APOLLIA_DIR="$HOME/.apollia"
REMOVE_MODELS=false
KEEP_CONFIG=false

for arg in "$@"; do
  case "$arg" in
    --all)         REMOVE_MODELS=true ;;
    --keep-config) KEEP_CONFIG=true ;;
    -h|--help)
      echo "Usage: $0 [--all] [--keep-config]"
      echo "  --all          Supprimer aussi models/ (GGUF, Whisper)"
      echo "  --keep-config  Conserver apollia.toml"
      exit 0
      ;;
    *) echo "Option inconnue : $arg"; exit 1 ;;
  esac
done

if [ ! -d "$APOLLIA_DIR" ]; then
  echo "Rien à nettoyer : $APOLLIA_DIR n'existe pas."
  exit 0
fi

echo "╔══════════════════════════════════════════╗"
echo "║  Apollia OS — Reset données locales      ║"
echo "╚══════════════════════════════════════════╝"
echo ""

# ─── 1. Bases de données SQLite ──────────────────────────────────────────────

echo "1/7  Bases de données SQLite..."
DB_COUNT=0
for f in "$APOLLIA_DIR"/*.db "$APOLLIA_DIR"/*.db-shm "$APOLLIA_DIR"/*.db-wal; do
  [ -f "$f" ] && rm -f "$f" && DB_COUNT=$((DB_COUNT + 1))
done
echo "     $DB_COUNT fichiers supprimés"

# ─── 2. Agents installés ─────────────────────────────────────────────────────

echo "2/7  Agents installés..."
if [ -d "$APOLLIA_DIR/agents" ]; then
  AGENT_COUNT=0
  for agent_dir in "$APOLLIA_DIR"/agents/*/; do
    [ -d "$agent_dir" ] || continue
    agent_name=$(basename "$agent_dir")
    rm -rf "$agent_dir"
    echo "     supprimé : $agent_name"
    AGENT_COUNT=$((AGENT_COUNT + 1))
  done
  # Supprimer aussi registry.json (liste des agents communautaires installés)
  rm -f "$APOLLIA_DIR/agents/community/registry.json"
  echo "     $AGENT_COUNT agents supprimés"
else
  echo "     aucun agent"
fi

# ─── 3. Mémoire (namespaces + user_memory) ──────────────────────────────────

echo "3/7  Mémoire..."
if [ -d "$APOLLIA_DIR/memory" ]; then
  MEM_COUNT=$(find "$APOLLIA_DIR/memory" -name "*.db*" 2>/dev/null | wc -l | tr -d ' ')
  rm -rf "$APOLLIA_DIR/memory"
  echo "     $MEM_COUNT fichiers mémoire supprimés"
else
  echo "     aucune mémoire"
fi

# ─── 4. Pipelines, venvs, rapports ──────────────────────────────────────────

echo "4/7  Pipelines, venvs, rapports..."
for subdir in pipelines venvs reports; do
  if [ -d "$APOLLIA_DIR/$subdir" ]; then
    rm -rf "$APOLLIA_DIR/$subdir"
    echo "     supprimé : $subdir/"
  fi
done

# ─── 5. Configuration et flags ───────────────────────────────────────────────

echo "5/7  Configuration et flags..."
# Toujours supprimer : onboarding flag, api token, MCP data, costs
rm -f "$APOLLIA_DIR/.onboarded"
rm -f "$APOLLIA_DIR/api-token"
rm -f "$APOLLIA_DIR/mcp-registry.json"
rm -f "$APOLLIA_DIR/mcp.toml"
rm -f "$APOLLIA_DIR/session_costs.jsonl"

if [ "$KEEP_CONFIG" = true ]; then
  echo "     apollia.toml conservé (--keep-config)"
else
  rm -f "$APOLLIA_DIR/apollia.toml"
  echo "     apollia.toml supprimé"
fi

# ─── 6. Socket Unix ──────────────────────────────────────────────────────────

echo "6/7  Socket Unix..."
if [ -S /tmp/apollia.sock ]; then
  rm -f /tmp/apollia.sock
  echo "     /tmp/apollia.sock supprimé"
else
  echo "     pas de socket actif"
fi
# Nettoyer aussi le lock d'update si présent
rm -f /tmp/apollia-update.lock

# ─── 7. Modèles (optionnel) ─────────────────────────────────────────────────

echo "7/7  Modèles..."
if [ "$REMOVE_MODELS" = true ]; then
  if [ -d "$APOLLIA_DIR/models" ]; then
    SIZE=$(du -sh "$APOLLIA_DIR/models" 2>/dev/null | cut -f1)
    rm -rf "$APOLLIA_DIR/models"
    echo "     models/ supprimé ($SIZE libérés)"
  else
    echo "     aucun modèle"
  fi
else
  if [ -d "$APOLLIA_DIR/models" ]; then
    SIZE=$(du -sh "$APOLLIA_DIR/models" 2>/dev/null | cut -f1)
    echo "     models/ conservé ($SIZE) — utiliser --all pour supprimer"
  else
    echo "     aucun modèle"
  fi
fi

# ─── Résumé ──────────────────────────────────────────────────────────────────

echo ""
echo "════════════════════════════════════════════"
echo "Reset terminé. État final de $APOLLIA_DIR :"
echo ""
if [ -d "$APOLLIA_DIR" ]; then
  ls -la "$APOLLIA_DIR" 2>/dev/null || echo "(vide)"
else
  echo "(répertoire supprimé)"
fi
echo ""
echo "Au prochain lancement du Desktop :"
echo "  - L'onboarding se relancera"
echo "  - L'onboarding-agent sera ré-extrait automatiquement"
echo "  - Les bases SQLite seront recréées vides"
