#!/usr/bin/env bash
# Build an isolated, deterministic Apollia data ecosystem for the automation
# verification suite. Produces a throwaway HOME whose `.apollia` is fully seeded
# (SQLite DBs + agents + memory + models + config), so the det scripts find data
# without touching the real ~/.apollia profile.
#
# Usage:
#   tests/cli/seed/build-seed.sh [SEED_HOME]
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
#   APOLLIA_SEED_PLANNER       "1" adds the opt-in orchestrated planner: the
#                              seed-planner agent row, its package under
#                              agents/seed-planner, and the seed-trigger-planner
#                              row (planner/fragments, planner/files). Not set
#                              means the base fixture, whose row counts the CLI
#                              suite and the -det books assert. Any other value
#                              is a hard error, for the same reason as a missing
#                              overlay. The plan-gate-llm book is the consumer.
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

# The schemas of chat.db and user_memory.db declare FTS5 virtual tables, and a
# sqlite3 built without that module answers "no such module: fts5" halfway
# through, leaving a profile whose databases are half there. Measured on the
# GitHub macOS runner on 2026-09-07: twenty assertions failed one after the
# other, none of them naming the cause. The product is never affected, it
# carries its own SQLite through rusqlite's bundled build; this is the CLI on
# PATH, and the check names it rather than letting the failure cascade.
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"

# Which SQLite this build drives, decided once. The command is preferred when
# it is there and carries FTS5; Python's own SQLite stands in otherwise, since
# Windows ships no `sqlite3` on PATH and neither does a minimal Linux image.
# Measured on 2026-09-08: a Windows machine with everything else in place could
# not build the profile at all.
SEED_SQLITE_MODE=""
SEED_PYTHON=""
FTS5_PROBE="CREATE VIRTUAL TABLE t USING fts5(x);"
if command -v sqlite3 >/dev/null 2>&1 &&
  sqlite3 :memory: "$FTS5_PROBE" >/dev/null 2>&1; then
  SEED_SQLITE_MODE="cli"
else
  for _py in "${APOLLIA_E2E_PYTHON:-}" python3 python; do
    [ -n "$_py" ] || continue
    command -v "$_py" >/dev/null 2>&1 || continue
    if "$_py" "$HERE/sqlite_exec.py" :memory: "$FTS5_PROBE" >/dev/null 2>&1; then
      SEED_PYTHON="$_py"
      SEED_SQLITE_MODE="python"
      break
    fi
  done
fi
if [ -z "$SEED_SQLITE_MODE" ]; then
  echo "seed: no SQLite carrying FTS5 is reachable, so the profile cannot be built" >&2
  echo "      Two ways out: a sqlite3 on PATH built with FTS5 (macOS: brew install" >&2
  echo "      sqlite, then its bin first on PATH; Debian: the packaged one has it;" >&2
  echo "      Windows: winget install SQLite.SQLite), or a Python 3 whose sqlite3" >&2
  echo "      module carries FTS5, which the bundled interpreter of this tree does." >&2
  exit 2
fi

# A path a native process can open. git-bash hands out `/c/Users/...`, which is
# its own POSIX view: the product is a native Windows binary and opens nothing
# under that name. `cygpath -m` renders `C:/Users/...`, accepted by Windows and
# safe to embed in JSON, which the backslash form is not. A no-op everywhere
# else, where cygpath does not exist.
seed_native_path() {
  if command -v cygpath >/dev/null 2>&1; then
    cygpath -m "$1"
  else
    printf '%s' "$1"
  fi
}

# The interpreter the seeded MCP servers are launched with, resolved rather
# than written into the fragment. `/usr/bin/python3` is there on macOS and on
# most Linux images and nowhere on Windows, where the two seeded rows would
# spawn nothing: the servers stay disconnected, and every connection assertion
# of the desktop books fails on a machine whose product is fine. The candidate
# is selected by RUNNING it, because `python3` on Windows is a Microsoft Store
# alias that sits on PATH and refuses to start. Measured on 2026-09-08.
SEED_MCP_PYTHON=""
for _py in "${APOLLIA_E2E_PYTHON:-}" "$SEED_PYTHON" /usr/bin/python3 python3 python; do
  [ -n "$_py" ] || continue
  command -v "$_py" >/dev/null 2>&1 || continue
  if "$_py" -c 'import sys; sys.exit(0 if sys.version_info[0] == 3 else 1)' >/dev/null 2>&1; then
    SEED_MCP_PYTHON="$(seed_native_path "$(command -v "$_py")")"
    break
  fi
done
if [ -z "$SEED_MCP_PYTHON" ]; then
  echo "seed: no runnable Python 3 found, so the seeded MCP servers will not start" >&2
  echo "      (the rest of the profile is built; the connections page will read empty)" >&2
fi

# One entry point for every statement this builder runs. SQL comes as an
# argument when there is one, on standard input otherwise, which is the shape
# the fragments use.
seed_sqlite() {
  local database="$1"
  shift
  if [ "$SEED_SQLITE_MODE" = "cli" ]; then
    if [ "$#" -gt 0 ]; then sqlite3 "$database" "$@"; else sqlite3 "$database"; fi
  else
    if [ "$#" -gt 0 ]; then
      "$SEED_PYTHON" "$HERE/sqlite_exec.py" "$database" "$@"
    else
      "$SEED_PYTHON" "$HERE/sqlite_exec.py" "$database"
    fi
  fi
}
REPO_ROOT="$(cd "$HERE/../../.." >/dev/null 2>&1 && pwd)"
SEED_HOME="${1:-$PWD/.apollia-seed-home}"
DATA="$SEED_HOME/.apollia"
CFG="$SEED_HOME/.config/apollia"

PROJECT_ROOT="$(seed_native_path "${APOLLIA_SEED_PROJECT_ROOT:-$REPO_ROOT}")"
# Native form, because these two are the only paths this script writes INTO a
# database rather than passing to a shell tool: the product reads them back and
# opens them itself. git-bash hands out `/c/Users/...`, its own POSIX view, and
# a native Windows binary opens nothing under that name. Measured on
# 2026-09-08: the desktop books answered "agent not found: seed-classifier" on
# a profile whose agent files were all in place, because installed_agents named
# them the POSIX way.
HOME_ALIAS="$(seed_native_path "${APOLLIA_SEED_HOME_ALIAS:-$SEED_HOME}")"
# Every absolute path written INTO a database has to name the home the profile
# will end up under, not the directory this script happens to build in. They are
# the same for the automaton, which builds straight into its target, and they
# differ for load.sh, which builds into a staging directory and then moves the
# result. Writing $DATA there baked a path under /var/folders that load.sh
# deletes on the way out: the agent files landed correctly in the home while
# agents.db pointed at a directory that no longer existed, so the boot loader
# found nothing and the guide and onboarding agents simply never appeared.
DATA_ALIAS="$HOME_ALIAS/.apollia"

OVERLAY="${APOLLIA_SEED_OVERLAY:-}"
if [ -n "$OVERLAY" ] && [ ! -d "$OVERLAY" ]; then
  echo "error: APOLLIA_SEED_OVERLAY points at $OVERLAY, which is not a directory" >&2
  echo "       unset it to build the checked-in seed alone, or create the overlay" >&2
  exit 1
fi

PLANNER="${APOLLIA_SEED_PLANNER:-}"
case "$PLANNER" in
  ""|1) ;;
  *)
    echo "error: APOLLIA_SEED_PLANNER is '$PLANNER'; set it to 1 for the planner opt-in, or unset it" >&2
    exit 1
    ;;
esac

# Expand the seed placeholders on stdin. Keeps absolute paths out of the
# checked-in fragments while the seeded rows still point at real locations.
expand_seed_paths() {
  sed -e "s|__APOLLIA_SEED_WORKSPACE__|$PROJECT_ROOT|g" \
      -e "s|__APOLLIA_SEED_HOME__|$HOME_ALIAS|g"
}

# Apply a schema dump to a DB, dropping the lines SQLite reserves for itself.
#
# `sqlite_sequence` is a reserved internal table SQLite auto-manages; the dumps
# include its CREATE line, which errors on replay.
#
# The shadow tables of an FTS5 virtual table (`<name>_fts_data`, `_idx`,
# `_content`, `_docsize`, `_config`) are the same case: the CREATE VIRTUAL TABLE
# line a few lines above builds them, and the dump names them too.
#
# Whether replaying them is an error depends on the sqlite3 build, and not in a
# way you can predict from the version number. Replaying the unfiltered
# chat.sql, four versions measured: 3.40.1 accepts, 3.45.1 refuses, 3.46.1
# refuses, 3.51.0 accepts. So this is not a "from version X onwards" rule and no
# caller should treat it as one. Where it refuses, the message is `object name
# reserved for internal use`, and under `set -e` the build dies on the third
# database out of seventeen: that is what made every CI run of this seed produce
# three databases and a self-test reporting twelve failures.
apply_schema() {
  grep -vE "CREATE TABLE sqlite_sequence|CREATE TABLE IF NOT EXISTS .[A-Za-z_]+_fts_(data|idx|content|docsize|config)." "$1" | seed_sqlite "$2"
}

echo "==> seed HOME: $SEED_HOME"
if [ -n "$OVERLAY" ]; then
  echo "==> overlay:   $OVERLAY"
else
  echo "==> overlay:   none (checked-in seed only)"
fi
if [ "$PLANNER" = "1" ]; then
  echo "==> planner:   opt-in (seed-planner agent + seed-trigger-planner)"
else
  echo "==> planner:   none (base fixture)"
fi
rm -rf "$SEED_HOME"
# `Downloads` is not part of the profile: it is the directory the seeded
# filesystem trigger watches (fragments/triggers_def.sql). On the human path the
# alias names the operator's own home, where it already exists; on the automated
# path the home is a throwaway one, and a trigger watching a directory nobody
# creates is a fixture naming something that is not there.
mkdir -p "$DATA" "$CFG" "$DATA/agents" "$DATA/memory" "$DATA/models" "$DATA/venvs" \
         "$SEED_HOME/Downloads"

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
    expand_seed_paths < "$frag" | seed_sqlite "$DATA/$db.db"
  fi
done

# 1a) Planner opt-in rows. Replayed AFTER the base fragments and BEFORE the
#     overlay, into databases the base schemas already created, so an overlay
#     still lands last and step 3 rewrites the planner's install_path like every
#     other agent row. Nothing here runs on the base build: the CLI suite and
#     the -det books count four agents and four triggers.
if [ "$PLANNER" = "1" ]; then
  for frag in "$HERE"/planner/fragments/*.sql; do
    [ -e "$frag" ] || continue
    db="$(basename "$frag" .sql)"
    echo "==> planner db rows:   $db.db"
    expand_seed_paths < "$frag" | seed_sqlite "$DATA/$db.db"
  done
fi

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
      expand_seed_paths < "$frag" | seed_sqlite "$DATA/$db.db"
    done
  fi
fi

# 2) On-disk files (agents, memory, models). Copied verbatim. The MCP registry
#    cache is written in 2b below, because it names the stub path.
[ -d "$HERE/files/agents" ] && cp -R "$HERE/files/agents/." "$DATA/agents/"
[ -d "$HERE/files/memory" ] && cp -R "$HERE/files/memory/." "$DATA/memory/"
[ -d "$HERE/files/models" ] && cp -R "$HERE/files/models/." "$DATA/models/"
# Fixture packages the install dialog previews or installs from a picker the
# automaton answers; never installed by the builder, so the package counts the
# books and the CLI suite assert stay what they are.
if [ -d "$HERE/files/fixtures" ]; then
  mkdir -p "$DATA/fixtures"
  cp -R "$HERE/files/fixtures/." "$DATA/fixtures/"
fi

# 2p) Planner opt-in package, copied over the base agents like an overlay's
#     (same name wins) and before step 3, whose install_path rewrite names it.
if [ "$PLANNER" = "1" ] && [ -d "$HERE/planner/files/agents" ]; then
  cp -R "$HERE/planner/files/agents/." "$DATA/agents/"
fi

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
  # Copied to the staging path, referenced by the final one, for the reason
  # given at DATA_ALIAS above.
  STUB_REF="$DATA_ALIAS/mcp-stub-server.py"
  cp "$HERE/files/mcp-stub-server.py" "$STUB_DST"
  chmod +x "$STUB_DST"
  if [ -f "$DATA/mcp.db" ]; then
    seed_sqlite "$DATA/mcp.db" \
      "UPDATE mcp_servers SET args_json = replace(args_json, '__APOLLIA_SEED_MCP_STUB__', '$STUB_REF') WHERE args_json LIKE '%__APOLLIA_SEED_MCP_STUB__%';"
    if [ -n "$SEED_MCP_PYTHON" ]; then
      seed_sqlite "$DATA/mcp.db" \
        "UPDATE mcp_servers SET command = '$SEED_MCP_PYTHON' WHERE command = '__APOLLIA_SEED_PYTHON__';"
    fi
  fi
  # The MCP registry cache. McpRegistryClient reads `<data dir>/mcp-registry.json`
  # without a network call while the file is younger than fifteen minutes
  # (crates/apollia-desktop/src/mcp/registry_client.rs, CACHE_TTL), and falls
  # back to it when the registry is unreachable, so the catalogue shows these
  # entries on a hermetic run. The seed-stub entry installs the same stub as
  # the mcp.db rows, through the same token, rewritten to the same path.
  if [ -f "$HERE/files/mcp-registry.json" ]; then
    sed -e "s|__APOLLIA_SEED_MCP_STUB__|$STUB_REF|g" \
        -e "s|__APOLLIA_SEED_PYTHON__|${SEED_MCP_PYTHON:-/usr/bin/python3}|g" \
        "$HERE/files/mcp-registry.json" > "$DATA/mcp-registry.json"
  fi
fi

# 3) Rewrite agent install_path + package root_path to the real seed location
#    (fragment used a placeholder). Runs only if agents.db exists.
#    install_path must point at the agent's .py entrypoint (agent.py): the boot
#    loader validates it as a .py file (loader.rs), not the containing directory.
if [ -f "$DATA/agents.db" ]; then
  seed_sqlite "$DATA/agents.db" \
    "UPDATE installed_agents SET install_path = '$DATA_ALIAS/agents/' || name || '/agent.py' WHERE 1;" 2>/dev/null || true
  seed_sqlite "$DATA/agents.db" \
    "UPDATE installed_agents SET source_path = '$DATA_ALIAS/agents/' || name || '/agent.py' WHERE 1;" 2>/dev/null || true
  seed_sqlite "$DATA/agents.db" \
    "UPDATE installed_packages SET root_path = '$DATA_ALIAS/agents/packages/' || name WHERE 1;" 2>/dev/null || true
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
