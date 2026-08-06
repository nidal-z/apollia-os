#!/usr/bin/env bash
# Regenerates the per-namespace memory *.db fixtures consumed by memory-det.json.
#
# The desktop memory page reads ~/.apollia/memory/<namespace>.db, one FILE per
# namespace (list_memory_namespaces scans *.db, the file stem IS the namespace).
# Because every read query filters by the namespace column, applying the whole
# fragment to each per-namespace db is equivalent to a per-namespace split: only
# that file's rows are ever returned for that namespace.
#
# Output: one <namespace>.db per seed namespace, in this directory. A harness
# step copies them to ~/.apollia/memory/ before the run.
#
# Deterministic: fixed schema + fixed fragment, no timestamps or random ids.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
seed_root="$(cd "${here}/../.." && pwd)"
schema="${seed_root}/schemas/user_memory.sql"
fragment="${seed_root}/fragments/user_memory.sql"

# Namespaces == db file names. A project namespace carries a colon, which the UI
# reads as the project marker (classifyNamespace) but which is illegal in a
# Windows path, so the checked-in file name percent encodes it and build-seed.sh
# decodes it back into the throwaway HOME. Encode here, so the file that lands in
# this directory is the one a Windows checkout can hold.
namespaces=(
  "__user__"
  "apollia-guide"
  "onboarding-agent"
  "seed-project-alpha:default"
  "legacy-notes"
)

# Percent-encode the characters no Windows checkout accepts in a path. Only the
# colon is used today; the function is the single place to extend.
encode_ns() {
  printf '%s' "${1//:/%3A}"
}

# The provided schema is a `.schema` dump that includes an explicit
# `CREATE TABLE sqlite_sequence(...)`. That table is reserved and auto-created by
# SQLite for the AUTOINCREMENT column in plan_choices, so executing the line
# errors. Strip it: it is redundant, not load-bearing.
schema_sql="$(grep -v 'sqlite_sequence' "${schema}")"

for ns in "${namespaces[@]}"; do
  db="${here}/$(encode_ns "${ns}").db"
  rm -f "${db}"
  printf '%s\n' "${schema_sql}" | sqlite3 "${db}"
  sqlite3 "${db}" < "${fragment}"
  echo "built ${db}"
done
