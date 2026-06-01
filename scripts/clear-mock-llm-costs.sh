#!/usr/bin/env bash
# Supprime les appels LLM mock injectés par mock-llm-costs.sh.
# Filtre sur task_id = 'mock_screenshot' - n'affecte aucun appel réel.

set -euo pipefail

DB="${APOLLIA_DATA_DIR:-$HOME/.apollia}/llm_calls.db"

if [[ ! -f "$DB" ]]; then
  echo "DB introuvable: $DB" >&2
  exit 1
fi

before=$(sqlite3 "$DB" "SELECT COUNT(*) FROM llm_calls WHERE task_id = 'mock_screenshot';")
sqlite3 "$DB" "DELETE FROM llm_calls WHERE task_id = 'mock_screenshot';"
echo "✓ $before ligne(s) mock supprimée(s) de $DB"
