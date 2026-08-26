#!/usr/bin/env python3
"""One reader for the 800-line module rule, on both sides of the tree.

`docs/agents/FORBIDDEN.md` states it once, in its Rust section: "NEVER modules
> 800 lines outside tests". The frontend obeys the same rule and declares it in
`crates/apollia-desktop/ui/AGENTS.md`, so it is measured here rather than in a
second script that would drift from this one.

What is counted differs by language, because what a line costs differs:

  rust      production lines, test regions excluded, the way
            `check_panic_free.py` classifies them. `crates/*/src/*.rs`.
  frontend  every line of a `.svelte` or `.ts` module under
            `crates/apollia-desktop/ui/src`, tests and type declarations
            excluded by filename. A `.svelte` file is markup, script and style
            at once and no part of it is free; the measure is the one the
            constat was raised on.

Stylesheets are out of scope on purpose. A `.css` file carries no control flow,
and the rule exists for what a reader has to hold in their head.

Both sides are ratchets, and a ratchet is two-sided: a file above the threshold
that no table names is a regression, and a file the table names that has come
back under it is debt paid whose entry must go in the same commit. Without the
second half the table outlives the debt.

`check_rust_rules.py` keeps its `module-size` rule name and delegates here, so
the corpus has one rule and one table rather than two that agree until they do
not.

Usage:
    python3 scripts/check_module_size.py [rust|frontend] [--list] [--selftest]

Exit codes: 0 clean, 1 at least one file breaks the rule, 2 nothing measured.
"""

import argparse
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(Path(__file__).resolve().parent))

THRESHOLD = 800

UI_SUBTREE = "crates/apollia-desktop/ui"

# ── ratchet tables ───────────────────────────────────────────────────────────
# Each entry is debt the tree carried when the table was written. Remove the
# entry in the commit that removes the debt; the run turns red in both
# directions until table and tree agree.

# Rust files over 800 production lines that the tree still owes a split. Empty:
# the fifty-two the table carried have been split, and a new one is a
# regression rather than an entry to add.
RUST_EXEMPT: set[str] = set()

# Rust modules over the threshold that hold one indivisible item, where a split
# would change behaviour rather than move lines. Each one states why in a
# `// REASON:` comment at the top of the file, and the rule reads that marker
# back: an exemption whose justification is deleted goes red, and so does one
# whose file has since come back under the threshold.
RUST_REASONED: set[str] = {
    # 115 externally tagged variants of one serialized enum. Nesting them into
    # sub-enums moves the variant name one level down in the JSON, and the only
    # nesting that preserves the shape (#[serde(untagged)]) rewrites the 967
    # construction sites the tree carries across 90 files.
    "crates/apollia-core/src/events/runtime_event.rs",
}

# Frontend modules allowed to exceed the threshold. Empty: the six that were
# over it were split, and nothing has been granted an exemption since.
FRONTEND_EXEMPT: set[str] = set()


def _two_sided(measured: dict[str, int], exempt: set[str], unit: str) -> list[str]:
    """The findings of one ratchet, both directions, for one measured set."""
    hits = []
    for path in sorted(measured):
        size = measured[path]
        over = size > THRESHOLD
        if over and path not in exempt:
            hits.append(
                f"{path}: {size} {unit} (threshold {THRESHOLD}). Split the module"
            )
        elif not over and path in exempt:
            hits.append(
                f"{path}: listed as exempt but now at {size} {unit}. The debt went "
                f"down: remove the entry in this same commit"
            )
    for path in sorted(exempt - set(measured)):
        hits.append(
            f"{path}: listed as exempt but absent from the inventory. "
            f"Remove the entry in this same commit"
        )
    return hits


# ── rust ─────────────────────────────────────────────────────────────────────


def rule_module_size(sources):
    """The `module-size` rule of `check_rust_rules.py`, kept at its call site.

    `sources` are its `Source` records. Two regimes answer here: `RUST_EXEMPT`,
    the ratchet of debt still owed, read in both directions like every other
    ratchet, and `RUST_REASONED`, the modules that cannot be split, each of
    which has to carry its `REASON:` marker for the entry to hold. The aside
    lists what is over the threshold under either regime, so a reader sees the
    debt shrink and the exemptions stay countable.
    """
    hits, aside = [], []
    for s in sources:
        prod = len(s.prod_lines)
        over = prod > THRESHOLD
        if s.path in RUST_REASONED:
            if not over:
                hits.append(
                    f"{s.path}: exempted from the module-size rule but now at {prod} "
                    f"production lines. Remove the entry from RUST_REASONED in "
                    f"this same commit"
                )
            elif not any("REASON:" in line for line in s.raw_lines[:20]):
                hits.append(
                    f"{s.path}: exempted from the module-size rule without a REASON: "
                    f"comment in its first 20 lines. State why the module cannot be "
                    f"split, or split it"
                )
            else:
                aside.append(f"{s.path}: {prod} production lines, exempted with a reason")
        elif over and s.path not in RUST_EXEMPT:
            hits.append(
                f"{s.path}: {prod} production lines (threshold {THRESHOLD}). "
                f"Split the module"
            )
        elif not over and s.path in RUST_EXEMPT:
            hits.append(
                f"{s.path}: listed as exempt but now at {prod} production lines. "
                f"The debt went down: remove the entry in this same commit"
            )
        elif over:
            aside.append(f"{s.path}: {prod} production lines, listed")
    for path in sorted(RUST_EXEMPT & RUST_REASONED):
        hits.append(
            f"{path}: carried in RUST_EXEMPT and in RUST_REASONED at once. "
            f"A module is either debt on a ratchet or an exemption with a reason, "
            f"not both"
        )
    known = {s.path for s in sources}
    for path in sorted((RUST_EXEMPT | RUST_REASONED) - known):
        table = "RUST_EXEMPT" if path in RUST_EXEMPT else "RUST_REASONED"
        hits.append(
            f"{path}: listed in {table} but absent from the inventory. "
            f"Remove the entry in this same commit"
        )
    return hits, {"listed files still over the threshold (aside)": aside}


def rust_sizes() -> dict[str, int]:
    """Production-line counts of every tracked Rust module."""
    import check_rust_rules  # deferred: that module imports this one

    return {s.path: len(s.prod_lines) for s in check_rust_rules.load()}


def rust_side() -> tuple[dict[str, int], list[str], int]:
    """The Rust side, measured through the rule itself rather than beside it.

    Going through `rule_module_size` is what keeps this entry point and the
    `module-size` rule of `check_rust_rules.py` on the same verdict, the
    `REASON:` marker of a reasoned exemption included.
    """
    import check_rust_rules  # deferred: that module imports this one

    sources = check_rust_rules.load()
    sizes = {s.path: len(s.prod_lines) for s in sources}
    hits, aside = rule_module_size(sources)
    listed = len(aside["listed files still over the threshold (aside)"])
    return sizes, hits, listed


# ── frontend ─────────────────────────────────────────────────────────────────


def frontend_paths() -> list[str]:
    """Desktop UI modules the rule covers, as the index lists them."""
    listing = subprocess.run(
        ["git", "ls-files", "--", f"{UI_SUBTREE}/src"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if listing.returncode != 0:
        return []
    return [
        line
        for line in listing.stdout.split("\n")
        if line.endswith((".svelte", ".ts"))
        and not line.endswith((".test.ts", ".d.ts"))
    ]


def frontend_side() -> tuple[dict[str, int], list[str], int]:
    """The frontend side: every line counted, one flat ratchet."""
    sizes = frontend_sizes()
    hits = _two_sided(sizes, FRONTEND_EXEMPT, "lines")
    listed = sum(1 for p, n in sizes.items() if n > THRESHOLD and p in FRONTEND_EXEMPT)
    return sizes, hits, listed


def frontend_sizes() -> dict[str, int]:
    """Line counts of every desktop UI module the rule covers."""
    sizes = {}
    for path in frontend_paths():
        file = REPO_ROOT / path
        if not file.is_file():
            continue
        with file.open(encoding="utf-8", errors="replace") as handle:
            sizes[path] = sum(1 for _ in handle)
    return sizes


# ── selftest ─────────────────────────────────────────────────────────────────


def _selftest() -> int:
    """Drive the ratchet from both sides, on a fixture rather than on the tree.

    A guard that only ever ran against a clean tree would prove that it ran, not
    that it catches anything.
    """
    failures = []

    def control(label: str, got, want) -> None:
        mark = "ok  " if got == want else "FAIL"
        if got != want:
            failures.append(f"{label}: got {got!r}, want {want!r}")
        print(f"  {mark} {label}")

    print("selftest: the ratchet answers on both sides")
    over = {"a/big.rs": THRESHOLD + 1}
    under = {"a/small.rs": THRESHOLD}
    control("a file over the threshold is a finding", len(_two_sided(over, set(), "l")), 1)
    control("a file at the threshold is not", len(_two_sided(under, set(), "l")), 0)
    control(
        "an exempt file over the threshold is silent",
        len(_two_sided(over, {"a/big.rs"}, "l")),
        0,
    )
    control(
        "an exempt file back under the threshold is a finding",
        len(_two_sided(under, {"a/small.rs"}, "l")),
        1,
    )
    control(
        "an exempt file the inventory no longer holds is a finding",
        len(_two_sided({}, {"a/gone.rs"}, "l")),
        1,
    )

    print("selftest: the frontend inventory is the tracked one")
    paths = frontend_paths()
    control("the subtree is listed", len(paths) > 0, True)
    control(
        "no test module is measured",
        [p for p in paths if p.endswith(".test.ts")],
        [],
    )
    control(
        "no stylesheet is measured",
        [p for p in paths if p.endswith(".css")],
        [],
    )

    if failures:
        print("\nSELFTEST FAILED")
        for line in failures:
            print(f"  {line}")
        return 1
    print("\nselftest: the ratchet is driven from both sides")
    return 0


# ── entry point ──────────────────────────────────────────────────────────────

SIDES = {"rust": rust_side, "frontend": frontend_side}


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "sides", nargs="*", choices=list(SIDES), metavar="side",
        help="rust, frontend, or neither for both",
    )
    parser.add_argument(
        "--list", action="store_true", help="print every finding instead of the first eight"
    )
    parser.add_argument(
        "--selftest", action="store_true",
        help="replay the fixture controls instead of measuring the tree",
    )
    args = parser.parse_args(argv[1:])
    if args.selftest:
        return _selftest()

    worst = 0
    measured_anything = False
    for side in args.sides or list(SIDES):
        sizes, hits, listed = SIDES[side]()
        if not sizes:
            print(f"nothing measured: no {side} module found", file=sys.stderr)
            worst = max(worst, 2)
            continue
        measured_anything = True
        print(f"\n== {side}: {len(sizes)} module(s) measured, {len(hits)} finding(s)")
        for h in hits if args.list else hits[:8]:
            print(f"  {h}")
        if not args.list and len(hits) > 8:
            print(f"  ... {len(hits) - 8} more (--list)")
        print(f"  -- exempt and still over the threshold: {listed}")
        if hits:
            worst = max(worst, 1)
    if not measured_anything:
        return 2
    if worst == 0:
        print(f"\nno module exceeds {THRESHOLD} lines outside its table")
    return worst


if __name__ == "__main__":
    sys.exit(main(sys.argv))
