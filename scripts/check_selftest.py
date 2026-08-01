#!/usr/bin/env python3
"""Pin the one bias that got past two independent checks.

Twice in this repository a check answered "this capability is wired" because the
symbol's name appeared in a doc-comment. `check_optional_builders.py` counted
`/// chain [`X::with_y`] to enable ...` as a call site, which hid
`with_permission_engine`, the original instance of the whole class.
`check_claims.py` did the same through its `status = "wired"` rule, which
excluded the definition line but not the comment above it, and passed a claim
asserting a rollback journal that nothing ever wrote.

Both filters are in place now. That is not what this file is for. Two scripts
written two days apart carried the same bias, always in the direction that
declares a dead capability alive, so the property worth pinning is not "the fix
was applied" but "a symbol that appears only in a comment is reported as not
wired". A fixture states that as a permanent expectation. A mutation run states
it once, on the day someone remembers to run it.

Each case below asserts both directions. A check that always says "not wired"
would satisfy the negative half while being useless, so every negative case is
paired with a positive control that must pass.

Usage:
    python3 scripts/check_selftest.py
"""

import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import check_claims  # noqa: E402
import check_optional_builders as builders  # noqa: E402

FAILURES: list[str] = []


def case(name: str, condition: bool, detail: str) -> None:
    if condition:
        print(f"  ok    {name}")
    else:
        print(f"  FAIL  {name}")
        FAILURES.append(f"{name}: {detail}")


# ── The builder sweep ────────────────────────────────────────────────────────

COMMENT_ONLY = """\
pub struct Thing {
    cap: Option<Cap>,
}

impl Thing {
    /// Install the capability.
    ///
    /// Chain [`Thing::with_cap`] to enable it. Callers that skip
    /// `.with_cap(...)` get the inert default.
    pub fn with_cap(mut self, cap: Cap) -> Self {
        self.cap = Some(cap);
        self
    }
}
"""

REAL_CALLER = """\
pub fn build() -> Thing {
    Thing::default().with_cap(Cap::new())
}
"""


def check_builder_sweep() -> None:
    print("builder sweep: a name seen only in a doc-comment is not a caller")
    only_comment = {Path("crates/x/src/thing.rs"): COMMENT_ONLY}
    case(
        "comment-only mention yields no caller",
        builders.production_callers("with_cap", only_comment) == [],
        "a doc-comment was counted as a call site, which is how "
        "`with_permission_engine` passed as wired",
    )

    with_caller = dict(only_comment)
    with_caller[Path("crates/y/src/boot.rs")] = REAL_CALLER
    found = builders.production_callers("with_cap", with_caller)
    case(
        "positive control: a real call site is found",
        len(found) == 1 and "boot.rs" in found[0],
        f"expected exactly one caller in boot.rs, got {found!r}. A check that "
        f"never finds a caller would satisfy the case above and be worthless",
    )

    in_tests = dict(only_comment)
    in_tests[Path("crates/y/tests/it.rs")] = REAL_CALLER
    case(
        "a caller under tests/ does not count as production",
        builders.production_callers("with_cap", in_tests) == [],
        "an integration test was accepted as a production caller",
    )


# ── The claims replay ────────────────────────────────────────────────────────

CLAIM_COMMENT_ONLY = """\
/// The engine consults [`PlanCache::lookup`] before planning.
///
/// See `lookup` for the key derivation.
pub fn lookup(&self, key: &str) -> Option<Plan> {
    None
}
"""

CLAIM_REAL_USE = """\
pub fn run(cache: &PlanCache) -> Option<Plan> {
    cache.lookup("k")
}
"""


def check_claims_wired() -> None:
    print("claims replay: `wired` is not satisfied by a doc-comment")
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        only = root / "only_comment.rs"
        only.write_text(CLAIM_COMMENT_ONLY, encoding="utf-8")
        claim = {"id": "fixture", "symbol": "PlanCache::lookup", "status": "wired"}

        raised = False
        try:
            check_claims.check_wired(claim, [only])
        except check_claims.Failure:
            raised = True
        case(
            "comment-only symbol fails the `wired` check",
            raised,
            "the doc-comment above the definition satisfied `wired`, which is "
            "exactly how the rollback-journal claim stayed green",
        )

        used = root / "real_use.rs"
        used.write_text(CLAIM_REAL_USE, encoding="utf-8")
        raised = False
        try:
            check_claims.check_wired(claim, [only, used])
        except check_claims.Failure:
            raised = True
        case(
            "positive control: a real use satisfies `wired`",
            not raised,
            "a genuine call site was rejected, which would make every `wired` "
            "claim unprovable and the check meaningless",
        )


def main() -> int:
    check_builder_sweep()
    check_claims_wired()
    if FAILURES:
        print(f"\n{len(FAILURES)} self-test failure(s):\n", file=sys.stderr)
        for f in FAILURES:
            print(f"  {f}\n", file=sys.stderr)
        return 1
    print("\nthe comment-only bias is closed in both checks")
    return 0


if __name__ == "__main__":
    sys.exit(main())
