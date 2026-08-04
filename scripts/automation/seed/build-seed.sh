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
#
# Environment:
#   APOLLIA_SEED_OVERLAY       directory holding extra schemas/, fragments/ and
#                              files/ applied ON TOP of the checked-in seed. Not
#                              set means no overlay, which is the state every
#                              automated caller (CI, the just recipes) runs in,
#                              so the fixture stays byte-identical for them. Set
#                              but missing is a hard error: an overlay that was
#                              asked for and silently skipped would produce a
#                              plausible-looking wrong fixture.
#   APOLLIA_SEED_PROJECT_ROOT  what __APOLLIA_SEED_WORKSPACE__ expands to.
#                              Defaults to this repository checkout, which is
#                              what the context providers need to have real git
#                              content to show. Point it elsewhere when the path
#                              itself ends up on a published screenshot.
#   APOLLIA_SEED_HOME_ALIAS    what __APOLLIA_SEED_HOME__ expands to. Defaults to
#                              SEED_HOME. load.sh overrides it because it builds
#                              into a staging directory and then moves the result
#                              to the real HOME, so the staging path must never
#                              be baked into a row.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd "$HERE/../../.." >/dev/null 2>&1 && pwd)"
SEED_HOME="${1:-$PWD/.apollia-seed-home}"
DATA="$SEED_HOME/.apollia"
CFG="$SEED_HOME/.config/apollia"

PROJECT_ROOT="${APOLLIA_SEED_PROJECT_ROOT:-$REPO_ROOT}"
HOME_ALIAS="${APOLLIA_SEED_HOME_ALIAS:-$SEED_HOME}"

OVERLAY="${APOLLIA_SEED_OVERLAY:-}"
if [ -n "$OVERLAY" ] && [ ! -d "$OVERLAY" ]; then
  echo "error: APOLLIA_SEED_OVERLAY points at $OVERLAY, which is not a directory" >&2
  echo "       unset it to build the checked-in seed alone, or create the overlay" >&2
  exit 1
fi

# Expand the seed placeholders on stdin. Keeps absolute paths out of the
# checked-in fragments while the seeded rows still point at real locations.
expand_seed_paths() {
  sed -e "s|__APOLLIA_SEED_WORKSPACE__|$PROJECT_ROOT|g" \
      -e "s|__APOLLIA_SEED_HOME__|$HOME_ALIAS|g"
}

# Apply a schema dump to a DB. `sqlite_sequence` is a reserved internal table
# SQLite auto-manages; the dumps include its CREATE line, which errors on replay.
apply_schema() {
  grep -v 'CREATE TABLE sqlite_sequence' "$1" | sqlite3 "$2"
}

echo "==> seed HOME: $SEED_HOME"
if [ -n "$OVERLAY" ]; then
  echo "==> overlay:   $OVERLAY"
else
  echo "==> overlay:   none (checked-in seed only)"
fi
rm -rf "$SEED_HOME"
mkdir -p "$DATA" "$CFG" "$DATA/agents" "$DATA/memory" "$DATA/models" "$DATA/venvs"

# 1) Databases: schema then fragment (fragment is INSERTs only).
#    A DB with a schema but no fragment is created empty (still valid).
#
#    Fragments carry machine-independent placeholders so the checked-in seed
#    holds no absolute path from whoever recorded it. They are expanded here:
#      __APOLLIA_SEED_WORKSPACE__  the project checkout the seeded project and
#                                  governance rows point at
#      __APOLLIA_SEED_HOME__       the HOME the seed will finally live under
for schema in "$HERE"/schemas/*.sql; do
  db="$(basename "$schema" .sql)"
  frag="$HERE/fragments/$db.sql"
  echo "==> db: $db.db"
  apply_schema "$schema" "$DATA/$db.db"
  if [ -f "$frag" ]; then
    expand_seed_paths < "$frag" | sqlite3 "$DATA/$db.db"
  fi
done

# 1b) Overlay databases. Schemas first (a DB the checked-in seed does not know
#     about, such as runtime_events.db, exists only here), then fragments, which
#     are replayed AFTER the base ones so overlay rows extend rather than race
#     them. An overlay fragment for a DB with no schema anywhere is an error the
#     sqlite3 call surfaces on its own.
if [ -n "$OVERLAY" ]; then
  if [ -d "$OVERLAY/schemas" ]; then
    for schema in "$OVERLAY"/schemas/*.sql; do
      [ -e "$schema" ] || continue
      db="$(basename "$schema" .sql)"
      echo "==> overlay db schema: $db.db"
      apply_schema "$schema" "$DATA/$db.db"
    done
  fi
  if [ -d "$OVERLAY/fragments" ]; then
    for frag in "$OVERLAY"/fragments/*.sql; do
      [ -e "$frag" ] || continue
      db="$(basename "$frag" .sql)"
      echo "==> overlay db rows:   $db.db"
      expand_seed_paths < "$frag" | sqlite3 "$DATA/$db.db"
    done
  fi
fi

# 2) On-disk files (agents, memory, models, mcp registry). Copied verbatim.
[ -d "$HERE/files/agents" ] && cp -R "$HERE/files/agents/." "$DATA/agents/"
[ -d "$HERE/files/memory" ] && cp -R "$HERE/files/memory/." "$DATA/memory/"
[ -d "$HERE/files/models" ] && cp -R "$HERE/files/models/." "$DATA/models/"
[ -f "$HERE/files/mcp-registry.json" ] && cp "$HERE/files/mcp-registry.json" "$DATA/mcp-registry.json"

# 2a) Memory fixture file names ARE namespaces: list_memory_namespaces() returns
#     the file stem and list_memory_entries(ns) opens `<ns>.db` then filters its
#     rows on the same string. A project namespace contains a colon, which is
#     illegal in a Windows path, so the checked-in fixture stores it percent
#     encoded and the real name is restored here, into a throwaway HOME that no
#     Windows checkout ever sees. Without this the file stem and the namespace
#     column disagree and the namespace renders empty, with no error anywhere.
for enc in "$DATA"/memory/*%3A*.db; do
  [ -e "$enc" ] || continue
  dec="$(dirname "$enc")/$(basename "$enc" | sed 's|%3A|:|g')"
  mv "$enc" "$dec"
done

# 2c) Overlay files, copied over the base ones (same name wins for the overlay).
if [ -n "$OVERLAY" ] && [ -d "$OVERLAY/files" ]; then
  [ -d "$OVERLAY/files/agents" ] && cp -R "$OVERLAY/files/agents/." "$DATA/agents/"
  [ -d "$OVERLAY/files/memory" ] && cp -R "$OVERLAY/files/memory/." "$DATA/memory/"
  [ -d "$OVERLAY/files/models" ] && cp -R "$OVERLAY/files/models/." "$DATA/models/"
  for enc in "$DATA"/memory/*%3A*.db; do
    [ -e "$enc" ] || continue
    dec="$(dirname "$enc")/$(basename "$enc" | sed 's|%3A|:|g')"
    mv "$enc" "$dec"
  done
fi

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

# 4) Config: place apollia.toml in both the standard and XDG locations. An
#    overlay may replace it wholesale (it carries the same placeholders).
CONFIG_SRC="$HERE/files/apollia.toml"
if [ -n "$OVERLAY" ] && [ -f "$OVERLAY/files/apollia.toml" ]; then
  CONFIG_SRC="$OVERLAY/files/apollia.toml"
fi
if [ -f "$CONFIG_SRC" ]; then
  expand_seed_paths < "$CONFIG_SRC" > "$DATA/apollia.toml"
  cp "$DATA/apollia.toml" "$CFG/apollia.toml"
fi

echo "==> done. Launch with: HOME=$SEED_HOME (toolchain env preserved)"
