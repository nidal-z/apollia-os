#!/usr/bin/env bash
# Injecte des appels LLM fictifs (OpenAI + Anthropic + local) sur les 7 derniers
# jours dans ~/.apollia/llm_calls.db pour générer un screenshot du graphique
# "Coûts LLM" (Observability → LLM Costs) sans payer de vrais appels.
#
# Chaque ligne insérée est taguée avec task_id = 'mock_screenshot' pour pouvoir
# être supprimée proprement via scripts/clear-mock-llm-costs.sh.
#
# Usage :
#   ./scripts/mock-llm-costs.sh                 # 7 jours par défaut
#   ./scripts/mock-llm-costs.sh --days 30       # 30 jours
#   ./scripts/mock-llm-costs.sh --days 90       # 90 jours
#   ./scripts/clear-mock-llm-costs.sh           # nettoie
#
# Le runtime doit être arrêté (ou tolérer un write concurrent — WAL est activé).

set -euo pipefail

# Number of days to backfill (default 7). Pass --days N to override.
DAYS=7
while [[ $# -gt 0 ]]; do
  case "$1" in
    --days)
      DAYS="$2"; shift 2 ;;
    --days=*)
      DAYS="${1#*=}"; shift ;;
    *)
      echo "Argument inconnu: $1" >&2; exit 2 ;;
  esac
done

DB="${APOLLIA_DATA_DIR:-$HOME/.apollia}/llm_calls.db"

if [[ ! -f "$DB" ]]; then
  echo "DB introuvable: $DB" >&2
  echo "Lance l'app Apollia au moins une fois pour qu'elle soit créée." >&2
  exit 1
fi

echo "→ Injection des coûts mock dans $DB"

# Tableau : "backend|model|prix_in_per_1k|prix_out_per_1k"
# Prix réalistes (mai 2026) — purement indicatifs pour le mock.
BACKENDS=(
  "openai|gpt-4o|0.0025|0.01"
  "openai|gpt-4o-mini|0.00015|0.0006"
  "anthropic|claude-sonnet-4-20250514|0.003|0.015"
  "anthropic|claude-haiku-4-5-20251001|0.0008|0.004"
  "local|qwen3-30b-q4|0|0"
)

# Profil journalier : multiplicateur d'activité (variation pour relief).
# Si DAYS > nombre de poids prédéfinis, on cycle dessus.
DAY_WEIGHTS=(0.4 0.7 1.0 0.5 1.3 0.9 1.1 0.3 0.8 1.2 0.6 1.4 0.5 0.9)

sql_batch=""

for ((day_idx=0; day_idx<DAYS; day_idx++)); do
  day_offset=$((DAYS - 1 - day_idx))
  weight="${DAY_WEIGHTS[$((day_idx % ${#DAY_WEIGHTS[@]}))]}"

  for entry in "${BACKENDS[@]}"; do
    IFS='|' read -r backend model price_in price_out <<< "$entry"

    # Nombre d'appels par jour/backend (entre 3 et 12, modulé par weight).
    call_count=$(awk -v w="$weight" 'BEGIN { srand(); print int(3 + w * (rand() * 10)) }')

    for ((i = 0; i < call_count; i++)); do
      # Tokens réalistes — varient par modèle.
      case "$backend" in
        openai|anthropic)
          prompt_tokens=$(awk 'BEGIN { srand(); print int(800 + rand() * 4000) }')
          completion_tokens=$(awk 'BEGIN { srand(); print int(200 + rand() * 1200) }')
          ;;
        local)
          prompt_tokens=$(awk 'BEGIN { srand(); print int(500 + rand() * 3000) }')
          completion_tokens=$(awk 'BEGIN { srand(); print int(100 + rand() * 800) }')
          ;;
      esac

      cost=$(awk -v pt="$prompt_tokens" -v ct="$completion_tokens" \
               -v pi="$price_in" -v po="$price_out" \
               'BEGIN { printf "%.6f", (pt / 1000.0) * pi + (ct / 1000.0) * po }')

      latency=$(awk 'BEGIN { srand(); print int(300 + rand() * 2500) }')

      # Heure aléatoire dans la journée.
      hour=$(awk 'BEGIN { srand(); print int(rand() * 24) }')
      minute=$(awk 'BEGIN { srand(); print int(rand() * 60) }')
      second=$(awk 'BEGIN { srand(); print int(rand() * 60) }')

      created_at=$(date -u -v-${day_offset}d -v${hour}H -v${minute}M -v${second}S \
                       +"%Y-%m-%dT%H:%M:%S.000Z" 2>/dev/null \
                   || date -u -d "${day_offset} days ago ${hour}:${minute}:${second}" \
                       +"%Y-%m-%dT%H:%M:%S.000Z")

      id=$(uuidgen | tr '[:upper:]' '[:lower:]')

      sql_batch+="INSERT INTO llm_calls (id, task_id, step_id, backend, model, prompt_tokens, completion_tokens, cost_usd, latency_ms, prompt_text, completion_text, created_at) VALUES ('$id', 'mock_screenshot', NULL, '$backend', '$model', $prompt_tokens, $completion_tokens, $cost, $latency, NULL, NULL, '$created_at');"$'\n'
    done
  done
done

echo "$sql_batch" | sqlite3 "$DB"

echo "✓ Mock injecté ($DAYS jours). Résumé par backend :"
sqlite3 -column -header "$DB" <<SQL
SELECT backend,
       COUNT(*)                       AS calls,
       printf('\$%.2f', SUM(cost_usd)) AS total_cost
FROM llm_calls
WHERE created_at >= datetime('now', '-$DAYS days')
GROUP BY backend
ORDER BY backend;
SQL

echo ""
echo "→ Ouvre Apollia Desktop → Observability → LLM Costs pour voir le graphique."
echo "→ Pour nettoyer : ./scripts/clear-mock-llm-costs.sh"
