#!/usr/bin/env python3
"""The binary a guard judges must be the one the tree under judgement produces.

Four guards render their verdict by running `target/debug/apollia-os`:
`check_cli_json_contract.py`, `check_cli_e2e_coverage.py`,
`check_entry_doc_commands.py`, and `tests/cli/cli-e2e.sh`, which
`check_instrument_verdicts.py` drives in turn. Each of them tested that the
file exists, and nothing else, so what they described was whatever artefact
happened to sit at that path.

It happened five times in one release campaign, on waves 3, 4, 6, 7 and 8a,
and every time the output was precise, plausible and about another tree: 183
contract breaches, 6 documentation lines refused, 198 leaves instead of 199,
PASS 156 FAIL 16 over the CLI suite. `cargo build -p apollia-cli --bin
apollia-os` turned all of it green without one file of the tree changing.

Two shapes, both outside the tree under judgement, and one measurement worth
keeping. The artefact that kept coming back weighs 33 822 656 bytes and carries
448 symbols and no debug info, where a build of this workspace's dev profile
(`debug = true` since the first commit) weighs 159 MB and carries 1.5 M
symbols: it is a stripped release build, so `cargo build -p apollia-cli` never
produced it. The campaign observed the swap happening during a `cargo test
--workspace` and attributed it to the workspace feature unification; the size
says the full mechanism is not established. The second shape is documented on
its own: the `CARGO_TARGET_DIR` is shared with the other working trees, one of
which carries pre-campaign code.

Nothing below rests on knowing which command writes the file. What the control
establishes is narrower and sufficient: the artefact at that path is not what a
build of this tree produces.

## The criterion, and what was set aside

`apollia-os --version` was the first candidate, since a marker the artefact
carries beats a comparison of timestamps. Measurement refuted it: the string it
publishes, `0.1.0-preview`, was set on 2026-08-11 by `b51b8b55`, and 283
commits have landed since, 149 of them touching `crates/`. It separates a
binary older than 2026-08-11 from a newer one and nothing finer, so it cannot
tell this tree from any of those 149 states. Stamping the git revision into the
binary at build time would answer, and it is set aside on purpose: it changes a
published surface, it makes every commit rebuild the CLI, and an instrument
repair is not the place to decide that.

What cargo already writes is enough. Beside every artefact it produces, it
writes a dep-info file, `<binary>.d`, naming by absolute path every source the
build read: 531 of them for this binary, the `.rs` files, the `include_str!`
prompts, the SQL migrations and the packaging manifest. Two predicates follow
from it, one per measured cause.

  P1  provenance. The dep-info names this tree's own
      `crates/apollia-cli/src/main.rs`. A build launched in another working
      tree of the same repository names that tree's path instead. That is
      occurrence 4: a build in `.claude/worktrees/poc-vlm` rewrote the shared
      `target/debug/apollia-os` with pre-campaign code, and no timestamp could
      have said so, the artefact being the newest of the files involved.
  P2  currency. No source the dep-info names is newer than the binary, and
      none has disappeared since. That is occurrence 5, where the artefact of
      2026-08-06 came back under a tree whose sources had moved on, and it is
      the ordinary case as well: an edit, or a checkout, after the build.

A third predicate was written, measured and removed: "the binary is not older
than the dep-info that describes it". Cargo writes the dep-info after it
uplifts the artefact, 32 ms after it on this tree, so a correct build fails
that test. Its intent is covered by P2, since an artefact that predates the
tree predates a source of it.

Two bounds, written here rather than discovered later.

A source edited while the build runs keeps a timestamp older than the
artefact, and no comparison of timestamps can see it. Cargo carries the same
blind spot on the same data.

An artefact rebuilt from the same sources under different feature flags, which
a workspace-wide test build produces by unifying them, is newer than every
source and comes from this tree, so it satisfies both predicates. Provenance
and currency are what a dep-info can prove; the feature set the build used is
written nowhere this control can read.

## Exit codes

  0  the binary corresponds to this tree; the coverage measured is printed
  2  nothing measured, with the command that repairs it

Never 1. A binary that does not match the tree is not a defect of the tree, and
reporting it as one is the whole defect this file exists to end.

Usage:
    python3 scripts/binary_freshness.py [--bin PATH] [--tree PATH]
"""

import argparse
import re
import sys
import time
from pathlib import Path
from typing import List, NamedTuple, Optional, Tuple

# `Optional[...]` rather than `X | None`, and `List`/`Tuple` rather than the
# builtins in an evaluated position: tests/cli/cli-e2e.sh runs this module
# under `/usr/bin/python3`, which is 3.9.6 on the macOS of this tree, and PEP
# 604 unions raise a TypeError there at import time. A shared control runs
# under the oldest interpreter of its callers or it is not shared.

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_BIN = REPO_ROOT / "target/debug/apollia-os"

# The entry point of the crate that produces the binary. A build of this tree
# names this exact absolute path; a build of any other tree names its own.
ANCHOR = "crates/apollia-cli/src/main.rs"

REMEDY = "cargo build -p apollia-cli --bin apollia-os"
NOTHING_MEASURED = 2

# Makefile rules split their prerequisites on spaces, and a space inside a path
# is escaped with a backslash.
UNESCAPED_SPACE = re.compile(r"(?<!\\) ")


class Verdict(NamedTuple):
    """Why the binary cannot be judged, or None when it can."""

    reason: Optional[str]
    detail: Tuple[str, ...] = ()
    sources: int = 0
    newest: Optional[str] = None


def dep_info_of(binary: Path) -> Path:
    """The dep-info file cargo writes beside the artefact."""
    return binary.with_suffix(".d")


def dep_sources(text: str) -> List[Path]:
    """Every prerequisite of every rule in a dep-info file.

    Cargo also writes rules with an empty right-hand side, one per source, so a
    line without prerequisites carries nothing and is skipped rather than read
    as a target.
    """
    found: List[Path] = []
    for raw in text.splitlines():
        line = raw.strip()
        _, sep, prerequisites = line.partition(": ")
        if not sep:
            continue
        for token in UNESCAPED_SPACE.split(prerequisites):
            path = token.replace("\\ ", " ").strip()
            if path:
                found.append(Path(path))
    return found


def _stamp(mtime: float) -> str:
    return time.strftime("%Y-%m-%d %H:%M:%S", time.localtime(mtime))


def check(binary: Path, tree: Path) -> Verdict:
    """Does this binary come from this tree, and from its current state?"""
    binary = Path(binary)
    tree = Path(tree)

    if not binary.is_file():
        return Verdict(f"{binary} is absent, so no command was run")

    anchor = (tree / ANCHOR).resolve()
    if not anchor.is_file():
        # Without the anchor the provenance predicate has nothing to recognise
        # a build of this tree by, and would pass everything. A control that
        # stopped controlling reports zero coverage, it does not pass.
        return Verdict(
            f"{ANCHOR} is absent from {tree}",
            (
                "the provenance of the binary is decided on that path, so "
                "nothing here could tell one tree from another",
            ),
        )

    dep_info = dep_info_of(binary)
    if not dep_info.is_file():
        return Verdict(
            f"{dep_info} is absent",
            (
                f"nothing states which sources produced {binary.name}, so no "
                f"verdict about this tree can rest on it",
            ),
        )

    sources = dep_sources(dep_info.read_text(encoding="utf-8", errors="replace"))
    if not sources:
        return Verdict(
            f"{dep_info} names no source",
            ("an empty dep-info states nothing about the artefact beside it",),
        )

    if anchor not in {source.resolve() for source in sources}:
        foreign = next(
            (s for s in sources if s.as_posix().endswith(ANCHOR)),
            None,
        )
        seen = (
            f"its dep-info names {foreign}"
            if foreign is not None
            else "its dep-info names no apollia-cli entry point at all"
        )
        return Verdict(
            f"{binary} was not built from {tree}",
            (seen, f"expected {anchor}"),
        )

    binary_mtime = binary.stat().st_mtime
    newer: List[Tuple[float, Path]] = []
    vanished: List[Path] = []
    newest = 0.0
    for source in sources:
        try:
            mtime = source.stat().st_mtime
        except OSError:
            vanished.append(source)
            continue
        newest = max(newest, mtime)
        if mtime > binary_mtime:
            newer.append((mtime, source))

    if vanished:
        return Verdict(
            f"{len(vanished)} source(s) the build read no longer exist",
            tuple(str(path) for path in vanished[:3]),
        )

    if newer:
        newer.sort(reverse=True)
        return Verdict(
            f"{len(newer)} source(s) changed after {binary} was built",
            tuple(f"{_stamp(m)}  {p}" for m, p in newer[:3])
            + (f"binary built {_stamp(binary_mtime)}",),
        )

    return Verdict(None, sources=len(sources), newest=_stamp(newest))


def refuse(verdict: Verdict, stream=sys.stderr) -> int:
    """Print the refusal and the command that repairs it, and answer 2."""
    print(f"NOTHING MEASURED: {verdict.reason}", file=stream)
    for line in verdict.detail:
        print(f"                  {line}", file=stream)
    print(f"                  Rebuild it with: {REMEDY}", file=stream)
    return NOTHING_MEASURED


def require(
    binary: Path,
    tree: Path = REPO_ROOT,
    *,
    stream=sys.stderr,
    report=sys.stdout,
) -> Optional[int]:
    """None when the binary may be judged, 2 when it may not, refusal printed.

    The caller returns that 2 as its own exit code: what it was about to
    measure is not the tree it names.
    """
    verdict = check(binary, tree)
    if verdict.reason is not None:
        return refuse(verdict, stream)
    try:
        shown = Path(binary).relative_to(tree)
    except ValueError:
        shown = Path(binary)
    print(
        f"binary: {shown}, built from this tree, {verdict.sources} sources, "
        f"newest {verdict.newest}",
        file=report,
    )
    return None


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Prove the apollia-os binary was produced by this working tree."
    )
    parser.add_argument("--bin", default=str(DEFAULT_BIN), help="path to the binary")
    parser.add_argument(
        "--tree", default=str(REPO_ROOT), help="the working tree it must come from"
    )
    args = parser.parse_args()
    refused = require(Path(args.bin), Path(args.tree))
    return 0 if refused is None else refused


if __name__ == "__main__":
    sys.exit(main())
