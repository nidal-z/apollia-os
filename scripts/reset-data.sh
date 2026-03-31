#!/usr/bin/env bash
# reset-data.sh — Remet ~/.apollia/ dans l'état d'une installation fraîche.
# Conserve : models/ (téléchargements lourds) et agents/onboarding-agent/ (built-in).

set -euo pipefail

APOLLIA_DIR="$HOME/.apollia"

echo "Suppression des bases de données..."
rm -f "$APOLLIA_DIR"/*.db "$APOLLIA_DIR"/*.db-shm "$APOLLIA_DIR"/*.db-wal

echo "Suppression des agents de test..."
for agent_dir in "$APOLLIA_DIR"/agents/*/; do
  agent_name=$(basename "$agent_dir")
  if [[ "$agent_name" != "onboarding-agent" ]]; then
    rm -rf "$agent_dir"
    echo "  supprimé : $agent_name"
  fi
done

echo "Nettoyage mémoire, rapports, config..."
rm -rf "$APOLLIA_DIR"/memory/*
rm -rf "$APOLLIA_DIR"/reports/*
rm -f  "$APOLLIA_DIR"/apollia.toml
rm -f  "$APOLLIA_DIR"/mcp-registry.json

echo ""
echo "Reset terminé. État final :"
ls "$APOLLIA_DIR"
