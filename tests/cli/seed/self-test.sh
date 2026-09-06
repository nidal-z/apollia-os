#!/usr/bin/env bash
#
# Self-test for the seed builder. Runs in a few seconds, needs sqlite3 and python3, and
# guards the six ways this builder has silently produced a wrong fixture:
#
#   1. an overlay that is asked for and quietly not applied,
#   2. an overlay that leaks into the runs which must never see one (CI, the
#      just recipes, the -det suite whose counts are assertions),
#   3. a memory fixture whose file name and namespace column disagree, which
#      renders an empty namespace with no error anywhere,
#   4. a seed whose rows name absolute paths that do not exist, which empties a
#      screen instead of raising anything,
#   5. a registry cache still carrying the stub placeholder, which the catalogue
#      would hand to the launcher as a literal script argument,
#   6. the planner opt-in leaking into the base fixture, or asked for and not
#      applied: the base counts four agents and four triggers, and the
#      plan-gate-llm book needs the fifth agent to be orchestrated.
#
# Each case is written GIVEN / WHEN / THEN. Run it directly:
#
#   bash tests/cli/seed/self-test.sh
#
# `-e` is deliberately absent: it would stop the run at the first failing case
# and destroy the count this script exists to produce. The exit code is posted
# explicitly at the bottom instead.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILDER="$HERE/build-seed.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

PASS=0
FAIL=0

ok() { PASS=$((PASS + 1)); echo "  ok   $1"; }
ko() { FAIL=$((FAIL + 1)); echo "  FAIL $1"; }

# Print the end of a build log. The logs live under a directory the EXIT trap
# removes, so naming their path told a CI reader nothing they could act on: the
# run that produced three databases out of seventeen said only "see
# /tmp/tmp.XXXX/base.log", and the parse errors that explained it were never
# printed anywhere.
log_tail() {
  echo "       ---- last 20 lines of $(basename "$1") ----"
  tail -n 20 "$1" | sed -e 's/^/       /'
}

expect_eq() {
  local label=$1 want=$2 got=$3
  if [ "$want" = "$got" ]; then ok "$label"; else ko "$label (want '$want', got '$got')"; fi
}

echo "seed builder self-test"

# ---------------------------------------------------------------------------
# GIVEN no overlay is configured
# WHEN  the seed is built
# THEN  every checked-in schema produced its database, and the row counts the
#       -det suite asserts against are the checked-in ones
# ---------------------------------------------------------------------------
BASE="$WORK/base"
if env -u APOLLIA_SEED_OVERLAY -u APOLLIA_SEED_PLANNER bash "$BUILDER" "$BASE" >"$WORK/base.log" 2>&1; then
  ok "base build succeeds"
else
  ko "base build succeeds"
  log_tail "$WORK/base.log"
fi

schema_count=$(find "$HERE/schemas" -name '*.sql' | wc -l | tr -d ' ')
db_count=$(find "$BASE/.apollia" -maxdepth 1 -name '*.db' | wc -l | tr -d ' ')
expect_eq "one database per checked-in schema" "$schema_count" "$db_count"

base_sessions=$(sqlite3 "$BASE/.apollia/chat.db" "SELECT COUNT(*) FROM chat_sessions;")
expect_eq "base chat_sessions count" "4" "$base_sessions"

base_projects=$(sqlite3 "$BASE/.apollia/projects.db" "SELECT COUNT(*) FROM projects;")
expect_eq "base projects count" "2" "$base_projects"

# ---------------------------------------------------------------------------
# GIVEN a memory fixture whose namespace contains a colon, stored percent
#       encoded so a Windows checkout can hold the file
# WHEN  the seed is built
# THEN  the file name on disk is the namespace its rows carry, because
#       list_memory_entries opens `<file stem>.db` and then filters on that same
#       string (crates/apollia-desktop/src/commands/memory.rs)
# ---------------------------------------------------------------------------
mismatched=0
for db in "$BASE"/.apollia/memory/*.db; do
  [ -e "$db" ] || continue
  stem="$(basename "$db" .db)"
  case "$stem" in
    *%3A*) mismatched=$((mismatched + 1)); continue ;;
  esac
  n=$(sqlite3 "$db" "SELECT COUNT(*) FROM semantic_memories WHERE namespace = '$stem';" 2>/dev/null || echo 0)
  if [ "$n" = "0" ]; then
    mismatched=$((mismatched + 1))
    echo "       namespace '$stem' has no row of its own"
  fi
done
expect_eq "every memory file stem resolves to its own rows" "0" "$mismatched"

project_ns=$(find "$BASE/.apollia/memory" -name '*:*.db' | wc -l | tr -d ' ')
expect_eq "the project namespace file carries its colon" "1" "$project_ns"

# ---------------------------------------------------------------------------
# GIVEN an overlay carrying one extra schema and one extra fragment
# WHEN  the seed is built with APOLLIA_SEED_OVERLAY pointing at it
# THEN  the overlay database exists, the overlay rows are added on top of the
#       base rows, and the placeholders are expanded in the overlay too
# ---------------------------------------------------------------------------
OV="$WORK/overlay"
mkdir -p "$OV/schemas" "$OV/fragments" "$OV/files/memory"
cat >"$OV/schemas/selftest_extra.sql" <<'SQL'
CREATE TABLE selftest_rows (id TEXT PRIMARY KEY, where_from TEXT NOT NULL);
SQL
cat >"$OV/fragments/selftest_extra.sql" <<'SQL'
INSERT INTO selftest_rows (id, where_from) VALUES ('a', '__APOLLIA_SEED_HOME__');
SQL
cat >"$OV/fragments/chat.sql" <<'SQL'
INSERT INTO chat_sessions
    (id, mode, agent_name, system_prompt, status, available_tools, created_at,
     closed_at, llm_backend, summary, title, parent_session_id, fork_depth,
     project_id, plan_mode, plan_phase)
VALUES
    ('selftest-session', 'libre', NULL, '', 'active', '[]',
     '2026-07-01T00:00:00Z', NULL, 'local', 'overlay', 'Overlay session',
     NULL, 0, NULL, 0, 'done');
SQL

WITH="$WORK/with-overlay"
if env -u APOLLIA_SEED_PLANNER APOLLIA_SEED_OVERLAY="$OV" bash "$BUILDER" "$WITH" >"$WORK/overlay.log" 2>&1; then
  ok "overlay build succeeds"
else
  ko "overlay build succeeds"
  log_tail "$WORK/overlay.log"
fi

if [ -f "$WITH/.apollia/selftest_extra.db" ]; then
  ok "an overlay-only database is created"
else
  ko "an overlay-only database is created"
fi

ov_sessions=$(sqlite3 "$WITH/.apollia/chat.db" "SELECT COUNT(*) FROM chat_sessions;")
expect_eq "overlay rows add to the base rows" "5" "$ov_sessions"

ov_expanded=$(sqlite3 "$WITH/.apollia/selftest_extra.db" "SELECT where_from FROM selftest_rows;")
expect_eq "placeholders are expanded in overlay fragments" "$WITH" "$ov_expanded"

# ---------------------------------------------------------------------------
# GIVEN the same overlay directory still on disk
# WHEN  the seed is built WITHOUT APOLLIA_SEED_OVERLAY
# THEN  none of it is applied, so an operator with an overlay in their home can
#       still run the assertion suite
# ---------------------------------------------------------------------------
CLEAN="$WORK/clean"
env -u APOLLIA_SEED_OVERLAY -u APOLLIA_SEED_PLANNER bash "$BUILDER" "$CLEAN" >/dev/null 2>&1
clean_sessions=$(sqlite3 "$CLEAN/.apollia/chat.db" "SELECT COUNT(*) FROM chat_sessions;")
expect_eq "no overlay unless asked for" "4" "$clean_sessions"

if [ -f "$CLEAN/.apollia/selftest_extra.db" ]; then
  ko "no overlay-only database unless asked for"
else
  ok "no overlay-only database unless asked for"
fi

# ---------------------------------------------------------------------------
# GIVEN APOLLIA_SEED_OVERLAY pointing at a directory that does not exist
# WHEN  the seed is built
# THEN  the build stops, rather than produce a fixture that looks right
# ---------------------------------------------------------------------------
if APOLLIA_SEED_OVERLAY="$WORK/absent" bash "$BUILDER" "$WORK/never" >/dev/null 2>&1; then
  ko "a missing overlay is a hard error"
else
  ok "a missing overlay is a hard error"
fi

# ---------------------------------------------------------------------------
# GIVEN the base seed built above, with APOLLIA_SEED_PLANNER unset
# WHEN  its agent and trigger tables are counted
# THEN  they hold the four agents and four triggers the CLI suite and the -det
#       books assert, and nothing of the planner is there, in the database or
#       on disk
# ---------------------------------------------------------------------------
base_agents=$(sqlite3 "$BASE/.apollia/agents.db" "SELECT COUNT(*) FROM installed_agents;")
expect_eq "base installed_agents count" "4" "$base_agents"
base_triggers=$(sqlite3 "$BASE/.apollia/triggers_def.db" "SELECT COUNT(*) FROM trigger_definitions;")
expect_eq "base trigger_definitions count" "4" "$base_triggers"
base_planner=$(sqlite3 "$BASE/.apollia/agents.db" \
  "SELECT COUNT(*) FROM installed_agents WHERE name = 'seed-planner';")
expect_eq "no planner agent row unless asked for" "0" "$base_planner"
base_planner_trigger=$(sqlite3 "$BASE/.apollia/triggers_def.db" \
  "SELECT COUNT(*) FROM trigger_definitions WHERE id = 'seed-trigger-planner';")
expect_eq "no planner trigger row unless asked for" "0" "$base_planner_trigger"
if [ -e "$BASE/.apollia/agents/seed-planner" ]; then
  ko "no planner package on disk unless asked for"
else
  ok "no planner package on disk unless asked for"
fi

# ---------------------------------------------------------------------------
# GIVEN APOLLIA_SEED_PLANNER=1
# WHEN  the seed is built
# THEN  a fifth agent row exists, orchestrated and with a system prompt (the
#       backend reads execution_mode and system_prompt from this row, and the
#       orchestrated engine refuses a manifest without one), its trigger row
#       targets it, its package is on disk at the path the row names, and the
#       rest of the fixture is the base one
# ---------------------------------------------------------------------------
PLANNED="$WORK/planner"
if env -u APOLLIA_SEED_OVERLAY APOLLIA_SEED_PLANNER=1 bash "$BUILDER" "$PLANNED" >"$WORK/planner.log" 2>&1; then
  ok "planner build succeeds"
else
  ko "planner build succeeds"
  log_tail "$WORK/planner.log"
fi
planned_agents=$(sqlite3 "$PLANNED/.apollia/agents.db" "SELECT COUNT(*) FROM installed_agents;")
expect_eq "the planner adds one agent row" "5" "$planned_agents"
planned_orchestrated=$(sqlite3 "$PLANNED/.apollia/agents.db" \
  "SELECT COUNT(*) FROM installed_agents WHERE name = 'seed-planner' AND enabled = 1 \
   AND manifest_json LIKE '%\"execution_mode\":\"orchestrated\"%' \
   AND manifest_json LIKE '%\"system_prompt\":\"%';")
expect_eq "the planner row is enabled, orchestrated, with a system prompt" "1" "$planned_orchestrated"
planned_triggers=$(sqlite3 "$PLANNED/.apollia/triggers_def.db" "SELECT COUNT(*) FROM trigger_definitions;")
expect_eq "the planner adds one trigger row" "5" "$planned_triggers"
planned_trigger_agent=$(sqlite3 "$PLANNED/.apollia/triggers_def.db" \
  "SELECT agent || ':' || enabled FROM trigger_definitions WHERE id = 'seed-trigger-planner';")
expect_eq "the planner trigger targets the enabled planner" "seed-planner:1" "$planned_trigger_agent"
planned_path=$(sqlite3 "$PLANNED/.apollia/agents.db" \
  "SELECT install_path FROM installed_agents WHERE name = 'seed-planner';")
if [ -f "$planned_path" ] && [ "$planned_path" = "$PLANNED/.apollia/agents/seed-planner/agent.py" ]; then
  ok "the planner row names its agent.py under the seed home"
else
  ko "the planner row names its agent.py under the seed home (got '$planned_path')"
fi
planned_sessions=$(sqlite3 "$PLANNED/.apollia/chat.db" "SELECT COUNT(*) FROM chat_sessions;")
expect_eq "the planner opt-in leaves the rest of the fixture alone" "4" "$planned_sessions"

# ---------------------------------------------------------------------------
# GIVEN APOLLIA_SEED_PLANNER set to something that is not 1
# WHEN  the seed is built
# THEN  the build stops: a misspelt switch must not quietly build the base
#       fixture and send the plan-gate book to a trigger that is not there
# ---------------------------------------------------------------------------
if env -u APOLLIA_SEED_OVERLAY APOLLIA_SEED_PLANNER=yes bash "$BUILDER" "$WORK/never-planner" >/dev/null 2>&1; then
  ko "an unknown planner switch value is a hard error"
else
  ok "an unknown planner switch value is a hard error"
fi

# ---------------------------------------------------------------------------
# GIVEN the base seed built above, whose registry cache names the MCP stub
#       through the same placeholder token as the mcp.db rows
# WHEN  the cache file is parsed
# THEN  it is a JSON array, no token is left in it, and the stub path it names
#       exists on disk, since a catalogue install runs exactly that path
# ---------------------------------------------------------------------------
registry_verdict=$(python3 - "$BASE/.apollia/mcp-registry.json" <<'PY'
import json, os, sys
path = sys.argv[1]
try:
    raw = open(path, encoding="utf-8").read()
    servers = json.loads(raw)
except (OSError, ValueError) as e:
    print(f"unreadable: {e}"); sys.exit(0)
if not isinstance(servers, list) or not servers:
    print("not a non-empty array"); sys.exit(0)
if "__APOLLIA_SEED_MCP_STUB__" in raw:
    print("placeholder token left in the cache"); sys.exit(0)
identifiers = [
    pkg["identifier"]
    for s in servers
    for pkg in (s.get("server", {}).get("packages") or [])
]
stubs = [i for i in identifiers if i.endswith("mcp-stub-server.py")]
if len(stubs) != 1:
    print(f"expected one stub identifier, found {len(stubs)}"); sys.exit(0)
if not os.path.isfile(stubs[0]):
    print(f"stub path does not exist: {stubs[0]}"); sys.exit(0)
print("ok")
PY
)
expect_eq "the registry cache names the stub at its final path" "ok" "$registry_verdict"

# ---------------------------------------------------------------------------
# GIVEN a project root passed explicitly
# WHEN  the seed is built
# THEN  the seeded project rows point at it, so a screenshot session can keep
#       the operator's own checkout path off a published image
# ---------------------------------------------------------------------------
ALT="$WORK/alt-root"
mkdir -p "$ALT"
env -u APOLLIA_SEED_OVERLAY -u APOLLIA_SEED_PLANNER APOLLIA_SEED_PROJECT_ROOT="$ALT" \
  bash "$BUILDER" "$WORK/alt" >/dev/null 2>&1
alt_root=$(sqlite3 "$WORK/alt/.apollia/projects.db" "SELECT path FROM project_providers WHERE path IS NOT NULL LIMIT 1;")
expect_eq "the project root is overridable" "$ALT" "$alt_root"

# ---------------------------------------------------------------------------
# GIVEN a home alias, which is how load.sh builds into a staging directory and
#       still names the home the seed will end up under
# WHEN  the seed is built
# THEN  the seeded filesystem trigger watches the alias, not the staging path
# ---------------------------------------------------------------------------
env -u APOLLIA_SEED_OVERLAY -u APOLLIA_SEED_PLANNER APOLLIA_SEED_HOME_ALIAS="/Users/seed-operator" \
  bash "$BUILDER" "$WORK/aliased" >/dev/null 2>&1
watched=$(sqlite3 "$WORK/aliased/.apollia/triggers_def.db" \
  "SELECT COUNT(*) FROM trigger_definitions WHERE source_config LIKE '%/Users/seed-operator/%';")
expect_eq "the home alias reaches the seeded rows" "1" "$watched"

# ---------------------------------------------------------------------------
# GIVEN the base seed built above, in the directory it was built for
# WHEN  every absolute path its databases name is resolved on disk
# THEN  all of them exist
#
# Added after the agents vanished from a loaded seed: the files were in place,
# agents.db named a staging directory that load.sh had deleted, and nothing
# logged an error.
# ---------------------------------------------------------------------------
PATHS="$HERE/self-test-paths.sh"
if bash "$PATHS" "$BASE/.apollia" >"$WORK/paths-base.log" 2>&1; then
  ok "a freshly built seed names only paths that exist"
else
  ko "a freshly built seed names only paths that exist"
  log_tail "$WORK/paths-base.log"
fi
if bash "$PATHS" "$PLANNED/.apollia" >"$WORK/paths-planner.log" 2>&1; then
  ok "a planner build names only paths that exist"
else
  ko "a planner build names only paths that exist"
  log_tail "$WORK/paths-planner.log"
fi

# ---------------------------------------------------------------------------
# GIVEN a seed built into a staging directory WITHOUT the home alias, then
#       moved, which is load.sh's gesture stripped of its one precaution
# WHEN  the same path check runs on the moved result
# THEN  it fails, because the rows still name the staging directory
#
# This is the case that keeps the check above from being decoration. A detector
# only ever run on a healthy subject proves the sweep happened, not that the
# detector works.
# ---------------------------------------------------------------------------
STAGE="$WORK/stage"
MOVED="$WORK/moved"
env -u APOLLIA_SEED_OVERLAY -u APOLLIA_SEED_PLANNER bash "$BUILDER" "$STAGE" >/dev/null 2>&1
mkdir -p "$MOVED"
mv "$STAGE/.apollia" "$MOVED/.apollia"
if bash "$PATHS" "$MOVED/.apollia" >/dev/null 2>&1; then
  ko "a seed built without the home alias and moved is caught"
else
  ok "a seed built without the home alias and moved is caught"
fi

echo
echo "seed self-test: $PASS passed, $FAIL failed"

# Explicit, and the last thing this script does. The count used to be computed
# and then thrown away: `set -e` is absent, so the bare test that closed the
# script was not its exit status, and whatever ran after it decided the verdict.
# The run that reported `15 passed, 0 failed` and exited 1 is what that costs.
if [ "$FAIL" -eq 0 ]; then
  exit 0
fi
exit 1
