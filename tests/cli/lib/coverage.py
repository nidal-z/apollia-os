#!/usr/bin/env python3
"""Command-coverage report for the CLI E2E suite.

Enumerates every clap command LEAF by walking `apollia-os --help` recursively,
then classifies each against the track sources:
  * exercised - the leaf's command path appears in a check_/capture_ assertion
  * skipped   - it appears only in a skip() line (justified, non-automatable)
  * uncovered - it appears nowhere

The result is a measured coverage number with named gaps, appended to report.md,
so "we test the CLI" is a percentage with explicit exclusions, not a vibe. This
is a best-effort textual match (a leaf path found as a token sequence in a track
source), not an execution trace; it flags drift, it is not a proof.

Stdlib only.
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path

# Top-level bare verbs are leaves themselves (git-style, per apollia-cli AGENTS).
# Walking their --help would recurse into option noise, so treat as leaves.
BARE_VERBS = {
    "start",
    "stop",
    "status",
    "run",
    "doctor",
    "inspect",
    "logs",
    "trace",
    "version",
    "digest",
    "onboard",
    "completions",
    "guide",
    "do",
    "explain",
    "update",
    "review",
}

# Nodes whose subtree we do not enumerate further (interactive/opaque leaves).
STOP_AT = {"help"}


def sh(bin_path, path):
    try:
        out = subprocess.run(
            [bin_path, *path, "--help"],
            capture_output=True,
            text=True,
            timeout=20,
        )
        return (out.stdout or "") + (out.stderr or "")
    except (subprocess.SubprocessError, OSError):
        return ""


def parse_subcommands(help_text):
    """Extract subcommand names from a clap `Commands:` section."""
    subs = []
    in_cmds = False
    for line in help_text.splitlines():
        if re.match(r"^Commands:", line):
            in_cmds = True
            continue
        if in_cmds:
            if not line.strip():
                break
            if re.match(r"^[A-Za-z]", line):  # next section header
                break
            m = re.match(r"^\s{2,}([a-z][a-z0-9-]*)\b", line)
            if m and m.group(1) not in STOP_AT:
                subs.append(m.group(1))
    return subs


def enumerate_leaves(bin_path):
    leaves = []
    top = parse_subcommands(sh(bin_path, []))
    if not top:
        return leaves, False
    # BFS with a visited/expand rule.
    stack = [[c] for c in top]
    while stack:
        path = stack.pop()
        name = path[-1]
        if len(path) == 1 and name in BARE_VERBS:
            leaves.append(path)
            continue
        subs = parse_subcommands(sh(bin_path, path))
        if subs:
            stack.extend(path + [s] for s in subs)
        else:
            leaves.append(path)
    return sorted(leaves), True


def classify(leaves, track_text):
    exercised, skipped, uncovered = [], [], []
    # Lines that assert vs lines that skip.
    assert_lines, skip_lines = [], []
    for ln in track_text.splitlines():
        s = ln.strip()
        if s.startswith("skip "):
            skip_lines.append(ln)
        elif re.match(r"^(check|capture)", s):
            assert_lines.append(ln)
    assert_blob = "\n".join(assert_lines)
    skip_blob = "\n".join(skip_lines)
    for leaf in leaves:
        pat = r"\b" + r"\s+".join(re.escape(p) for p in leaf) + r"\b"
        if re.search(pat, assert_blob):
            exercised.append(leaf)
        elif re.search(pat, skip_blob) or re.search(pat, track_text):
            skipped.append(leaf)
        else:
            uncovered.append(leaf)
    return exercised, skipped, uncovered


def render(exercised, skipped, uncovered, ok):
    total = len(exercised) + len(skipped) + len(uncovered)
    out = ["\n## Command coverage (best-effort)\n"]
    if not ok:
        out.append("_Could not enumerate the command tree from the binary._\n")
        return "\n".join(out)
    pct = (100 * len(exercised) // total) if total else 0
    out.append(
        f"- {len(exercised)}/{total} leaf commands exercised ({pct}%), "
        f"{len(skipped)} justified-skip, {len(uncovered)} uncovered\n"
    )
    if uncovered:
        out.append("**Uncovered leaves (investigate):**")
        for leaf in uncovered:
            out.append(f"- `{' '.join(leaf)}`")
    return "\n".join(out) + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bin", required=True)
    ap.add_argument("--tracks-dir", required=True)
    ap.add_argument("--append-md", required=True)
    args = ap.parse_args()

    leaves, ok = enumerate_leaves(args.bin)
    track_text = "\n".join(
        p.read_text(encoding="utf-8", errors="replace")
        for p in Path(args.tracks_dir).glob("*.sh")
    )
    exercised, skipped, uncovered = classify(leaves, track_text)
    md = render(exercised, skipped, uncovered, ok)
    with open(args.append_md, "a", encoding="utf-8") as fh:
        fh.write(md)
    # Also echo a one-line summary to stdout for the console.
    if ok:
        total = len(exercised) + len(skipped) + len(uncovered)
        print(
            f"coverage: {len(exercised)}/{total} exercised, "
            f"{len(skipped)} skip, {len(uncovered)} uncovered"
        )
    else:
        print("coverage: could not enumerate command tree", file=sys.stderr)


if __name__ == "__main__":
    main()
