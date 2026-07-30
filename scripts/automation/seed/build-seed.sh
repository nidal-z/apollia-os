#!/usr/bin/env bash
# Build an isolated, deterministic Apollia data ecosystem for the automation
# verification suite. Produces a throwaway HOME whose `.apollia` is fully seeded
# (SQLite DBs + agents + memory + models + config), so the det scripts find data
# without touching the real ~/.apollia profile.
#
# Usage:
#   scripts/automation/seed/build-seed.sh [SEED_HOME]
# SEED_HOME defaults to $PWD/.apollia-seed-home. The app is then launched with
# HOME=$SEED_HOME (build toolchain env preserved) by the just recipe.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd "$HERE/../../.." >/dev/null 2>&1 && pwd)"
SEED_HOME="${1:-$PWD/.apollia-seed-home}"
DATA="$SEED_HOME/.apollia"
CFG="$SEED_HOME/.config/apollia"

# Expand the seed placeholders on stdin. Keeps absolute paths out of the
# checked-in fragments while the seeded rows still point at real locations.
expand_seed_paths() {
  sed -e "s|__APOLLIA_SEED_WORKSPACE__|$REPO_ROOT|g" \
      -e "s|__APOLLIA_SEED_HOME__|$SEED_HOME|g"
}

echo "==> seed HOME: $SEED_HOME"
rm -rf "$SEED_HOME"
mkdir -p "$DATA" "$CFG" "$DATA/agents" "$DATA/memory" "$DATA/models" "$DATA/venvs"

# 1) Databases: schema then fragment (fragment is INSERTs only).
#    A DB with a schema but no fragment is created empty (still valid).
#    `sqlite_sequence` is a reserved internal table SQLite auto-manages; the
#    schema dumps include its CREATE line, which errors on replay, so strip it.
#
#    Fragments carry machine-independent placeholders so the checked-in seed
#    holds no absolute path from whoever recorded it. They are expanded here:
#      __APOLLIA_SEED_WORKSPACE__  the repository checkout (seeded project and
#                                  governance rows point at real files)
#      __APOLLIA_SEED_HOME__       the throwaway HOME for this run
for schema in "$HERE"/schemas/*.sql; do
  db="$(basename "$schema" .sql)"
  frag="$HERE/fragments/$db.sql"
  echo "==> db: $db.db"
  grep -v 'CREATE TABLE sqlite_sequence' "$schema" | sqlite3 "$DATA/$db.db"
  if [ -f "$frag" ]; then
    expand_seed_paths < "$frag" | sqlite3 "$DATA/$db.db"
  fi
done

# 2) On-disk files (agents, memory, models, mcp registry). Copied verbatim.
[ -d "$HERE/files/agents" ] && cp -R "$HERE/files/agents/." "$DATA/agents/"
[ -d "$HERE/files/memory" ] && cp -R "$HERE/files/memory/." "$DATA/memory/"
[ -d "$HERE/files/models" ] && cp -R "$HERE/files/models/." "$DATA/models/"
[ -f "$HERE/files/mcp-registry.json" ] && cp "$HERE/files/mcp-registry.json" "$DATA/mcp-registry.json"

# 2b) MCP stub server. The connections sidebar only lists MCP servers whose
#     handshake succeeds at boot, so the seeded mcp_servers rows must spawn a
#     real (if deterministic) MCP server. Copy the stub next to the data dir and
#     rewrite the placeholder token in mcp.db args to its absolute path (the seed
#     HOME is dynamic, so the path cannot live in the SQL fragment).
if [ -f "$HERE/files/mcp-stub-server.py" ]; then
  STUB_DST="$DATA/mcp-stub-server.py"
  cp "$HERE/files/mcp-stub-server.py" "$STUB_DST"
  chmod +x "$STUB_DST"
  if [ -f "$DATA/mcp.db" ]; then
    sqlite3 "$DATA/mcp.db" \
      "UPDATE mcp_servers SET args_json = replace(args_json, '__APOLLIA_SEED_MCP_STUB__', '$STUB_DST') WHERE args_json LIKE '%__APOLLIA_SEED_MCP_STUB__%';"
  fi
fi

# 3) Rewrite agent install_path + package root_path to the real seed location
#    (fragment used a placeholder). Runs only if agents.db exists.
#    install_path must point at the agent's .py entrypoint (agent.py): the boot
#    loader validates it as a .py file (loader.rs), not the containing directory.
if [ -f "$DATA/agents.db" ]; then
  sqlite3 "$DATA/agents.db" \
    "UPDATE installed_agents SET install_path = '$DATA/agents/' || name || '/agent.py' WHERE 1;" 2>/dev/null || true
  sqlite3 "$DATA/agents.db" \
    "UPDATE installed_agents SET source_path = '$DATA/agents/' || name || '/agent.py' WHERE 1;" 2>/dev/null || true
  sqlite3 "$DATA/agents.db" \
    "UPDATE installed_packages SET root_path = '$DATA/agents/packages/' || name WHERE 1;" 2>/dev/null || true
fi

# 4) Config: place apollia.toml in both the standard and XDG locations.
if [ -f "$HERE/files/apollia.toml" ]; then
  expand_seed_paths < "$HERE/files/apollia.toml" > "$DATA/apollia.toml"
  cp "$DATA/apollia.toml" "$CFG/apollia.toml"
fi

echo "==> done. Launch with: HOME=$SEED_HOME (toolchain env preserved)"
