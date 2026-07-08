#!/usr/bin/env bash
# Regenerate the TypeScript and Python host-driving clients from the OpenAPI spec.
#
# Usage:
#   bash clients/regen.sh              # generate from the committed clients/openapi.json
#   bash clients/regen.sh --from-daemon  # first refresh the spec from a running daemon
#
# The generators are build-time only (npm / pipx), not runtime dependencies of
# Apollia. Requires: npx (Node) for TypeScript, openapi-python-client for Python.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SPEC="$HERE/openapi.json"
PORT="${APOLLIA_TCP_PORT:-7771}"
TOKEN_FILE="${APOLLIA_TOKEN_FILE:-$HOME/.apollia/api-token}"

if [ "${1:-}" = "--from-daemon" ]; then
  echo "Refreshing spec from daemon on 127.0.0.1:${PORT}"
  token=""
  [ -f "$TOKEN_FILE" ] && token="$(tr -d '\n' <"$TOKEN_FILE")"
  curl -sf "http://127.0.0.1:${PORT}/api/v1/openapi.json" \
    ${token:+-H "Authorization: Bearer ${token}"} \
    -o "$SPEC"
  echo "Wrote $SPEC"
fi

if [ ! -f "$SPEC" ]; then
  echo "error: $SPEC not found. Run with --from-daemon against a live runtime first." >&2
  exit 1
fi

# --- TypeScript: types via openapi-typescript ---
echo "Generating TypeScript types -> clients/ts/src/schema.ts"
mkdir -p "$HERE/ts/src"
npx --yes openapi-typescript "$SPEC" -o "$HERE/ts/src/schema.ts"

# --- Python: full client via openapi-python-client ---
echo "Generating Python client -> clients/python"
if command -v openapi-python-client >/dev/null 2>&1; then
  GEN="openapi-python-client"
else
  GEN="uvx openapi-python-client"
fi
( cd "$HERE/python" && rm -rf apollia_runtime_client && \
  $GEN generate --path "$SPEC" --meta none --output-path apollia_runtime_client --overwrite )

echo "Done. Review the diff, then commit the regenerated clients."
