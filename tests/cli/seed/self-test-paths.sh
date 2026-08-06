#!/usr/bin/env bash
# Every absolute path a seeded database names must exist on disk.
#
# This exists because it went wrong silently. `load.sh` builds into a staging
# directory and moves the result, so a path baked from the build directory
# points at something the script deletes on its way out. The agent files landed
# correctly, `agents.db` named a directory under /var/folders that no longer
# existed, the boot loader found nothing, and the guide and onboarding agents
# were simply absent from the application. Nothing failed, nothing logged an
# error, the screens were just empty.
#
# Usage: bash tests/cli/seed/self-test-paths.sh <SEED_DATA_DIR>
#   e.g. bash tests/cli/seed/self-test-paths.sh ~/.apollia
set -euo pipefail

DATA="${1:-$HOME/.apollia}"
if [ ! -d "$DATA" ]; then
  echo "no data directory at $DATA" >&2
  exit 1
fi

python3 - "$DATA" <<'PY'
import glob, os, re, sqlite3, sys

data = sys.argv[1]
# An absolute POSIX path, long enough not to catch a stray "/" in prose.
PATH_RE = re.compile(r"(/[A-Za-z0-9._@%+-]+){2,}")
missing, checked = [], 0

for db in sorted(glob.glob(os.path.join(data, "*.db"))):
    # Copy rather than open read-only: a WAL database whose -shm is owned by a
    # running application refuses a read-only handle, and the seed is meant to
    # be checked while the app is up.
    import shutil, tempfile
    tmp = tempfile.NamedTemporaryFile(suffix=".db", delete=False)
    tmp.close()
    try:
        shutil.copy2(db, tmp.name)
        for ext in ("-wal", "-shm"):
            if os.path.exists(db + ext):
                shutil.copy2(db + ext, tmp.name + ext)
        con = sqlite3.connect(tmp.name)
    except (OSError, sqlite3.Error):
        os.unlink(tmp.name)
        continue
    try:
        tables = [r[0] for r in con.execute(
            "select name from sqlite_master where type='table'")]
        for table in tables:
            try:
                cols = [r[1] for r in con.execute(f'pragma table_info("{table}")')]
                rows = con.execute(f'select {",".join(chr(34)+c+chr(34) for c in cols)} '
                                   f'from "{table}"').fetchall()
            except sqlite3.Error:
                continue
            for row in rows:
                for col, value in zip(cols, row):
                    if not isinstance(value, str):
                        continue
                    for match in PATH_RE.finditer(value):
                        candidate = match.group(0)
                        # Only judge paths that claim to be inside a profile or a
                        # temporary directory: the rest belong to the host.
                        if "/.apollia" not in candidate and "/tmp." not in candidate \
                           and "/var/folders/" not in candidate:
                            continue
                        checked += 1
                        if not os.path.exists(candidate):
                            missing.append(
                                f"{os.path.basename(db)} / {table}.{col}: {candidate}")
    finally:
        con.close()
        for suffix in ("", "-wal", "-shm"):
            try:
                os.unlink(tmp.name + suffix)
            except OSError:
                pass

print(f"{checked} absolute path(s) named by the seeded databases")
if not missing:
    print("every one of them exists on disk")
    sys.exit(0)

print(f"\n{len(missing)} path(s) named in a database do not exist:", file=sys.stderr)
for m in sorted(set(missing)):
    print(f"  {m}", file=sys.stderr)
print("\nA path under /var/folders means the seed was built in a staging "
      "directory and the rows kept the build path instead of the final one. "
      "Rebuild with APOLLIA_SEED_HOME_ALIAS set, which is what load.sh does.",
      file=sys.stderr)
sys.exit(1)
PY
