#!/usr/bin/env python3
"""Pin the biases the verifiers themselves keep producing.

Three checks were written for this corpus, and all three shipped with a bias on
the same side: the side that reports success.

  - `check_optional_builders.py` counted `/// chain [`X::with_y`] ...` as a call
    site, which hid `with_permission_engine`, the original instance of the very
    class it exists to catch.
  - `check_claims.py` did the same through its `status = "wired"` rule, and
    passed a claim asserting a rollback journal that nothing ever wrote.
  - `check_claim_anchors.py` counted only its failures, so its mirror rule
    announced "NO COVERAGE" while passing, and went on announcing it once six
    claims really were mirrored.

Three verifiers, three biases, one direction. Correcting each instance does
nothing for the fourth verifier someone writes next, so this file pins the
properties rather than the fixes:

  1. A symbol that appears only in a comment is reported as NOT wired.
  2. A rule that examined nothing reports zero coverage, never a pass, and the
     per-zone breakdown accounts for every marker.
  3. A detector that only ever runs against a clean tree fires on a dirty one.
     `check_no_font_cdn.py` guards a promise no CSP covers on the documentation
     site, and its whole tree is compliant today, so a green line proves the
     scan ran, not that the detector works. Both directions are asserted here.

Each case asserts both directions. A check that always answered "not wired", or
that printed a coverage table of zeros, would satisfy the negative half while
being worthless, so every negative case is paired with a positive control.

Usage:
    python3 scripts/check_selftest.py
"""

import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(Path(__file__).resolve().parent))

import check_claims  # noqa: E402
import check_no_font_cdn as fontcdn  # noqa: E402
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

        # Same property, other disguise. A re-export names the symbol on a
        # line that is neither a comment nor a definition, so it satisfied
        # `wired` on its own: a constant could be defined, re-exported and read
        # by nobody. Both forms are covered, because this crate writes the
        # block one, where the symbol lands on a continuation line starting
        # with neither `use` nor `//`.
        for label, body in (
            ("one-line", "pub use crate::cache::PlanCache;\n"),
            (
                "block",
                "pub use crate::cache::{\n    Other,\n    PlanCache,\n};\n",
            ),
        ):
            reexport = root / f"reexport_{label}.rs"
            reexport.write_text(body, encoding="utf-8")
            raised = False
            try:
                check_claims.check_wired(claim, [only, reexport])
            except check_claims.Failure:
                raised = True
            case(
                f"{label} re-export alone fails the `wired` check",
                raised,
                "a re-export satisfied `wired`, so a symbol defined and "
                "exported but read by nobody would keep its claim green",
            )

        # `strip_test_code` cuts a file at its FIRST `#[cfg(test)]`, so a real
        # call site sitting below an inline test item is invisible. That was
        # raised as a blind spot; it is a blind spot, but pin its direction
        # rather than trust the prose: dropping lines can only make
        # `check_wired` find fewer uses, so it errs toward a false alarm, never
        # toward a claim that passes on nothing. A gate that cries wolf is the
        # one that ends up behind continue-on-error, which this repo has had to
        # repair twice, so the day someone loosens the cut this case says which
        # way the loosening went.
        below = root / "use_below_inline_test.rs"
        below.write_text(
            "#[cfg(test)]\nconst FIXTURE: u8 = 1;\n\n"
            "fn caller() {\n    PlanCache::lookup(key);\n}\n",
            encoding="utf-8",
        )
        raised = False
        try:
            check_claims.check_wired(claim, [below])
        except check_claims.Failure:
            raised = True
        case(
            "a use below an inline `#[cfg(test)]` errs toward alarm, not silence",
            raised,
            "the truncation started letting through code it used to cut, which "
            "flips the bias from false alarm to false pass and makes every "
            "`wired` claim satisfiable by a test fixture",
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


# ── The general property, not the instance ───────────────────────────────────


def check_zero_coverage_is_reported() -> None:
    """A rule that examined nothing must say so, never report success.

    Three verifiers, three biases, every one of them on the side that reports
    success. The comment bias made a dead capability look wired, twice. The third
    was subtler and belongs to the same family: the mirror rule of
    `check_claim_anchors.py` counted only its failures, so it printed "NO
    COVERAGE" while quietly passing, and later, once six claims really were
    mirrored, it still printed "NO COVERAGE" because a correctly mirrored claim
    incremented nothing. Both readings were wrong in the same direction, and a
    green line stood over work nobody had checked.

    Fixing each instance does not protect the fourth verifier. What generalises
    is the property: **coverage is reported, and zero coverage is reported as
    zero, never as a pass**. A rule with nothing to examine is not a rule that
    holds; it is a rule that has not run.

    This checks the property where it can be checked mechanically: every zone
    line printed by the anchors check states a count, and a zone with no markers
    is flagged rather than passed over in silence.
    """
    print("coverage reporting: a rule that examined nothing says so")

    out = subprocess.run(
        [sys.executable, str(REPO_ROOT / "scripts" / "check_claim_anchors.py")],
        capture_output=True, text=True,
    )
    report = out.stdout

    # The header alone is not the report. Assert the rows, and assert they
    # account for every marker: a breakdown that silently omits a zone is the
    # same failure as a rule that examines nothing, one level up.
    total = re.search(r"(\d+) claims, (\d+) markers", report)
    rows = re.findall(r"^\s{2}(\S[^\n]*?)\s+(\d+)\s+(\d+)(?:\s|$)", report, re.M)
    case(
        "the per-zone table has rows, not just a header",
        len(rows) >= 3,
        f"only {len(rows)} data row(s) parsed. A header with no rows reads as a "
        f"coverage report and states nothing",
    )
    case(
        "the per-zone counts account for every marker",
        bool(total) and sum(int(m) for _, _, m in rows) == int(total.group(2)),
        f"rows sum to {sum(int(m) for _, _, m in rows)}, total claims "
        f"{total.group(2) if total else '?'}. A breakdown that drops a zone hides "
        f"exactly the zone nobody looked at",
    )

    zero_zones = [
        line for line in report.splitlines()
        if re.match(r"^\s+\S.*\s\d+\s+0(\s|$)", line)
    ]
    case(
        "a zone with zero markers is flagged, not passed over",
        all("no coverage" in line for line in zero_zones),
        f"a zone at zero markers printed no flag: {zero_zones!r}. Silence over an "
        f"unexamined zone is what a green build then certifies",
    )

    case(
        "the mirror rule states its coverage either way",
        ("mirror rule:" in report)
        and ("NO COVERAGE" in report or re.search(r"mirror rule:\s*\d+", report)),
        "the mirror rule reported neither a count nor its own emptiness, which is "
        "how it spent the whole chantier passing without examining anything",
    )


def check_font_cdn_detector_fires() -> None:
    """A guard whose tree is already clean has never been shown to work.

    `check_no_font_cdn.py` exists because the remote-font regression happened
    once and was reverted. It passes today for a reason that says nothing about
    the detector: there is nothing to find. So drive it on the shapes the
    regression actually takes, and pair each with a control that must stay
    silent, because a detector that flags everything is worth as little as one
    that flags nothing.
    """
    print("font CDN guard: the detector fires on the shapes it exists to catch")

    dirty = [
        (
            "a Google Fonts stylesheet link",
            '<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Inter" />',
        ),
        ("a gstatic woff2", "@font-face { src: url(https://fonts.gstatic.com/s/inter/v13/x.woff2); }"),
        ("a bare remote font file", "src: url('https://example.com/assets/Inter.woff2?v=3');"),
        ("a Typekit embed", 'href="https://use.typekit.net/abc1234.css"'),
    ]
    for name, sample in dirty:
        case(
            f"flags {name}",
            bool(fontcdn.offending_urls(sample)),
            f"the detector stayed silent on {sample!r}, which is the exact line "
            f"the guard was written to stop",
        )

    clean = [
        ("a bundled @fontsource import", '@import "@fontsource/inter-tight/400.css";'),
        ("a relative font file", "src: url('/fonts/inter.woff2');"),
        ("a plain documentation link", "See https://diataxis.fr for the framework."),
        ("the repository URL", 'href="https://github.com/Apollia-OS/apollia-os"'),
    ]
    for name, sample in clean:
        case(
            f"stays silent on {name}",
            not fontcdn.offending_urls(sample),
            f"the detector flagged {sample!r}. A guard that fails on compliant "
            f"input gets switched off, and then guards nothing",
        )

    scanned = fontcdn.iter_files()
    case(
        "the scan has coverage",
        len(scanned) >= 20,
        f"only {len(scanned)} file(s) matched. A guard pointed at an empty tree "
        f"reports success for the same reason a compliant tree does",
    )

    # Volume is not coverage. The first version of this gate scanned over a
    # thousand files and still missed `ui/overlay.html`, the webview's second
    # entry point, which is exactly the kind of file a pasted <link> lands in.
    # Assert the entry points by name, and pair it with a negative case, or this
    # is one more verifier that only knows how to pass.
    case(
        "every declared entry point is inside the scanned set",
        not fontcdn.uncovered_required(),
        f"outside the scan: {fontcdn.uncovered_required()!r}. A gate that walks "
        f"src/ and skips the HTML entry point guards the half nobody edits",
    )

    narrowed = fontcdn.SCAN_ROOTS
    try:
        fontcdn.SCAN_ROOTS = [Path("docs/site/src")]
        case(
            "negative control: narrowing the roots is reported, not tolerated",
            bool(fontcdn.uncovered_required()),
            "the coverage assertion stayed silent while the roots were cut back "
            "to a single subdirectory, so it would never notice the drift it "
            "exists to catch",
        )
    finally:
        fontcdn.SCAN_ROOTS = narrowed

    covers_app = any("apollia-desktop" in str(p) for p in scanned)
    covers_site = any("docs/site" in str(p) for p in scanned)
    case(
        "the scan reaches both the app and the documentation site",
        covers_app and covers_site,
        f"app covered: {covers_app}, site covered: {covers_site}. The promise is "
        f"broken by whichever of the two nobody scanned",
    )


def main() -> int:
    check_builder_sweep()
    check_claims_wired()
    check_zero_coverage_is_reported()
    check_font_cdn_detector_fires()
    if FAILURES:
        print(f"\n{len(FAILURES)} self-test failure(s):\n", file=sys.stderr)
        for f in FAILURES:
            print(f"  {f}\n", file=sys.stderr)
        return 1
    print(
        "\nfour properties hold: neither a comment nor a re-export is a use, "
        "zero coverage says so, and the font guard fires on a dirty tree"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
