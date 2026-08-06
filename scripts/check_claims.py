#!/usr/bin/env python3
"""Replay every capability claim of the documentation against the code.

The corpus repeatedly stated that a capability was active when the symbol behind
it existed but nothing ever called it. A permission engine documented across
five pages runs in no shipped binary; five `[observability]` capture switches had
a settings page and no reader. Prose review never catches this, because the prose
is internally coherent: only the code disagrees.

`docs/CLAIMS.toml` records one entry per capability claim. This script replays
each one and fails the build when a claim and the code diverge.

Three checks, none of which uses a line number:

1. **Anchor.** Each claim carries an `id`, and the documented paragraph carries a
   matching `<!-- claim:<id> -->` HTML comment (invisible once rendered). The
   comment survives a page move, a merge, and a translation; a line number
   survives none of those. A missing anchor means the paragraph was deleted,
   renamed or translated without its marker, and the claim now guards nothing.

2. **`status = "wired"`.** The symbol must appear in `crates/` outside test code.
   A definition alone is not enough: a caller is required.

3. **`status = "not-wired"`.** The `proof` file must still contain the evidence
   string. When it stops matching, the situation changed and the documentation
   that says "not available" has to be revisited. A claim of absence rots exactly
   like a claim of presence.

4. **`status = "absent"`.** The symbol must have no occurrence at all outside
   test code. This is the inverse of `wired`, and it exists for a case the corpus
   really has: a decision whose content is a removal. There, the absence of the
   symbol *is* the delivery, and nothing else in the build would notice its
   silent return.

Usage:
    python3 scripts/check_claims.py            # replay, exit 1 on divergence
    python3 scripts/check_claims.py --list     # print the inventory
"""

import argparse
import re
import subprocess
import sys
import tomllib
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
CLAIMS_FILE = REPO_ROOT / "docs" / "CLAIMS.toml"

# Where a `<!-- claim:id -->` marker may live: the published corpus, its
# translations, and the decision record.
#
# The rulebook (`docs/agents/` and the `AGENTS.md` files) is deliberately absent,
# and this is a constraint rather than an oversight. That corpus is removed when
# the public repository is seeded, so a marker placed there would vanish with it
# and take a claim's only anchor along. Keeping the anchor roots to files that
# survive publication means the gate cannot be broken by a packaging step.
#
# A rule in the rulebook that asserts a capability therefore states it by
# pointing at the published page that carries the claim, rather than by carrying
# a marker of its own. Nothing is lost: the claim is still replayed, and the
# rulebook stops being load-bearing for a gate it cannot guarantee to be part of.
ANCHOR_ROOTS = [
    REPO_ROOT / "docs" / "site" / "docs",
    REPO_ROOT / "docs" / "site" / "i18n",
]

# A re-export names a symbol without reading it, so it must not satisfy
# `wired`. See the note at the call site.
RE_EXPORT = re.compile(r"(pub\s+)?use\s+")

VALID_STATUS = {"wired", "not-wired", "absent"}


class Failure(Exception):
    """A claim that no longer holds."""


def strip_test_code(text: str) -> str:
    """Drop everything from the first `#[cfg(test)]` attribute onward.

    Apollia puts unit tests in a trailing `mod tests` guarded by `#[cfg(test)]`,
    so cutting there removes test callers without needing a Rust parser.

    Know which way this errs before trusting it. The cut is at the *first*
    marker, so any production code that happens to follow an inline
    `#[cfg(test)]` block in the same file is dropped too. `check_wired` returns
    on the first use it finds, so fewer lines to search means a higher chance of
    finding none: **the bias is toward a false alarm**, a claim reported broken
    while its caller exists just below a test block.

    In practice the trailing-`mod tests` convention keeps this rare. When it does
    bite, the fix is to move the production code above the test module rather
    than to loosen the check: a gate that cries wolf is the one that ends up
    behind `continue-on-error`, and this repository has had to repair that twice.
    """
    marker = text.find("#[cfg(test)]")
    return text if marker == -1 else text[:marker]


def rust_sources() -> list[Path]:
    out = subprocess.run(
        ["git", "ls-files", "crates/**/*.rs", "sdk/**/*.py"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    return [REPO_ROOT / line for line in out.stdout.splitlines() if line]


def anchor_files() -> list[Path]:
    files: list[Path] = []
    for root in ANCHOR_ROOTS:
        if root.is_dir():
            files.extend(root.rglob("*.md"))
    return files


def check_anchor(claim: dict, anchors: dict[str, list[Path]]) -> None:
    cid = claim["id"]
    if cid not in anchors:
        raise Failure(
            f"no page carries `<!-- claim:{cid} -->`. The paragraph this claim "
            f"guards was moved, renamed, translated or deleted without its "
            f"marker. Restore the marker, or drop the claim."
        )


def check_wired(claim: dict, sources: list[Path]) -> None:
    symbol = claim["symbol"]
    # The last path segment: `PermissionEngine::decide` is called as `decide`
    # on a value, so the fully qualified form rarely appears at a call site.
    needle = symbol.split("::")[-1]
    definition = re.compile(
        rf"\b(fn|struct|enum|trait|type|const|static)\s+{re.escape(needle)}\b"
    )
    for path in sources:
        try:
            body = strip_test_code(path.read_text(encoding="utf-8", errors="ignore"))
        except OSError:
            continue
        in_use_block = False
        for line in body.splitlines():
            # A comment naming the symbol is not a use. Without this, a claim
            # stays green on the strength of the doc-comment that sits directly
            # above the definition, which is the one place the name is certain
            # to appear. `with_journal` passed that way while no production site
            # called it, and the corpus promised an undo that could never run.
            stripped = line.lstrip()
            if stripped.startswith("//"):
                continue
            # A re-export is not a read either. `pub use crate::x::SYMBOL;`
            # names the symbol on a line that is neither a comment nor a
            # definition, so it satisfied this check on its own: a constant
            # could be defined, re-exported, and read by nobody, and the claim
            # stayed green. Found by mutating the one production read of
            # MICROSOFT_DEFAULT_CLIENT_ID: the guard stayed green on the
            # strength of `lib.rs` alone.
            #
            # The block form costs the state below. `pub use foo::{A, B, C};`
            # spread over several lines puts the symbol on a continuation line
            # that starts with neither `use` nor `//`, which is exactly the
            # shape this crate uses, so the one-line filter missed it.
            if RE_EXPORT.match(stripped):
                in_use_block = "{" in line and "}" not in line
                continue
            if in_use_block:
                if "}" in line:
                    in_use_block = False
                continue
            if needle in line and not definition.search(line):
                return
    raise Failure(
        f"`{symbol}` is declared `wired` but has no use outside test code. "
        f"Either a caller was removed, or the documentation overstates what "
        f"ships. This is the exact shape of every drift this file exists to stop."
    )


def check_absent(claim: dict, sources: list[Path]) -> None:
    """The symbol must not appear anywhere in the sources.

    Reintroducing a symbol a decision removed is invisible: it compiles, tests
    pass, and the page that recorded the removal keeps saying it is gone.

    Unlike the other checks this one does **not** call `strip_test_code`, and the
    two err in opposite directions. For `wired`, cutting at the first
    `#[cfg(test)]` removes candidate lines, so it can report a claim broken while
    its caller sits just below a test block: a false alarm. For `absent`, cutting
    would hide a symbol reintroduced after a test module: a missed regression,
    and this check exists precisely to catch that. So it reads the whole file.

    A removal decision also removes the tests that exercised the symbol, so a hit
    anywhere is worth a look. False alarm is the right failure mode here, because
    the alternative is silence on the one thing nothing else can see.
    """
    symbol = claim["symbol"]
    for path in sources:
        try:
            body = path.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        for number, line in enumerate(body.splitlines(), start=1):
            if symbol in line:
                raise Failure(
                    f"`{symbol}` is declared removed but appears at "
                    f"{path.relative_to(REPO_ROOT)}:{number}. Either it came "
                    f"back, in which case the decision that removed it needs "
                    f"revisiting, or this entry is stale."
                )


def check_not_wired(claim: dict) -> None:
    proof_path = REPO_ROOT / claim["proof"]
    if not proof_path.is_file():
        raise Failure(f"proof file `{claim['proof']}` no longer exists")
    body = proof_path.read_text(encoding="utf-8", errors="ignore")
    evidence = claim["evidence"]
    if evidence not in body:
        raise Failure(
            f"`{claim['proof']}` no longer contains the evidence "
            f"{evidence!r}. The code moved: recheck whether "
            f"`{claim['symbol']}` is now reachable, then update both the "
            f"documentation and this entry."
        )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--list", action="store_true", help="print the inventory")
    args = parser.parse_args()

    if not CLAIMS_FILE.is_file():
        print(f"error: {CLAIMS_FILE} not found", file=sys.stderr)
        return 1

    with CLAIMS_FILE.open("rb") as handle:
        claims = tomllib.load(handle).get("claim", [])

    if args.list:
        for claim in claims:
            print(f"{claim['status']:>10}  {claim['id']:<44} {claim['symbol']}")
        print(f"\n{len(claims)} claims")
        return 0

    # Index every marker once rather than re-scanning per claim.
    anchors: dict[str, list[Path]] = {}
    pattern = re.compile(r"<!--\s*claim:([a-z0-9-]+)\s*-->")
    for path in anchor_files():
        try:
            for found in pattern.findall(path.read_text(encoding="utf-8", errors="ignore")):
                anchors.setdefault(found, []).append(path)
        except OSError:
            continue

    sources = rust_sources()
    seen: set[str] = set()
    failures: list[tuple[str, str]] = []

    for claim in claims:
        cid = claim.get("id", "<missing id>")
        try:
            if cid in seen:
                raise Failure("duplicate id")
            seen.add(cid)
            if claim.get("status") not in VALID_STATUS:
                raise Failure(
                    f"status {claim.get('status')!r} is not one of {sorted(VALID_STATUS)}"
                )
            check_anchor(claim, anchors)
            if claim["status"] == "wired":
                check_wired(claim, sources)
            elif claim["status"] == "not-wired":
                check_not_wired(claim)
            elif claim["status"] == "absent":
                check_absent(claim, sources)
        except (Failure, KeyError) as exc:
            failures.append((cid, str(exc)))

    orphans = sorted(set(anchors) - {c.get("id") for c in claims})
    for orphan in orphans:
        where = ", ".join(str(p.relative_to(REPO_ROOT)) for p in anchors[orphan])
        failures.append((orphan, f"marker present in {where} with no entry in CLAIMS.toml"))

    if failures:
        print(f"{len(failures)} claim(s) no longer hold:\n", file=sys.stderr)
        for cid, reason in failures:
            print(f"  {cid}\n      {reason}\n", file=sys.stderr)
        return 1

    print(f"{len(claims)} claims replayed, all hold.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
