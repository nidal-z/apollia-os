#!/usr/bin/env bash
#
# Self-test for the seed builder. Runs in a few seconds, needs only sqlite3, and
# guards the three ways this builder has silently produced a wrong fixture:
#
#   1. an overlay that is asked for and quietly not applied,
#   2. an overlay that leaks into the runs which must never see one (CI, the
#      just recipes, the -det suite whose counts are assertions),
#   3. a memory fixture whose file name and namespace column disagree, which
#      renders an empty namespace with no error anywhere.
#
# Each case is written GIVEN / WHEN / THEN. Run it directly:
#
#   bash scripts/automation/seed/self-test.sh
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILDER="$HERE/build-seed.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

PASS=0
FAIL=0

ok() { PASS=$((PASS + 1)); echo "  ok   $1"; }
ko() { FAIL=$((FAIL + 1)); echo "  FAIL $1"; }

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
if env -u APOLLIA_SEED_OVERLAY bash "$BUILDER" "$BASE" >"$WORK/base.log" 2>&1; then
  ok "base build succeeds"
else
  ko "base build succeeds (see $WORK/base.log)"
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
if APOLLIA_SEED_OVERLAY="$OV" bash "$BUILDER" "$WITH" >"$WORK/overlay.log" 2>&1; then
  ok "overlay build succeeds"
else
  ko "overlay build succeeds (see $WORK/overlay.log)"
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
env -u APOLLIA_SEED_OVERLAY bash "$BUILDER" "$CLEAN" >/dev/null 2>&1
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
# GIVEN a project root passed explicitly
# WHEN  the seed is built
# THEN  the seeded project rows point at it, so a screenshot session can keep
#       the operator's own checkout path off a published image
# ---------------------------------------------------------------------------
ALT="$WORK/alt-root"
mkdir -p "$ALT"
env -u APOLLIA_SEED_OVERLAY APOLLIA_SEED_PROJECT_ROOT="$ALT" \
  bash "$BUILDER" "$WORK/alt" >/dev/null 2>&1
alt_root=$(sqlite3 "$WORK/alt/.apollia/projects.db" "SELECT path FROM project_providers WHERE path IS NOT NULL LIMIT 1;")
expect_eq "the project root is overridable" "$ALT" "$alt_root"

# ---------------------------------------------------------------------------
# GIVEN a home alias, which is how load.sh builds into a staging directory and
#       still names the home the seed will end up under
# WHEN  the seed is built
# THEN  the seeded filesystem trigger watches the alias, not the staging path
# ---------------------------------------------------------------------------
env -u APOLLIA_SEED_OVERLAY APOLLIA_SEED_HOME_ALIAS="/Users/seed-operator" \
  bash "$BUILDER" "$WORK/aliased" >/dev/null 2>&1
watched=$(sqlite3 "$WORK/aliased/.apollia/triggers_def.db" \
  "SELECT COUNT(*) FROM trigger_definitions WHERE source_config LIKE '%/Users/seed-operator/%';")
expect_eq "the home alias reaches the seeded rows" "1" "$watched"

echo
echo "seed self-test: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
