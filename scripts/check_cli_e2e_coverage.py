#!/usr/bin/env python3
"""Every CLI leaf command is invoked, for real, by at least one E2E track.

The previous coverage report (tests/cli/lib/coverage.py before this guard)
matched leaf names anywhere in the track sources, so a `skip` line, a label or
a comment counted as coverage. Under that rule the suite announced 172/199
leaves exercised while 27 of them appeared in no actual command line: `inspect`
was "exercised" by the word inspect inside `memory inspect`, and `start`/`stop`
were "skipped" on a passing mention. This guard counts only invocations that
run the binary: a `"$BIN"` (or the runtime-track prefix `"${Q[@]}"`) followed
by the leaf's verb path, global flags allowed in between. Mentions, labels,
comments and `skip` lines count for nothing.

The leaf inventory is not a checked-in list: it is enumerated from the built
binary by walking `--help` recursively under a throwaway HOME, so a new
subcommand enters the floor the day it is merged, without anyone editing this
file.

Verdict by exit code, since the caller reads it rather than the text:

  0  every leaf has at least one real invocation in some track
  1  at least one leaf is invoked by no track
  2  nothing was measured: the binary is absent (build it with
     `cargo build -p apollia-cli`), the tree walk produced no leaf, or the
     tracks directory holds no track

`--selftest` exercises the classifier on fixtures, in both directions: a
comment mention and a `skip` line must not count, a real call must.

Usage:
    python3 scripts/check_cli_e2e_coverage.py [--bin PATH] [--tracks-dir DIR]
    python3 scripts/check_cli_e2e_coverage.py --selftest
"""

import argparse
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_BIN = REPO_ROOT / "target/debug/apollia-os"
DEFAULT_TRACKS = REPO_ROOT / "tests/cli/tracks"

# Global flags a track may place between the binary and the verb path.
GLOBAL_FLAGS = re.compile(
    r"^(--json|--socket\s+\S+|-q|--quiet|--no-color|--debug|-v|--verbose)\s+"
)

# A real invocation: `"$BIN"`, `'$BIN'`, `$BIN` or the runtime-track prefix
# `"${Q[@]}"`, then everything up to the next invocation on the same line.
# Matching inside a `bash -c "..."` string is wanted: those lines run.
INVOCATION = re.compile(r'(?:["\']?\$BIN["\']?|"\$\{Q\[@\]\}")\s+((?:(?!\$BIN|\$\{Q).)*)')


def help_text(bin_path: str, path: list[str], env: dict) -> str:
    try:
        out = subprocess.run(
            [bin_path, *path, "--help"],
            capture_output=True,
            text=True,
            timeout=20,
            env=env,
        )
    except (subprocess.SubprocessError, OSError):
        return ""
    return (out.stdout or "") + (out.stderr or "")


def subcommands(text: str) -> list[str]:
    lines = text.splitlines()
    try:
        start = next(i for i, ln in enumerate(lines) if ln.strip() == "Commands:")
    except StopIteration:
        return []
    subs = []
    for ln in lines[start + 1 :]:
        if not ln.strip():
            break
        m = re.match(r"^  (\S+)", ln)
        if m and m.group(1) != "help":
            subs.append(m.group(1))
    return subs


def enumerate_leaves(bin_path: str) -> list[list[str]]:
    """Walk `--help` recursively; a node without subcommands is a leaf."""
    with tempfile.TemporaryDirectory(prefix="apollia-cli-cov-") as home:
        env = dict(os.environ, HOME=home, NO_COLOR="1")
        top = subcommands(help_text(bin_path, [], env))
        leaves: list[list[str]] = []
        stack = [[c] for c in top]
        while stack:
            path = stack.pop()
            subs = subcommands(help_text(bin_path, path, env))
            if subs:
                stack.extend(path + [s] for s in subs)
            else:
                leaves.append(path)
    return sorted(leaves)


def track_invocations(text: str) -> list[list[str]]:
    """Verb paths of the real invocations in one track source."""
    invocations = []
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("#") or stripped.startswith("skip "):
            continue
        for m in INVOCATION.finditer(line):
            rest = m.group(1)
            while True:
                g = GLOBAL_FLAGS.match(rest)
                if not g:
                    break
                rest = rest[g.end() :]
            tokens: list[str] = []
            for tok in rest.split():
                if (
                    tok.startswith(("-", "$", '"', "'", "<", ">"))
                    or tok in ("|", "2>", "&&", ";", "||", ")")
                ):
                    break
                tokens.append(tok)
            if tokens:
                invocations.append(tokens)
    return invocations


def covered(leaf: list[str], invocations: list[list[str]]) -> bool:
    return any(toks[: len(leaf)] == leaf for toks in invocations)


def classify(
    leaves: list[list[str]], per_track: dict[str, list[list[str]]]
) -> dict[str, list[str]]:
    """Map each leaf path (joined) to the names of the tracks that invoke it."""
    return {
        " ".join(leaf): [name for name, invs in per_track.items() if covered(leaf, invs)]
        for leaf in leaves
    }


def selftest() -> int:
    failures = []

    def case(name: str, condition: bool) -> None:
        if condition:
            print(f"  ok    {name}")
        else:
            print(f"  FAIL  {name}")
            failures.append(name)

    fixture = "\n".join(
        [
            '# "$BIN" audit journal --json   (a comment is not a call)',
            'skip "audit replay" "a skip line is not a call either"',
            'check "j" "$BIN" audit journal --json',
            'check "r" "${Q[@]}" task resume ghost --approve',
            'check "g" "$BIN" --json --socket "$S" agent list',
            'check_exit "u" 1 bash -c "\'$BIN\' update --yes; exit $?"',
        ]
    )
    invs = track_invocations(fixture)
    per_track = {"fixture.sh": invs}

    hits = classify([["audit", "journal"]], per_track)["audit journal"]
    case("a real call counts", hits == ["fixture.sh"])
    hits = classify([["audit", "replay"]], per_track)["audit replay"]
    case("a skip line does not count", hits == [])
    comment_only = track_invocations('# "$BIN" model search foo')
    case(
        "a comment mention does not count",
        not covered(["model", "search"], comment_only),
    )
    case(
        "the runtime-track prefix counts",
        classify([["task", "resume"]], per_track)["task resume"] == ["fixture.sh"],
    )
    case(
        "global flags before the verbs are stripped",
        classify([["agent", "list"]], per_track)["agent list"] == ["fixture.sh"],
    )
    case(
        "a call inside bash -c counts",
        classify([["update"]], per_track)["update"] == ["fixture.sh"],
    )
    case(
        "an uninvoked leaf is reported uncovered",
        classify([["ghost", "verb"]], per_track)["ghost verb"] == [],
    )

    if failures:
        print(f"\nselftest: {len(failures)} case(s) failed", file=sys.stderr)
        return 1
    print("\nselftest: every case holds")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=str(DEFAULT_BIN))
    parser.add_argument("--tracks-dir", default=str(DEFAULT_TRACKS))
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args()

    if args.selftest:
        return selftest()

    bin_path = Path(args.bin)
    if not bin_path.exists():
        print(
            f"NOTHING MEASURED: {bin_path} is absent, so no leaf was enumerated.\n"
            "                 Build it with: cargo build -p apollia-cli",
            file=sys.stderr,
        )
        return 2

    leaves = enumerate_leaves(str(bin_path))
    if not leaves:
        print("NOTHING MEASURED: the --help walk enumerated no leaf", file=sys.stderr)
        return 2

    tracks = sorted(Path(args.tracks_dir).glob("*.sh"))
    if not tracks:
        print(
            f"NOTHING MEASURED: no track in {args.tracks_dir}",
            file=sys.stderr,
        )
        return 2

    per_track = {t.name: track_invocations(t.read_text(encoding="utf-8")) for t in tracks}
    hits = classify(leaves, per_track)
    uncovered = [leaf for leaf, names in hits.items() if not names]

    total = len(leaves)
    print(f"{total} leaves, {total - len(uncovered)} invoked by at least one track")
    if uncovered:
        print(f"\n{len(uncovered)} leaf/leaves with no real invocation in any track:")
        for leaf in uncovered:
            print(f"  NONE  {leaf}")
        print(
            "\nEvery leaf must be run by tests/cli/tracks/*.sh: a deterministic"
            " error path is enough, a mention or a skip line is not.",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
