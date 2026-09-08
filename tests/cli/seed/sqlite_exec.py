#!/usr/bin/env python3
"""Run SQL against a SQLite database, standing in for the `sqlite3` command.

Windows ships no `sqlite3` on PATH, and neither does a minimal Linux image, so
the seeded profile could not be built there at all: the suite refused before a
single assertion, on a machine where everything else was in place. Python
carries SQLite in its standard library, and the suite already resolves an
interpreter for its other controls, so the dependency is one this tree can
drop rather than ask for.

Usage:
    sqlite_exec.py <database> [sql]

With no SQL argument the statements are read from standard input, which is the
shape the seed fragments use.
"""

import sqlite3
import sys


def main() -> int:
    if len(sys.argv) < 2:
        sys.stderr.write("usage: sqlite_exec.py <database> [sql]\n")
        return 2
    database = sys.argv[1]
    script = sys.argv[2] if len(sys.argv) > 2 else sys.stdin.read()
    connection = sqlite3.connect(database)
    try:
        connection.executescript(script)
        connection.commit()
    except sqlite3.Error as error:
        sys.stderr.write(f"{database}: {error}\n")
        return 1
    finally:
        connection.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
