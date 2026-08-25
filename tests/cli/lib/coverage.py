#!/usr/bin/env python3
"""Command-coverage report for the CLI E2E suite, counted on real invocations.

Enumerates every clap command LEAF by walking `apollia-os --help` recursively,
then classifies each against the actual `"$BIN"` / `"${Q[@]}"` invocations of
the track sources:

  * track1    - invoked by the offline track, which runs on every PR
  * opt-in    - invoked only by Track 2 or 3 (gated by APOLLIA_REQUIRE_RUNTIME)
  * NONE      - invoked by no track at all

The previous version of this file matched leaf names anywhere in the track
text, so labels, comments and `skip` lines counted as coverage; `inspect` was
"exercised" by the word inspect inside `memory inspect`. The classifier now
lives in scripts/check_cli_e2e_coverage.py (the enforcing guard, wired into
`just guards` and the CLI E2E CI job) and this report loads it from there, so
the suite's artifact and the guard can never disagree on what counts.

Floor: zero leaves without a track. When violated this exits 1, and the guard
fails wherever it is wired.

Stdlib only.
"""

import argparse
import importlib.util
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]
GUARD = REPO_ROOT / "scripts/check_cli_e2e_coverage.py"


def load_guard():
    spec = importlib.util.spec_from_file_location("check_cli_e2e_coverage", GUARD)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def render(hits: dict[str, list[str]]) -> str:
    total = len(hits)
    track1 = [l for l, names in hits.items() if any(n.startswith("track1") for n in names)]
    optin = [
        l
        for l, names in hits.items()
        if names and all(not n.startswith("track1") for n in names)
    ]
    none = [l for l, names in hits.items() if not names]
    out = ["\n## Command coverage (real invocations)\n"]
    out.append(
        f"- {total} leaf commands: {len(track1)} invoked by Track 1 (every PR), "
        f"{len(optin)} only by Tracks 2/3 (opt-in), {len(none)} by no track\n"
    )
    if none:
        out.append("**Leaves invoked by no track (floor violated):**")
        out.extend(f"- `{leaf}`" for leaf in none)
    return "\n".join(out) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", required=True)
    ap.add_argument("--tracks-dir", required=True)
    ap.add_argument("--append-md", required=True)
    args = ap.parse_args()

    guard = load_guard()
    leaves = guard.enumerate_leaves(args.bin)
    if not leaves:
        with open(args.append_md, "a", encoding="utf-8") as fh:
            fh.write("\n## Command coverage\n\n_Could not enumerate the command tree._\n")
        print("coverage: could not enumerate command tree", file=sys.stderr)
        return 2

    tracks = sorted(Path(args.tracks_dir).glob("*.sh"))
    per_track = {
        t.name: guard.track_invocations(t.read_text(encoding="utf-8")) for t in tracks
    }
    hits = guard.classify(leaves, per_track)
    with open(args.append_md, "a", encoding="utf-8") as fh:
        fh.write(render(hits))

    none = [l for l, names in hits.items() if not names]
    total = len(hits)
    print(
        f"coverage: {total - len(none)}/{total} leaves invoked by a track, "
        f"{len(none)} by none"
    )
    return 1 if none else 0


if __name__ == "__main__":
    sys.exit(main())
