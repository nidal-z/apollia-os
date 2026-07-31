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

# Where a `<!-- claim:id -->` marker may live. The published corpus, its
# translations, and the rulebook the assistants read.
ANCHOR_ROOTS = [
    REPO_ROOT / "docs" / "site" / "docs",
    REPO_ROOT / "docs" / "site" / "i18n",
    REPO_ROOT / "docs" / "agents",
    REPO_ROOT / "docs" / "adr",
]
ANCHOR_EXTRA_GLOBS = ["AGENTS.md", "*/AGENTS.md", "*/*/AGENTS.md", "*/*/*/AGENTS.md"]

VALID_STATUS = {"wired", "not-wired", "absent"}


class Failure(Exception):
    """A claim that no longer holds."""


def strip_test_code(text: str) -> str:
    """Blank out everything from the first `#[cfg(test)]` attribute onward.

    Apollia puts unit tests in a trailing `mod tests` guarded by `#[cfg(test)]`,
    so truncating there removes test callers without needing a Rust parser. The
    trade-off is deliberate: this may keep a little code that follows an inline
    `#[cfg(test)]` block, so the check can report a caller that is really a test.
    It errs toward accepting a claim rather than toward a false alarm, which is
    the right way round for a gate that blocks a release.
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
    for pattern in ANCHOR_EXTRA_GLOBS:
        files.extend(REPO_ROOT.glob(pattern))
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
        for line in body.splitlines():
            if needle in line and not definition.search(line):
                return
    raise Failure(
        f"`{symbol}` is declared `wired` but has no use outside test code. "
        f"Either a caller was removed, or the documentation overstates what "
        f"ships. This is the exact shape of every drift this file exists to stop."
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
