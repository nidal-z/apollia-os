#!/usr/bin/env python3
"""Pin the biases the verifiers themselves keep producing.

Three checks were written for this corpus, and all three shipped with a bias on
the same side: the side that reports success.

  - `check_optional_builders.py` counted `/// chain [`X::with_y`] ...` as a call
    site, which hid the opt-in permission engine of `apollia-tools`, the
    original instance of the very class it exists to catch.
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
  4. A guard that reports a red says why. The CLI E2E suite computed the cause
     of every failed assertion and threw it away before writing its artifact,
     which is the same bias one step further on: the reader who cannot resolve
     the red falls back on believing the green. The heavy-guard recorder held
     the same shape one level up: its `linux-test` line answered `exit 1, 79
     bin, 4437 tst` and kept no name, so learning which three tests had fallen
     took a replay of the whole Linux suite under artificial load.
  5. A rule carrying a named exemption reports when the exemption grows. The
     tracker-reference rule of `check_prose.py` excuses exactly one path, and a
     green run alone cannot tell one excused path from five. The exemption is
     therefore driven from both sides, like the detector it belongs to.
  6. A guard reads the same set of files whatever tree it runs in. The same
     bias again, one step further out: `check_no_font_cdn.py` walked the disk,
     so it read 1185 files in a developer's tree and 1059 in an extraction of
     the same commit, both green, and its verdict named neither the tree nor
     the 126 generated files that separated them. Coverage that depends on the
     tree drops silently, in the direction that reports success.
  7. Every guard of the corpus is named by a file that launches it. The same
     bias one step further out again: a guard nobody starts reports nothing,
     and nothing distinguishes it from a guard that passed. Two of this
     corpus were in that state, named by no pre-commit entry, no workflow and
     no recipe.
  8. A textual sweep sees what a lint gracies, and an attribute is read by what
     it binds to. `check_panic_free.py` exists because `clippy::unwrap_used`,
     denied by the workspace and restated by five crates, stayed silent on six
     production `unwrap()` whose `Err` type was `Infallible`. Its own way of
     going wrong is the mirror image: a `#[cfg(test)]` matched by proximity
     instead of by what it binds to drops production files from the sweep, and
     the count that serves as a control does not move while it happens.
  9. A detector counts what can act, not what is written about it. The design
     token guard was promoted from a sweep that read whole lines, so
     `hsl(28 11% 13%)` documented inside a `<code>` span of the design showcase
     was reported as a hardcoded colour. That is the bias of the first three
     turned outward: a verifier that inflates its own catch is as unreadable as
     one that hides it, and the guard that cries wolf is the one that ends up
     behind an exclusion. Both directions are asserted, because a detector that
     stopped reading attributes would satisfy the prose case and see nothing.

 10. A ratchet only descends, and it says so in both directions. The same
     exemption bias as case 5, in the form a per-file allowance takes: a file
     above its number fails, and a file *below* its number fails too, until the
     number follows it down. Without the second half, the list outlives the
     debt and the tree carries 168 named allowances nobody can retire.

 12. The boundary that runs the corpus answers the same three codes the corpus
     answers. The distinction each guard pays for, 1 for a defect found and 2
     for nothing measured, is worth nothing if the recipe that launches sixty
     of them collapses it on the way out. `just guards` named its skips and
     exited 0 over them, so a tree where a guard had stopped measuring was
     handed back as green, which is case 2 one step further out: the reader
     who cannot resolve the missing measure falls back on believing the green.

 11. A script reads its arguments before acting on them. `make_scan.py --help`
     regenerated a tracked file and `check_prose.py --help` rendered a verdict,
     because both executed their measure whatever the arguments said. Every
     tracked script must answer `--help` with its usage, exit 0, and leave the
     tree exactly as it found it.

Each case asserts both directions. A check that always answered "not wired", or
that printed a coverage table of zeros, would satisfy the negative half while
being worthless, so every negative case is paired with a positive control.

Usage:
    python3 scripts/check_selftest.py
"""

import argparse
import io
import json
import os
import re
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(Path(__file__).resolve().parent))

import check_claims  # noqa: E402
import check_no_font_cdn as fontcdn  # noqa: E402
import check_panic_free as panicfree  # noqa: E402
import check_optional_builders as builders  # noqa: E402
import check_design_tokens as designtokens  # noqa: E402
import check_prose  # noqa: E402
import binary_freshness  # noqa: E402
import worktree_verdicts  # noqa: E402

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
        "a doc-comment was counted as a call site, which is how the opt-in "
        "permission engine of `apollia-tools` passed as wired",
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
        capture_output=True,
        text=True,
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
        line
        for line in report.splitlines()
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
        (
            "a gstatic woff2",
            "@font-face { src: url(https://fonts.gstatic.com/s/inter/v13/x.woff2); }",
        ),
        (
            "a bare remote font file",
            "src: url('https://example.com/assets/Inter.woff2?v=3');",
        ),
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


# ── The font guard reads the same set in every tree ──────────────────────────
# Same family, one step out from the block above. There the question was
# whether the detector fires; here it is whether the detector was pointed at
# the same thing twice. `iter_files()` walks the disk and never consults git,
# so any generated file carrying a scanned suffix joined the scan in the tree
# that had built it and left it in the tree that had not. Measured on
# `d20a956e`: 1185 files against 1059 in an extraction of the same commit, both
# exit 0, and the verdict was one bare number either way.
#
# The tracked inventory is the reference, and `git ls-files` is exactly what an
# extraction of HEAD lays down: this repository declares no `.gitattributes`,
# so no path is dropped by `export-ignore`. The guard itself still never calls
# git, because it has to run inside such an extraction, which is not a
# repository. Comparing against the inventory is this file's job, not its.


def _in_scope(rel: str) -> bool:
    path = Path(rel)
    if path in fontcdn.SCAN_FILES:
        return True
    under_root = any(
        root == path or root in path.parents for root in fontcdn.SCAN_ROOTS
    )
    return under_root and path.suffix.lower() in fontcdn.SCAN_SUFFIXES


def _check_ignore(paths: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", "check-ignore", "--stdin", "--no-index"],
        cwd=fontcdn.REPO_ROOT,
        input="\n".join(paths),
        capture_output=True,
        text=True,
        check=False,
    )


def check_font_cdn_scan_is_tree_invariant() -> None:
    print("font CDN guard: the scan reads the same set whatever tree it runs in")

    generated = [
        ("the generated API pages", "docs/site/docs/reference/api/oria.api.mdx"),
        ("the generated Figma map", "crates/apollia-desktop/ui/figma/MAPPING.md"),
    ]
    for name, rel in generated:
        case(
            f"skips {name}",
            fontcdn.is_excluded(Path(rel)),
            f"{rel} stayed in the scan. Git ignores it, so it exists in the tree "
            f"that generated it and nowhere else, and the guard's coverage "
            f"follows it instead of the commit",
        )

    # Anchored, not a bare name. `api` or `figma` as directory names would take
    # hand-written neighbours with them, and a guard that quietly stops reading
    # a source file is the defect above with its sign flipped.
    kept = [
        ("the hand-written reference index", "docs/site/docs/reference/index.md"),
        (
            "a tracked file beside the generated map",
            "crates/apollia-desktop/ui/figma/README.md",
        ),
    ]
    for name, rel in kept:
        case(
            f"negative control: still reads {name}",
            not fontcdn.is_excluded(Path(rel)),
            f"{rel} was excluded. The exclusion matched a bare directory name "
            f"instead of the anchored path, and took tracked sources with it",
        )

    inventory = subprocess.run(
        ["git", "ls-files"],
        cwd=fontcdn.REPO_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    case(
        "the tracked inventory is readable",
        inventory.returncode == 0,
        f"`git ls-files` exited {inventory.returncode}: {inventory.stderr.strip()!r}. "
        f"Nothing was measured, which is not the same as a scan that matches",
    )
    if inventory.returncode != 0:
        return
    tracked = set(inventory.stdout.splitlines())
    scanned = {str(p.relative_to(fontcdn.REPO_ROOT)) for p in fontcdn.iter_files()}

    extra = sorted(scanned - tracked)
    ignored: list[str] = []
    if extra:
        query = _check_ignore(extra)
        # 0 means at least one path is ignored, 1 means none is. Anything else
        # is a failure to measure, and must never read as "none is".
        case(
            "the ignore query ran",
            query.returncode in (0, 1),
            f"`git check-ignore` exited {query.returncode}: {query.stderr.strip()!r}",
        )
        if query.returncode not in (0, 1):
            return
        ignored = sorted(set(query.stdout.splitlines()))

    case(
        "no generated file is inside the scan",
        not ignored,
        f"the scan reads {len(ignored)} git-ignored file(s), among them "
        f"{ignored[:3]!r}. Those exist only in the tree that generated them, so "
        f"this verdict does not cover them anywhere else",
    )

    # Positive control on the same query, and it needs no file to exist:
    # `git check-ignore` answers on paths, not on inodes. Without it, a green
    # above would prove this tree carries no generated output, not that the
    # query can see any.
    probe = _check_ignore(
        [
            "docs/site/docs/reference/api/probe.mdx",
            "crates/apollia-desktop/ui/figma/MAPPING.md",
        ]
    )
    case(
        "positive control: the ignore query finds the two generated entries",
        len(probe.stdout.splitlines()) == 2,
        f"the query returned {probe.stdout.splitlines()!r} for two paths git is "
        f"known to ignore, so the case above would be green because the query is "
        f"blind, not because the scan is clean",
    )

    # A source being written is untracked and not ignored. It is read here and
    # not in a clone, which is the direction that adds scrutiny and lasts until
    # the next commit, so it is reported rather than failed.
    pending = [rel for rel in extra if rel not in set(ignored)]
    if pending:
        print(
            f"  note  {len(pending)} untracked source(s) read in this tree only: {pending[:3]}"
        )

    hidden = sorted(
        rel for rel in tracked if _in_scope(rel) and fontcdn.is_excluded(Path(rel))
    )
    case(
        "no tracked source is hidden by an exclusion",
        not hidden,
        f"{len(hidden)} tracked in-scope file(s) are excluded, among them "
        f"{hidden[:3]!r}. An exclusion written for generated output has started "
        f"eating the sources the promise rests on",
    )

    widened = fontcdn.EXCLUDED_PATHS
    try:
        fontcdn.EXCLUDED_PATHS = widened | {Path("docs/site/docs/reference")}
        regrown = [
            rel for rel in tracked if _in_scope(rel) and fontcdn.is_excluded(Path(rel))
        ]
        case(
            "positive control: an exclusion that swallows tracked sources is seen",
            bool(regrown),
            "widening the exclusion to a directory full of tracked pages left the "
            "assertion silent, so it would never notice the drift it exists to catch",
        )
    finally:
        fontcdn.EXCLUDED_PATHS = widened


# ── The prose tracker-reference rule ─────────────────────────────────
# Fifth instance of the same family, and the tree it guards was made clean by
# the very change that added it, so a green run proves the sweep walked 2456
# files and nothing about the detector. The rule is therefore driven on a
# temporary tree: `scan(root, files)` takes both, so no repository state, no
# `git init` and no network are involved.
#
# Fixture discipline is not optional here. `check_prose.py` inventories this
# file, so a fixture spelled in clear text makes the guard fail on its own
# self-test. Every fixture below is composed from fragments for the same reason
# the comment at the end of this file gives for the personal-path pattern.

TRACKED_SAMPLE = "see " + "CAP" + "-149 before touching the boundary"
CLEAN_SAMPLE = "see the capture on the boundary before touching it"
DIRTY_NAME = "tests/integration/test_" + "sprint" + "_28_config.rs"
CLEAN_NAME = "tests/integration/test_system_config_and_routing.rs"
TWIN_MANIFEST = "crates/apollia-desktop/ui/figma/manifest.json"

# One pair per widened form of the tracker rule: the shape it must flag, and
# the nearest compliant neighbour it must stay silent on. The negatives are
# the measured bounds of the rule, not inventions: the CLI suite's two-level
# section labels, an abbreviation ending in a matchable letter, a Rust test
# name, an English word that embeds the vocabulary, and the one concatenated
# identifier the rule defers (see the TODO beside the pattern).
LETTERED_ID_SAMPLE = "the audit is described at " + "F" + ".76 of the plan"
LETTERED_ID_CLEAN = "the audit exports its findings as PDF.76 revisions"
DEEP_ID_SAMPLE = "the shortcut ships under " + "A" + ".1.13 of the plan"
DEEP_ID_CLEAN = "the suite runs section A.14 before the runtime tracks"
STORY_NUM_SAMPLE = "landed by " + "Sto" + "ry 42, kept for the release"
STORY_NUM_CLEAN = "a history 42 pages long, kept for the release"
LOT_NUM_SAMPLE = "connected by " + "Lo" + "t 4 end to end"
LOT_NUM_CLEAN = "slot 4 stays pinned while the model loads"
LOT_CONCAT_DEFERRED = "the set is named " + "lot3" + "_exports for now"


def _prose_tree(tmp: Path, files: dict[str, bytes]) -> list[str]:
    for rel, blob in files.items():
        path = tmp / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(blob)
    return list(files)


def check_prose_tracker_rule() -> None:
    print("prose guard: the tracker rule fires, and its one exemption is bounded")

    with tempfile.TemporaryDirectory() as raw:
        tmp = Path(raw)
        files = _prose_tree(
            tmp,
            {
                "docs/dirty.md": TRACKED_SAMPLE.encode("utf-8"),
                "docs/clean.md": CLEAN_SAMPLE.encode("utf-8"),
                TWIN_MANIFEST: TRACKED_SAMPLE.encode("utf-8"),
                "docs/shot.png": b"\x89PNG\r\n\x1a\n\xff\xfe"
                + TRACKED_SAMPLE.encode("utf-8"),
                DIRTY_NAME: CLEAN_SAMPLE.encode("utf-8"),
                CLEAN_NAME: CLEAN_SAMPLE.encode("utf-8"),
                "docs/lettered.md": LETTERED_ID_SAMPLE.encode("utf-8"),
                "docs/lettered-clean.md": LETTERED_ID_CLEAN.encode("utf-8"),
                "docs/deep.md": DEEP_ID_SAMPLE.encode("utf-8"),
                "docs/deep-clean.md": DEEP_ID_CLEAN.encode("utf-8"),
                "docs/storynum.md": STORY_NUM_SAMPLE.encode("utf-8"),
                "docs/storynum-clean.md": STORY_NUM_CLEAN.encode("utf-8"),
                "docs/lotnum.md": LOT_NUM_SAMPLE.encode("utf-8"),
                "docs/lotnum-clean.md": LOT_NUM_CLEAN.encode("utf-8"),
                "docs/lotconcat.md": LOT_CONCAT_DEFERRED.encode("utf-8"),
            },
        )

        found = check_prose.scan(tmp, files)
        hits = {f.split(":")[0] for f in found}

        case(
            "flags a tracker reference in a file body",
            any(f.startswith("docs/dirty.md:1:") for f in found),
            f"the rule stayed silent on the shape it exists to catch. "
            f"findings: {found!r}",
        )
        case(
            "stays silent on the same sentence without the reference",
            "docs/clean.md" not in hits,
            f"a compliant line was reported. A guard that fails on compliant "
            f"input gets switched off, and then guards nothing. "
            f"findings: {found!r}",
        )
        case(
            "skips a file the decoder rejects",
            "docs/shot.png" not in hits,
            f"a byte match inside an undecodable file was reported. Six such "
            f"files exist under the documentation site, and none of them can "
            f"be corrected. findings: {found!r}",
        )
        case(
            "flags a two-level lettered plan identifier",
            any(f.startswith("docs/lettered.md:1:") for f in found),
            f"the widened rule stayed silent on the lettered identifier "
            f"family. findings: {found!r}",
        )
        case(
            "stays silent on an abbreviation ending in a matchable letter",
            "docs/lettered-clean.md" not in hits,
            f"a compliant abbreviation was reported, so any file mentioning "
            f"PDF revisions would fail the rule. findings: {found!r}",
        )
        case(
            "flags a three-level plan identifier",
            any(f.startswith("docs/deep.md:1:") for f in found),
            f"the three-level identifier family escaped the rule. "
            f"findings: {found!r}",
        )
        case(
            "stays silent on a two-level CLI suite section label",
            "docs/deep-clean.md" not in hits,
            f"a suite section label was reported; the CLI end-to-end suite "
            f"names 28 of them, and none is a plan reference. "
            f"findings: {found!r}",
        )
        case(
            "flags a capitalized numbered story reference",
            any(f.startswith("docs/storynum.md:1:") for f in found),
            f"the numbered story form escaped the rule. findings: {found!r}",
        )
        case(
            "stays silent on an English word embedding the vocabulary",
            "docs/storynum-clean.md" not in hits,
            f"an embedding word was reported, so compliant prose would fail "
            f"the rule. findings: {found!r}",
        )
        case(
            "flags a numbered lot reference",
            any(f.startswith("docs/lotnum.md:1:") for f in found),
            f"the numbered lot form escaped the rule. findings: {found!r}",
        )
        case(
            "stays silent on a word ending in the lot vocabulary",
            "docs/lotnum-clean.md" not in hits,
            f"a compliant embedding word was reported. findings: {found!r}",
        )
        # The deferred bound, asserted so its removal is a decision rather
        # than an accident: the concatenated form is excused until the SDK
        # identifier it collides with is renamed (see the TODO beside the
        # pattern in check_prose.py). When the pattern is widened, this case
        # must flip to a positive in the same change.
        case(
            "the concatenated lot form is still deferred",
            "docs/lotconcat.md" not in hits,
            f"the concatenated form was reported, but the tree still carries "
            f"the SDK identifier it collides with; widen the pattern and this "
            f"case together. findings: {found!r}",
        )
        case(
            "flags the vocabulary carried by a file name",
            any(
                f
                == f"{DIRTY_NAME}: "
                + check_prose.RULES[-1].label
                + ", in the file name"
                for f in found
            ),
            f"the name pass reported nothing, so a file could satisfy the rule "
            f"in its body and carry it in its name. findings: {found!r}",
        )
        case(
            "stays silent on a name that carries none",
            CLEAN_NAME not in hits,
            f"the name pass flagged a compliant path. findings: {found!r}",
        )
        case(
            "the named exemption is honoured",
            TWIN_MANIFEST not in hits,
            f"the one excused path was reported, so the rule cannot be kept "
            f"green while the twin manifest is out of scope. findings: {found!r}",
        )

        # Negative control on the exemption itself, the shape `check_no_font_cdn`
        # uses on its roots. Without it, one excused path grows to five and
        # nothing says so.
        rules = check_prose.RULES
        try:
            check_prose.RULES = rules[:-1] + [rules[-1]._replace(exempt=None)]
            widened = {f.split(":")[0] for f in check_prose.scan(tmp, files)}
            case(
                "negative control: dropping the exemption is reported, not tolerated",
                TWIN_MANIFEST in widened,
                "the excused path stayed silent with its exemption removed, so "
                "the exemption assertion above proves nothing about the list it "
                "is meant to bound",
            )
        finally:
            check_prose.RULES = rules


# ── The worktree verdict comparator ──────────────────────────────────────────
# Same family, fourth instance, and this one was written knowing the family. A
# comparator that only read exit codes would call the two `svelte-check` runs
# equal: 1 in a fresh worktree over 853 files with 2050 fabricated errors, 1 in
# the main tree over 4943 files with the single real one. It would report a
# prepared worktree, and every guard verdict read from it afterwards would be a
# guess. So the negative case is exactly that pair, and it is paired with a
# positive control, because a comparator that always answered "different" would
# satisfy the negative half and be worthless.
#
# No guard is run here. The fixtures are JSON records in a temporary directory,
# which is what `--compare` reads.

MATCHING_MEASURES = {
    "cargo-check": {"exit": 0},
    "cargo-clippy": {"exit": 0},
    "cargo-test": {"exit": 101, "binaries": 77, "tests": 4370, "home_changes": 0},
    "cli-e2e": {"exit": 0, "pass": 154, "fail": 0},
    "ui-build": {"exit": 0},
    "svelte-check": {"exit": 1, "files": 4943, "errors": 1},
    "vitest": {"exit": 0, "tests": 790},
    "docs-build": {"exit": 0},
    "desktop-automation": {"exit": 0, "steps": 2424, "failed": 0},
    "linux-test": {"exit": 0, "binaries": 78, "tests": 4370},
}


def _record(tree: str, overrides: dict[str, dict] | None = None) -> dict:
    """A full record, on one commit, that every guard reports as prepared."""
    guards = {
        key: {"prepared": True, "measures": dict(measures), "seconds": 1.0}
        for key, measures in MATCHING_MEASURES.items()
    }
    for key, entry in (overrides or {}).items():
        guards[key] = entry
    return {
        "tree": tree,
        "head": "6a3f59de06c66c0c7dc6f4222dd381ab2ebc33c5",
        "porcelain_lines": 0,
        "python_bundle": f"{tree}/target/python-bundle/aarch64-apple-darwin/python",
        "guards": guards,
    }


def _compare(root: Path, left: dict, right: dict) -> subprocess.CompletedProcess:
    (root / "main.json").write_text(json.dumps(left), encoding="utf-8")
    (root / "worktree.json").write_text(json.dumps(right), encoding="utf-8")
    return subprocess.run(
        [
            sys.executable,
            str(REPO_ROOT / "scripts" / "worktree_verdicts.py"),
            "--compare",
            str(root / "main.json"),
            str(root / "worktree.json"),
        ],
        capture_output=True,
        text=True,
    )


def check_worktree_comparator() -> None:
    print("worktree verdicts: two exit codes of 1 are not the same verdict")

    declared = [guard.key for guard in worktree_verdicts.GUARDS]
    case(
        "the fixture covers every guard the tool declares",
        set(declared) == set(MATCHING_MEASURES),
        f"the tool declares {sorted(declared)} and the fixture covers "
        f"{sorted(MATCHING_MEASURES)}. A guard absent from both records is "
        f"absent from the comparison, so it would never produce a gap",
    )

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)

        unprepared = _record("/main")
        fresh = _record(
            "/worktree",
            {
                "svelte-check": {
                    "prepared": True,
                    "measures": {"exit": 1, "files": 853, "errors": 2050},
                    "seconds": 1.0,
                }
            },
        )
        run = _compare(root, unprepared, fresh)
        case(
            "same exit code, different FILES and ERRORS, is a gap",
            run.returncode == 1 and "svelte-check" in run.stdout + run.stderr,
            f"exit {run.returncode} on the pair that decided the shape of this "
            f"tool: 1 over 853 files with 2050 errors against 1 over 4943 with "
            f"1. Output:\n{run.stdout}{run.stderr}",
        )

        run = _compare(root, _record("/main"), _record("/worktree"))
        case(
            "positive control: two identical records are conforming",
            run.returncode == 0,
            f"exit {run.returncode} on two records that agree on every measure. "
            f"A comparator that always answers `different` satisfies the case "
            f"above and states nothing. Output:\n{run.stdout}{run.stderr}",
        )

        blind = _record(
            "/worktree",
            {
                "svelte-check": {
                    "prepared": False,
                    "reason": "svelte-check is not installed",
                    "probe": "crates/apollia-desktop/ui/node_modules/.bin/svelte-check",
                }
            },
        )
        run = _compare(root, _record("/main"), blind)
        case(
            "a guard recorded as not prepared is never conforming",
            run.returncode == 1 and "not prepared" in run.stdout + run.stderr,
            f"exit {run.returncode} on a record where one guard could not run. "
            f"Zero coverage reported as a pass is the bias this whole file "
            f"exists to pin. Output:\n{run.stdout}{run.stderr}",
        )

        # The criterion is "on the same commit as the main tree". Nothing else
        # in this tool would enforce it, and a comparison across two commits
        # attributes to the worktree a difference the commit produced.
        other = _record("/worktree")
        other["head"] = "0123456789abcdef0123456789abcdef01234567"
        run = _compare(root, _record("/main"), other)
        case(
            "two records on different commits are refused, not compared",
            run.returncode == 2,
            f"exit {run.returncode} on two records made on different commits. "
            f"Comparing them would credit the worktree with a difference the "
            f"commit produced. Output:\n{run.stdout}{run.stderr}",
        )

        # `cargo test` is the one guard whose exit code is deliberately outside
        # the comparison, because a non-deterministic test would otherwise decide
        # the verdict for a cause foreign to the worktree. Both directions:
        # the exit code alone is not a gap, the test count alone is.
        flaky = _record(
            "/worktree",
            {
                "cargo-test": {
                    "prepared": True,
                    "measures": {
                        "exit": 0,
                        "binaries": 77,
                        "tests": 4370,
                        "home_changes": 0,
                    },
                    "seconds": 1.0,
                }
            },
        )
        run = _compare(root, _record("/main"), flaky)
        case(
            "cargo test: a differing exit code alone is not a gap",
            run.returncode == 0,
            f"exit {run.returncode} while both trees ran 77 binaries and 4370 "
            f"tests and only the exit code differed. That is the flaky test "
            f"taking the criterion hostage, which the framing ruled out. "
            f"Output:\n{run.stdout}{run.stderr}",
        )

        died = _record(
            "/worktree",
            {
                "cargo-test": {
                    "prepared": True,
                    "measures": {
                        "exit": 101,
                        "binaries": 0,
                        "tests": 0,
                        "home_changes": 0,
                    },
                    "seconds": 1.0,
                }
            },
        )
        run = _compare(root, _record("/main"), died)
        case(
            "cargo test: the same exit code over 0 binaries is a gap",
            run.returncode == 1,
            f"exit {run.returncode} while one tree ran 4370 tests and the other "
            f"ran none, both exiting 101. That is exactly the fresh-worktree "
            f"verdict this tool exists to separate from a real run. "
            f"Output:\n{run.stdout}{run.stderr}",
        )

        # The home sentinel is the third compared measure of cargo test: a run
        # that wrote into the sentinel ~/.apollia differs from one that left it
        # alone even when every test summary agrees. The conforming direction
        # is already held by the identical-records control above.
        dirty_home = _record(
            "/worktree",
            {
                "cargo-test": {
                    "prepared": True,
                    "measures": {
                        "exit": 101,
                        "binaries": 77,
                        "tests": 4370,
                        "home_changes": 2,
                    },
                    "seconds": 1.0,
                }
            },
        )
        run = _compare(root, _record("/main"), dirty_home)
        case(
            "cargo test: a dirty home sentinel alone is a gap",
            run.returncode == 1 and "home_changes" in run.stdout + run.stderr,
            f"exit {run.returncode} while one run wrote 2 entries into the "
            f"sentinel home and the other wrote none, with identical test "
            f"summaries. A write into the operator's profile hidden behind a "
            f"green suite is the defect this measure exists to expose. "
            f"Output:\n{run.stdout}{run.stderr}",
        )

        # `desktop-automation` is the one exempt-when-unprepared guard, and the
        # exemption is driven from both sides, like every named exemption in
        # this file: an unprepared line must not decide the comparison, and it
        # must never swallow a real difference once both trees carry a report.
        unrun = _record(
            "/worktree",
            {
                "desktop-automation": {
                    "prepared": False,
                    "reason": "no automation report",
                    "probe": [".apollia-automation/report.json"],
                }
            },
        )
        run = _compare(root, _record("/main"), unrun)
        case(
            "desktop-automation: unprepared is exempt, named, and not a gap",
            run.returncode == 0 and "exempt" in run.stdout + run.stderr,
            f"exit {run.returncode} while the worktree simply has no automation "
            f"report. Preparing that guard is a manual run of the real app, so "
            f"an absent report must be named and skipped, not turned into a "
            f"red. Output:\n{run.stdout}{run.stderr}",
        )

        red_run = _record(
            "/worktree",
            {
                "desktop-automation": {
                    "prepared": True,
                    "measures": {"exit": 1, "steps": 2424, "failed": 10},
                    "seconds": 1.0,
                }
            },
        )
        run = _compare(root, _record("/main"), red_run)
        case(
            "desktop-automation: two fresh reports that disagree are a gap",
            run.returncode == 1
            and "check_automation_report" in run.stdout + run.stderr,
            f"exit {run.returncode} while one tree's corpus ran green and the "
            f"other's failed 10 steps. The exemption covers absence only; once "
            f"both trees measured, a difference is a difference. "
            f"Output:\n{run.stdout}{run.stderr}",
        )

        # `linux-test` is recorded but exempt from the comparison: it needs a
        # Docker daemon, and a machine without one would turn every comparison
        # red for a cause foreign to the two trees. The exemption is driven
        # from both sides: it never decides the verdict, its row still prints,
        # and the list of exempt guards is bounded by name so it can only grow
        # in the light.
        case(
            "the compare exemption is bounded to the docker-dependent guard",
            [g.key for g in worktree_verdicts.GUARDS if g.exempt] == ["linux-test"],
            f"the exempt guards are "
            f"{[g.key for g in worktree_verdicts.GUARDS if g.exempt]}. A second "
            f"exemption entering this list unnamed would be a guard leaving "
            f"the comparison in the dark",
        )

        no_daemon = _record(
            "/worktree",
            {
                "linux-test": {
                    "prepared": True,
                    "measures": {"exit": 2, "binaries": None, "tests": None},
                    "seconds": 1.0,
                }
            },
        )
        run = _compare(root, _record("/main"), no_daemon)
        case(
            "linux-test: a run that measured nothing is not a parity gap",
            run.returncode == 0
            and "linux-check.sh" in run.stdout
            and "recorded outside the comparison" in run.stdout,
            f"exit {run.returncode} while one tree measured its Linux suites "
            f"and the other had no Docker daemon (exit 2, nothing measured). "
            f"The exempt row must still print so the absence stays visible. "
            f"Output:\n{run.stdout}{run.stderr}",
        )


# ── The recorder that says which tests fell ─────────────────────────────
# Case 4 again, on the heavy-guard recorder rather than on the CLI artifact.
# The `linux-test` line answered `exit 1, 79 bin, 4437 tst` inside a recording
# on 2026-08-27 and kept no name, so learning which three tests had fallen took
# a replay of the whole Linux suite under artificial load.
#
# The two fixtures below are the shape of a real run, captured on 2026-08-28
# from a throwaway crate built to be red: two failing unit tests and one
# failing doc-test. They carry the three shapes the extraction has to tell
# apart, an ignored test ending `... ignored, <reason>`, a doc-test name
# holding spaces, and the summary line `test result: FAILED.` that names no
# test at all.

RED_CARGO = """running 4 tests
test tests::skipped ... ignored, never runs
test tests::it_passes ... ok
test tests::result_tests_shaped_like_the_trap ... FAILED
test tests::it_fails ... FAILED

failures:

---- tests::it_fails stdout ----
thread 'tests::it_fails' panicked at src/lib.rs:19:9:
assertion `left == right` failed: deliberate

failures:
    tests::it_fails
    tests::result_tests_shaped_like_the_trap

test result: FAILED. 1 passed; 2 failed; 1 ignored; 0 measured; 0 filtered out

running 1 test
test src/lib.rs - add (line 3) ... FAILED

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
"""

# The green fixture carries the HOME-SENTINEL line the wrapper appends around
# the real `cargo test`, so the run it stands for is fully measured and the
# control below can assert that a green suite reads as a green suite rather
# than as a guard that measured nothing.
GREEN_CARGO = """running 1 test
test tests::it_passes ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out
HOME-SENTINEL: CLEAN
"""

RED_E2E = """  PASS  : 151
  FAIL  : 3
  SKIP  : 12

Failed assertions:
  - track1 :: agent list --json :: exit code
  - track1 :: config show :: missing field
"""

GREEN_E2E = """  PASS  : 154
  FAIL  : 0
  SKIP  : 12
All assertions passed.
"""


def check_red_tests_are_named() -> None:
    print("worktree verdicts: a red names the tests that made it red")

    guards = {guard.key: guard for guard in worktree_verdicts.GUARDS}
    red = worktree_verdicts.measure_cargo_test(RED_CARGO, 101)
    case(
        "a red run records the name of every failing test",
        red["failed_tests"]
        == [
            "tests::result_tests_shaped_like_the_trap",
            "tests::it_fails",
            "src/lib.rs - add (line 3)",
        ]
        and red["failed"] == 3,
        f"measured {red!r}. The recorder kept the counts and threw the names "
        f"away, which is the difference between a guard that says red and a "
        f"guard that says what fell",
    )

    green = worktree_verdicts.measure_cargo_test(GREEN_CARGO, 0)
    case(
        "positive control: a green run invents no name and is not `not measured`",
        green["failed_tests"] == []
        and green["failed"] == 0
        and not worktree_verdicts.unmeasured({"measures": green}),
        f"measured {green!r}. An extraction that answered None on a green "
        f"would satisfy the case above and read every green run as a guard "
        f"that measured nothing",
    )

    nothing = worktree_verdicts.measure_cargo_test("", 101)
    entry = {"prepared": True, "measures": nothing}
    case(
        "a run that measured nothing names nothing, and says so",
        nothing["failed_tests"] is None
        and nothing["failed"] is None
        and worktree_verdicts.cell(entry, guards["cargo-test"]) == "not measured",
        f"measured {nothing!r}, cell "
        f"{worktree_verdicts.cell(entry, guards['cargo-test'])!r}. An empty "
        f"list on a run that never reported would claim a suite ran clean when "
        f"no suite ran at all",
    )

    linux = worktree_verdicts.measure_linux_test(RED_CARGO, 1)
    case(
        "the Linux mirror names its red too, and its counts are unchanged",
        linux["failed_tests"] == red["failed_tests"]
        and (linux["binaries"], linux["tests"]) == (2, 4),
        f"measured {linux!r}. The container prints the same lines and "
        f"linux-check.sh tees them, so a Linux red is readable without the "
        f"replay it cost on 2026-08-27",
    )

    e2e_red = worktree_verdicts.measure_cli_e2e(RED_E2E, 1)
    e2e_green = worktree_verdicts.measure_cli_e2e(GREEN_E2E, 0)
    e2e_blind = worktree_verdicts.measure_cli_e2e("built nothing", 2)
    case(
        "the CLI suite names its failed assertions, and only when it ran",
        e2e_red["failed_cases"]
        == [
            "track1 :: agent list --json :: exit code",
            "track1 :: config show :: missing field",
        ]
        and e2e_green["failed_cases"] == []
        and e2e_blind["failed_cases"] is None,
        f"red {e2e_red!r}, green {e2e_green!r}, blind {e2e_blind!r}. The three "
        f"states are the same three the counters already carry, and the names "
        f"must follow them rather than invent a fourth",
    )

    many = "\n".join(f"test crate::mod::t{i:02d} ... FAILED" for i in range(40))
    many += "\ntest result: FAILED. 0 passed; 40 failed; 0 ignored; 0 measured\n"
    capped = worktree_verdicts.measure_cargo_test(many, 101)
    lines = worktree_verdicts.red_lines(guards["cargo-test"], capped)
    case(
        "the list is capped, and the cap says what it hid",
        len(capped["failed_tests"]) == worktree_verdicts.FAILED_CAP
        and capped["failed"] == 40
        and lines[-1].endswith(f"more not listed (cap {worktree_verdicts.FAILED_CAP})"),
        f"kept {len(capped['failed_tests'])} of {capped['failed']}, last line "
        f"{lines[-1]!r}. A silent truncation is a second thing thrown away, "
        f"and an uncapped list is a record nobody reads",
    )

    named = [(guard.key, guard.reds) for guard in worktree_verdicts.GUARDS if guard.reds]
    compared = [
        (guard.key, key)
        for guard in worktree_verdicts.GUARDS
        if guard.reds
        for key in guard.reds
        if key in guard.compared
    ]
    case(
        "the three suites name their red, and no name decides the comparison",
        [key for key, _ in named] == ["cargo-test", "cli-e2e", "linux-test"]
        and compared == [("cli-e2e", "fail")],
        f"guards that name a red {named!r}, name keys inside the comparison "
        f"{compared!r}. Comparing which tests fell would hand the criterion to "
        f"the flaky test the framing took it away from; `fail` is a count, and "
        f"it was already compared before the names existed",
    )


# ── The desktop automation report reader ─────────────────────────────────────
# Same family as the comparator: `.apollia-automation/report.json` carried
# ok=False for weeks while every chain stayed green, because no chain read it.
# The reader that now exists must be held to the family's properties: it fires
# on a red report, a report that predates HEAD is "nothing measured" and never
# a pass (a green verdict about an older tree is the freshest form of the
# success bias), and the green direction is a positive control so a reader
# that always answers red cannot satisfy the negative half.


def _automation_report(ok: bool, steps: list[dict], finished: str) -> dict:
    return {
        "script": "fixture",
        "startedAt": finished,
        "finishedAt": finished,
        "ok": ok,
        "steps": steps,
        "captures": {},
        "screenshots": [],
    }


def check_automation_report_reader() -> None:
    print("automation report: the runtime verdict is read, and stale is not a pass")

    reader = REPO_ROOT / "scripts" / "check_automation_report.py"
    fresh = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.000Z")

    def run_reader(report: Path, script: Path) -> subprocess.CompletedProcess:
        return subprocess.run(
            [sys.executable, str(reader), str(report), str(script)],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
        )

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        script = root / "script.json"
        script.write_text(
            json.dumps(
                {
                    "name": "fixture",
                    "steps": [
                        {"kind": "screenshot", "label": "section-01-fixture"},
                        {"kind": "waitFor", "testid": "anchor"},
                    ],
                }
            ),
            encoding="utf-8",
        )

        red = root / "red.json"
        red.write_text(
            json.dumps(
                _automation_report(
                    False,
                    [
                        {"index": 0, "kind": "screenshot", "ok": True},
                        {
                            "index": 1,
                            "kind": "waitFor",
                            "ok": False,
                            "detail": "timeout",
                        },
                    ],
                    fresh,
                )
            ),
            encoding="utf-8",
        )
        run = run_reader(red, script)
        case(
            "a fresh red report exits 1 and names its red section",
            run.returncode == 1 and "section-01-fixture" in run.stdout,
            f"exit {run.returncode} on a fresh report with one failed step. "
            f"Output:\n{run.stdout}{run.stderr}",
        )

        green = root / "green.json"
        green.write_text(
            json.dumps(
                _automation_report(
                    True,
                    [
                        {"index": 0, "kind": "screenshot", "ok": True},
                        {"index": 1, "kind": "waitFor", "ok": True},
                    ],
                    fresh,
                )
            ),
            encoding="utf-8",
        )
        run = run_reader(green, script)
        case(
            "positive control: a fresh green report exits 0",
            run.returncode == 0,
            f"exit {run.returncode} on a fresh all-green report. A reader that "
            f"always answers red satisfies the case above and states nothing. "
            f"Output:\n{run.stdout}{run.stderr}",
        )

        run = run_reader(root / "absent.json", script)
        case(
            "an absent report is nothing measured (exit 2), not a pass",
            run.returncode == 2,
            f"exit {run.returncode} on a missing report file. "
            f"Output:\n{run.stdout}{run.stderr}",
        )

        stale = root / "stale.json"
        stale.write_text(
            json.dumps(
                _automation_report(
                    True,
                    [{"index": 0, "kind": "screenshot", "ok": True}],
                    "2000-01-01T00:00:00.000Z",
                )
            ),
            encoding="utf-8",
        )
        run = run_reader(stale, script)
        case(
            "a green report older than HEAD is nothing measured (exit 2)",
            run.returncode == 2,
            f"exit {run.returncode} on a green report that predates the HEAD "
            f"commit. It measured an older tree, and reading it as a pass is "
            f"the success bias with a timestamp. Output:\n{run.stdout}{run.stderr}",
        )


# ── The CLI E2E failure detail ───────────────────────────────────────────────
# Fifth instance, and the same bias read from the other end. Here the artifact
# does not claim a success it has not earned; it reports a failure it cannot
# explain, which a reader resolves the same way, by believing the green part.
#
# Six assertion sites in tests/cli/lib/assert.sh build a detail carrying the
# expectation, the command and the head of the observed output. `_fail` printed
# it under a verbose flag no automatic caller sets, and never passed it to
# `_report_row`, so an uploaded report.json named a failing label with no cause.
# Three more sites recorded a failure without producing a row at all, and a run
# rendered a summary announcing one failure over an empty assertion list.
#
# The property is pinned on the libraries alone. `check_selftest.py` runs in the
# `prose-guard` job, a bare checkout with no cargo build, and `cli-e2e.sh` exits
# 2 before its first assertion when it finds no binary, so a case that drove the
# real suite could never be green there. Recording inside `_record_fail` rather
# than beside each of its callers is what makes a library-level pilot sufficient.
#
# The two redaction cases are not decoration. The report directory is git-ignored
# so the prose guard never scans it, and CI uploads it from what will be a public
# repository. The adversarial one pins the interaction that the borne alone would
# miss: truncation happens where the detail is built, redaction where it is
# recorded, so a cut landing inside a root leaves a fragment no substitution
# matches any more, and the fragment is a personal-path pattern in its own right.

SELFTEST_HOME = "/Users/selftest-operator"
SELFTEST_REPO = SELFTEST_HOME + "/dev/apollia-selftest"

# Padding + a root, cut at the 300 characters check() keeps, leaves the first 12
# characters of the root behind. On this tree that length is exactly the personal
# filesystem path that check_prose.py refuses, which is why the cut is what makes
# the fragment dangerous rather than merely untidy. The pattern is not spelled out
# here for the same reason check_prose.py spells its own with a bracket class: a
# file that quotes it fails the rule it is describing, as this comment did on its
# first draft.
PILOT = r"""
set -uo pipefail

LIB_DIR="__LIB_DIR__"
RUN_TMP="__RUN_TMP__"
REPO_ROOT="__REPO_ROOT__"
REAL_HOME="__REAL_HOME__"

# shellcheck source=/dev/null
source "$LIB_DIR/assert.sh"
# shellcheck source=/dev/null
source "$LIB_DIR/report.sh"

PASS=0; FAIL=0; SKIP=0; FAILED_LABELS=()
CURRENT_TRACK="selftest"
report_init "$RUN_TMP"

check "green case" /usr/bin/true
check "red case" /bin/bash -c 'printf "%s\n" "needle-on-output"; exit 3'

_record_fail "outside any assertion" \
    "run=$RUN_TMP/scratch1 bin=$REPO_ROOT/target/debug/apollia-os home=$REAL_HOME/.apollia"

PAD=$(printf '%288s' '' | /usr/bin/tr ' ' 'x')
check "cut lands inside a root" \
    /bin/bash -c 'printf "%s%s" "$1" "$2"; exit 4' _ "$PAD" "$REAL_HOME"

report_finalize "$RUN_TMP/report.json" "$RUN_TMP/report.md" "$PASS" "$FAIL" "$SKIP" 0 \
    || { echo "PILOT: report_finalize failed" >&2; exit 9; }
exit 0
"""


def _run_pilot(run_tmp: Path) -> tuple[dict, subprocess.CompletedProcess]:
    script = (
        PILOT.replace("__LIB_DIR__", str(REPO_ROOT / "tests" / "cli" / "lib"))
        .replace("__RUN_TMP__", str(run_tmp))
        .replace("__REPO_ROOT__", SELFTEST_REPO)
        .replace("__REAL_HOME__", SELFTEST_HOME)
    )
    path = run_tmp / "pilot.sh"
    path.write_text(script, encoding="utf-8")
    run = subprocess.run(["bash", str(path)], capture_output=True, text=True)
    out = run_tmp / "report.json"
    payload = json.loads(out.read_text(encoding="utf-8")) if out.exists() else {}
    return payload, run


def check_e2e_failure_detail() -> None:
    print("CLI E2E report: a failure carries its cause, a pass carries none")

    with tempfile.TemporaryDirectory() as tmp:
        run_tmp = Path(tmp)
        payload, run = _run_pilot(run_tmp)
        rows = {r["label"]: r for r in payload.get("assertions", [])}
        raw = json.dumps(payload, ensure_ascii=False)
        context = (
            f"pilot exit {run.returncode}, rows {sorted(rows)}.\n"
            f"stdout:\n{run.stdout}\nstderr:\n{run.stderr}"
        )

        red = rows.get("red case", {})
        detail = red.get("detail", "")
        case(
            "a failing assertion records the command and the output",
            red.get("verdict") == "FAIL"
            and "needle-on-output" in detail
            and "/bin/bash" in detail,
            f"detail {detail!r}. Six sites build this string and one of them "
            f"stopped handing it over, which is the whole regression: a report "
            f"that names a failing label and no cause. {context}",
        )

        green = rows.get("green case", {})
        case(
            "a passing assertion records no detail at all",
            green.get("verdict") == "PASS" and "detail" not in green,
            f"green row {green!r}. A fix that writes the detail on every row "
            f"satisfies the case above and drowns the artifact, so the absence "
            f"is asserted, not the emptiness. {context}",
        )

        outside = rows.get("outside any assertion", {})
        case(
            "a failure recorded outside any assertion still produces a row",
            outside.get("verdict") == "FAIL" and outside.get("detail"),
            f"row {outside!r}. Three callers in cli-e2e.sh record a failure "
            f"without going through an assertion, and a run of theirs rendered "
            f"a summary of one failure over an empty list. {context}",
        )

        roots = (str(run_tmp), SELFTEST_REPO, SELFTEST_HOME)
        case(
            "the three roots are replaced, in the order that makes it work",
            not any(root in raw for root in roots)
            and "$REPO" in outside.get("detail", ""),
            f"detail {outside.get('detail')!r}. The report directory is "
            f"git-ignored, so the prose guard never sees it, and CI uploads it. "
            f"The $REPO token is what proves the order held: the repository "
            f"lives under the real HOME, so substituting HOME first would leave "
            f"the repository pass nothing to match. {context}",
        )

        cut = rows.get("cut lands inside a root", {})
        cut_detail = cut.get("detail", "")
        leaked = [
            SELFTEST_HOME[:n]
            for n in range(4, len(SELFTEST_HOME) + 1)
            if SELFTEST_HOME[:n] in raw
        ]
        case(
            "a cut landing inside a root leaves no fragment behind",
            bool(cut_detail) and not leaked,
            f"fragments still in the report: {leaked!r}, detail {cut_detail!r}. "
            f"The detail is truncated where it is built and redacted where it is "
            f"recorded, so a cut inside a root defeats the substitution and "
            f"manufactures the exact pattern the redaction exists to remove. "
            f"{context}",
        )


# ── The crossing: a guard nobody launches ────────────────────────────────────────────

# The files that declare where a command runs. A guard whose command appears in
# none of their launching lines is declared and launched by nothing, which is
# the same bias as the six above one step further out: the corpus reports green
# because nothing ran, and the reader who cannot tell a guard that passed from
# a guard that was never started falls back on believing the green.
#
# A mention is not a launch. Only the lines that make something run reach the
# text this crossing searches: `entry:` values in the pre-commit config, `run:`
# values in the workflows (block scalars included), recipe bodies in the
# justfile, and the argv of the heavy-guards table of worktree_verdicts.py.
# The previous crossing searched whole files, so a guard named only by a
# comment satisfied it; three guards of this corpus were in exactly that state
# inside the pre-commit config while a fourth boundary really launched them,
# which made the verdict right by accident.
BOUNDARY_FILES = (".pre-commit-config.yaml", "justfile")
WORKFLOW_DIR = ".github/workflows"

# Guards that are not `scripts/check_*.py` files but belong to the same corpus:
# each maps to the pattern a launching line must carry, searched per line.
# Several of these existed only as CI jobs while the CI was not running, which
# left them green by absence on every machine.
EXTERNAL_GUARDS = {
    "cargo clippy --workspace": r"\bcargo clippy --workspace\b",
    "cargo machete": r"\bcargo machete\b",
    "cargo audit": r"\bcargo audit\b",
    "cargo deny check": r"\bcargo deny check\b",
    "mypy apollia": r"\bmypy apollia\b",
    "pytest": r"(?:^|&&|;)\s*pytest\b",
    "npm run audit:i18n": r"\bnpm run audit:i18n\b",
    "npm run audit:a11y": r"\bnpm run audit:a11y\b",
    "npx svelte-check": r"\bnpx svelte-check\b",
    "npm run build": r"\bnpm run build\b",
    "automation validate.py": r"\bscripts/automation/tools/validate\.py\b",
    "cli-e2e.sh": r"\btests/cli/cli-e2e\.sh\b",
    "linux-check.sh": r"\bscripts/linux-check\.sh\b",
    "worktree_verdicts.py": r"\bscripts/worktree_verdicts\.py\b",
    "ruff check scripts": r"\bruff check\b.*\bscripts\b",
}

# Externals the corpus carries without a boundary yet, waived one by one so the
# list can only shrink: an entry whose pattern shows up in a launching line is
# stale and fails the crossing, and the entry leaves with the condition that
# justifies it. An unnamed waiver would be the ratchet growing in the dark.
EXTERNAL_GUARDS_AWAITING_BOUNDARY: dict[str, str] = {}

_YAML_LAUNCH_KEY = re.compile(r"^(\s*)(?:-\s+)?(?:run|entry):\s*(.*?)\s*$")


def _yaml_launch_lines(text: str) -> list[str]:
    """Values of `run:` and `entry:` keys, block scalars included."""
    out: list[str] = []
    lines = text.splitlines()
    i = 0
    while i < len(lines):
        m = _YAML_LAUNCH_KEY.match(lines[i])
        if m is None:
            i += 1
            continue
        indent, value = len(m.group(1)), m.group(2)
        i += 1
        if value and not value.startswith(("|", ">")):
            out.append(value)
            continue
        while i < len(lines):
            line = lines[i]
            if line.strip() and len(line) - len(line.lstrip()) <= indent:
                break
            out.append(line)
            i += 1
    return out


def _justfile_launch_lines(text: str) -> list[str]:
    """Recipe body lines: indented, non-empty, not shell comments."""
    return [
        line
        for line in text.splitlines()
        if line[:1] in (" ", "\t")
        and line.strip()
        and not line.lstrip().startswith("#")
    ]


def _launching_only(text: str) -> str:
    """Drop the lines that cannot start a command: comments and blanks."""
    return "\n".join(
        line
        for line in text.splitlines()
        if line.strip() and not line.lstrip().startswith(("#", "//"))
    )


def _boundary_text() -> tuple[str, list[str]]:
    read: list[str] = []
    lines: list[str] = []
    pre_commit = REPO_ROOT / ".pre-commit-config.yaml"
    if pre_commit.is_file():
        lines += _yaml_launch_lines(
            pre_commit.read_text(encoding="utf-8", errors="replace")
        )
        read.append(".pre-commit-config.yaml")
    justfile = REPO_ROOT / "justfile"
    if justfile.is_file():
        lines += _justfile_launch_lines(
            justfile.read_text(encoding="utf-8", errors="replace")
        )
        read.append("justfile")
    workflows = sorted((REPO_ROOT / WORKFLOW_DIR).glob("*.yml"))
    workflows += sorted((REPO_ROOT / WORKFLOW_DIR).glob("*.yaml"))
    for path in workflows:
        if not path.is_file():
            continue
        lines += _yaml_launch_lines(path.read_text(encoding="utf-8", errors="replace"))
        read.append(str(path.relative_to(REPO_ROOT)))
    # The heavy guards run through `just worktree-verdicts`, whose commands
    # live in the GUARDS table rather than in a recipe body.
    lines += [" ".join(guard.command) for guard in worktree_verdicts.GUARDS]
    read.append("scripts/worktree_verdicts.py:GUARDS")
    return "\n".join(lines), read


def orphan_guards(basenames: list[str], text: str) -> list[str]:
    """Guard basenames that no launching line carries.

    Comment lines are dropped before the search: a `#` line naming a guard is
    a mention, and a mention is not a launch. The previous version searched
    the raw text, which is how a name cited in a comment passed for a boundary.
    """
    launching = _launching_only(text)
    return sorted(name for name in basenames if name not in launching)


def launched_externals(patterns: dict[str, str], text: str) -> dict[str, bool]:
    """For each external guard, whether one launching line matches its pattern."""
    lines = _launching_only(text).splitlines()
    return {
        name: any(re.search(pattern, line) for line in lines)
        for name, pattern in patterns.items()
    }


def check_guards_are_launched() -> None:
    print("guard corpus: every tracked guard is launched by a boundary file")

    inventory = subprocess.run(
        ["git", "ls-files", "scripts/check_*.py"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    case(
        "the guard inventory is readable",
        inventory.returncode == 0,
        f"`git ls-files` exited {inventory.returncode}: {inventory.stderr.strip()!r}. "
        f"Nothing was measured, which is not the same as no orphan",
    )
    if inventory.returncode != 0:
        return

    tracked = [Path(line).name for line in inventory.stdout.split()]
    # Untracked guards are out of reach on purpose: they are absent from a fresh
    # clone, so demanding a boundary for one would ask every clone to launch a
    # file it does not have. The day such a guard becomes tracked, it enters
    # this crossing without a line being added here.
    case(
        "the crossing has a corpus to measure",
        len(tracked) >= 2,
        f"`git ls-files scripts/check_*.py` returned {tracked!r}. A crossing "
        f"over an empty corpus reports no orphan and proves nothing",
    )

    text, read = _boundary_text()
    case(
        "the boundary files were read",
        len(read) >= 4 and ".pre-commit-config.yaml" in read and "justfile" in read,
        f"read {read!r}. A crossing over an empty text would report every guard "
        f"as an orphan, and one over a partial text would invent orphans",
    )

    orphans = orphan_guards(tracked, text)
    case(
        "no tracked guard is orphaned of a boundary",
        not orphans,
        f"{len(orphans)} guard(s) launched by no boundary file: {orphans!r}. Each "
        f"is a rule the corpus carries and nothing enforces. Add it to the "
        f"`guards` recipe of the justfile, or to a pre-commit entry",
    )

    # Positive control, on the same query and against the same text: without it
    # a green above would prove this tree carries no orphan only if the
    # crossing can see one at all, which is exactly what a bare `in` on a
    # truncated text would fail to do.
    invented = "check_a_guard_no_boundary_names.py"
    case(
        "positive control: a name no boundary file carries is reported",
        orphan_guards([invented], text) == [invented],
        f"the crossing found {invented!r} inside the boundary text, so the case "
        f"above would be green because the crossing matches anything, not "
        f"because the corpus is launched",
    )

    # Both directions of the mention rule, on fixtures rather than on the
    # tree: the defect this crossing had was accepting a comment as a launch,
    # so its correction is pinned from the failing side and from the passing
    # side at once.
    case(
        "a guard named only by a comment is an orphan",
        orphan_guards(["check_x.py"], "# check_x.py is not launched") == ["check_x.py"],
        "a comment satisfied the crossing, which is the exact bias this file "
        "exists to catch: a mention is not a launch",
    )
    case(
        "a guard named by a launching line is not an orphan",
        orphan_guards(["check_x.py"], "python3 scripts/check_x.py") == [],
        "a real launching line was reported as an orphan, so the crossing "
        "would demand boundaries no file can provide",
    )

    # External guards: the rules of this corpus that are not check_*.py files.
    launched = launched_externals(EXTERNAL_GUARDS, text)
    missing = sorted(
        name
        for name, ok in launched.items()
        if not ok and name not in EXTERNAL_GUARDS_AWAITING_BOUNDARY
    )
    case(
        "every external guard is launched by at least one boundary",
        not missing,
        f"{len(missing)} external guard(s) launched by no boundary: {missing!r}. "
        f"Each ran only as a CI job or not at all, which is how three of them "
        f"sat red on the tree while every local gate stayed green",
    )

    stale = sorted(
        name for name in EXTERNAL_GUARDS_AWAITING_BOUNDARY if launched.get(name)
    )
    case(
        "no waiver outlives its boundary",
        not stale,
        f"waived external(s) now launched by a boundary: {stale!r}. Remove the "
        f"entry from EXTERNAL_GUARDS_AWAITING_BOUNDARY: a waiver that outlives "
        f"its condition is the ratchet growing in the dark",
    )

    unknown = sorted(set(EXTERNAL_GUARDS_AWAITING_BOUNDARY) - set(EXTERNAL_GUARDS))
    case(
        "every waiver names a known external guard",
        not unknown,
        f"waiver(s) naming no external guard: {unknown!r}. A waiver on a name "
        f"the table does not carry excuses nothing and hides a typo",
    )

    # Positive control for the external table, same shape as the one above.
    invented_external = {
        "external nothing launches": r"\bcheck-nothing-launches-this\b"
    }
    case(
        "positive control: an external no boundary launches is reported",
        launched_externals(invented_external, text)
        == {"external nothing launches": False},
        "the external crossing matched a pattern no boundary carries, so its "
        "green would mean the search matches anything",
    )


# ── The invocation contract ──────────────────────────────────────────────────
# A script that answers `--help` by running its measure has not read its
# arguments: `make_scan.py --help` regenerated a tracked file, and
# `check_prose.py --help` rendered a verdict on an argument it never read.
# The contract is argparse's: `--help` prints the usage, exits 0, and touches
# nothing. The sweep asserts it over every tracked Python script, and the
# fixture controls pin that the predicate can tell a measure from an answer.


# ── The boundary that runs the corpus ────────────────────────────────────────

_RECIPE_HEAD = re.compile(r"^guards:\n", re.M)


def guards_recipe_body(justfile: str) -> str:
    """The bash body of the `guards` recipe, dedented as just runs it.

    Read rather than copied: a fixture holding a transcription of the loop
    would keep passing after the recipe itself stopped answering.
    """
    m = _RECIPE_HEAD.search(justfile)
    if m is None:
        return ""
    body: list[str] = []
    for line in justfile[m.end() :].splitlines():
        if line.strip() and not line.startswith((" ", "\t")):
            break
        body.append(line[4:] if line.startswith("    ") else line)
    return "\n".join(body) + "\n"


def _stub_recipe(body: str, subjects: list[str], externals: list[str]) -> str:
    """The same body, judging stub subjects instead of the tree's guards."""
    for name, entries in (("guards", subjects), ("externals", externals)):
        block = f"{name}=(\n" + "".join(f'  "{e}"\n' for e in entries) + ")\n"
        body, count = re.subn(
            rf"^{name}=\(\n.*?^\)\n", lambda _m: block, body, count=1, flags=re.S | re.M
        )
        if count != 1:
            return ""
    return body


def _run_recipe(tmp: Path, body: str, codes: list[int], gates: list[str]) -> tuple[int, str]:
    subjects = []
    for i, code in enumerate(codes):
        stub = tmp / f"stub{i}_{code}.py"
        stub.write_text(f"import sys\nsys.exit({code})\n", encoding="utf-8")
        subjects.append(str(stub))
    script = tmp / "guards.sh"
    script.write_text(_stub_recipe(body, subjects, gates), encoding="utf-8")
    proc = subprocess.run(
        ["bash", str(script)],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    return proc.returncode, proc.stdout + proc.stderr


def check_guards_recipe_exit() -> None:
    print("guard boundary: the recipe answers the same three codes its guards do")

    body = guards_recipe_body(
        (REPO_ROOT / "justfile").read_text(encoding="utf-8", errors="replace")
    )
    case(
        "the `guards` recipe body is read from the justfile",
        "reds=()" in body and "skips=()" in body,
        f"extracted {len(body)} char(s), which do not carry the two tallies. "
        f"The three cases below judge whatever this returns, so an empty "
        f"extraction would pass them all without running the recipe",
    )

    with tempfile.TemporaryDirectory() as raw:
        tmp = Path(raw)

        code, said = _run_recipe(tmp, body, [0, 0], ["true"])
        case(
            "positive control: everything measured and green answers 0",
            code == 0 and "3 guards green, 0 red, 0 measured nothing" in said,
            f"code {code}, output {said!r}",
        )

        code, said = _run_recipe(tmp, body, [0, 1], ["true"])
        case(
            "a guard that found a defect answers 1",
            code == 1 and "1 red" in said,
            f"code {code}, output {said!r}",
        )

        code, said = _run_recipe(tmp, body, [0, 2], ["true"])
        case(
            "a guard that measured nothing, with no red, answers 2 and not 0",
            code == 2 and "1 measured nothing" in said,
            f"code {code}, output {said!r}. Exit 0 here is the defect this case "
            f"exists to close: the boundary reports green while a guard of the "
            f"corpus has stopped measuring",
        )

        code, said = _run_recipe(tmp, body, [1, 2], ["true"])
        case(
            "a red outranks a guard that measured nothing",
            code == 1,
            f"code {code}, output {said!r}",
        )

        code, said = _run_recipe(tmp, body, [0], ["false"])
        case(
            "an external gate that failed reaches the same exit as a red guard",
            code == 1 and "1 red" in said,
            f"code {code}, output {said!r}",
        )

        code, said = _run_recipe(tmp, body, [0], ["apollia-no-such-tool-1a2b3c"])
        case(
            "an external gate whose tool is absent measured nothing, not a defect",
            code == 2 and "1 measured nothing" in said,
            f"code {code}, output {said!r}. A red here is the confusion this "
            f"case exists to close, and the externals loop shipped with it "
            f"while the guards loop above had already been taught the "
            f"distinction: a tool that is not installed observed nothing, and "
            f"calling that a defect sends an operator hunting something the "
            f"corpus never saw",
        )


def _help_reply(script: Path, cwd: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(script), "--help"],
        cwd=cwd,
        capture_output=True,
        text=True,
        timeout=120,
        check=False,
    )


def _breaks_help_contract(proc: subprocess.CompletedProcess[str]) -> str | None:
    """Why a `--help` reply breaks the contract, or None if it holds."""
    if proc.returncode != 0:
        return f"exit {proc.returncode}"
    if "usage" not in (proc.stdout + proc.stderr).lower():
        return "no usage in the reply"
    return None


def check_help_contract() -> None:
    print("invocation contract: every script answers --help without measuring")

    inventory = subprocess.run(
        ["git", "ls-files", "scripts/*.py", "scripts/**/*.py"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    tracked = sorted(set(inventory.stdout.split()))
    case(
        "the script inventory is readable and non-trivial",
        inventory.returncode == 0 and len(tracked) >= 2,
        f"`git ls-files` exited {inventory.returncode} with {len(tracked)} "
        f"script(s). A sweep over an empty corpus proves nothing",
    )
    if inventory.returncode != 0:
        return

    status = ["git", "status", "--porcelain"]
    before = subprocess.run(
        status, cwd=REPO_ROOT, capture_output=True, text=True, check=False
    )
    offenders = []
    for rel in tracked:
        why = _breaks_help_contract(_help_reply(REPO_ROOT / rel, REPO_ROOT))
        if why is not None:
            offenders.append(f"{rel} ({why})")
    after = subprocess.run(
        status, cwd=REPO_ROOT, capture_output=True, text=True, check=False
    )
    case(
        "every tracked script replies to --help with its usage, exit 0",
        not offenders,
        f"{len(offenders)} script(s) measured instead of answering: "
        f"{offenders!r}. Each one reads its arguments by position or not at "
        f"all; route them through argparse",
    )
    case(
        "the --help sweep left the tree unchanged",
        before.stdout == after.stdout,
        f"`git status --porcelain` changed across the sweep:\n"
        f"before: {before.stdout!r}\nafter: {after.stdout!r}. A --help that "
        f"writes a file is the defect this contract exists to close",
    )

    # Both directions, on fixtures rather than on the tree: a predicate that
    # accepted everything would turn the sweep above green for free.
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        bad = root / "measures_anyway.py"
        bad.write_text("import sys\nprint('scanned 12 files')\nsys.exit(1)\n")
        case(
            "positive control: a script without --help is reported",
            _breaks_help_contract(_help_reply(bad, root)) is not None,
            "a script that ran its measure and exited 1 satisfied the "
            "predicate, so the sweep above matches anything",
        )
        good = root / "answers.py"
        good.write_text(
            "import argparse\n"
            "argparse.ArgumentParser(description='fixture').parse_args()\n"
        )
        case(
            "negative control: a compliant script passes the predicate",
            _breaks_help_contract(_help_reply(good, root)) is None,
            "a script that answered usage on exit 0 was reported, so the "
            "sweep would demand what argparse cannot provide",
        )


# ── The panic-free sweep ─────────────────────────────────────────────────────
# Same family as the font CDN block above, and the same reason to be here: a
# sweep whose tree is clean has never been shown to work. This one is worse off
# than most, since the lint it replaces is green on the very sites it misses,
# so a green sweep and a green lint say nothing about each other. The four
# shapes below must be caught and the four beside them must stay silent, and
# the exclusion is pinned by name, because its failure mode is to drop
# production files while every printed count stays where it was.

PANIC_BARE = """\
fn run() -> usize {
    let value: Option<usize> = Some(1);
    value.unwrap()
}
"""

PANIC_ALLOW_ONLY = """\
fn run() -> usize {
    let value: Option<usize> = Some(1);
    #[allow(clippy::unwrap_used)]
    value.unwrap()
}
"""

PANIC_SAFETY_TOO_FAR = """\
fn run() -> usize {
    // SAFETY: the value is a literal written three lines below.
    let value: Option<usize> = Some(1);
    let doubled = value;
    let tripled = doubled;
    tripled.unwrap()
}
"""

PANIC_AFTER_TEST_MOD = """\
#[cfg(test)]
mod tests {
    #[test]
    fn t() {
        Some(1).unwrap();
    }
}

pub fn run() -> usize {
    Some(2).unwrap()
}
"""

PANIC_SAFETY_ABOVE = """\
fn run() -> usize {
    // SAFETY: the value on the line below is a literal.
    Some(1).unwrap()
}
"""

PANIC_CLOUD_TEST_BLOCK = """\
#[cfg(all(test, feature = "cloud"))]
mod tests {
    #[test]
    fn t() {
        Some(1).unwrap();
    }
}
"""

PANIC_DOC_EXAMPLE = """\
//! ```
//! let cwd = std::env::current_dir().unwrap();
//! ```
"""

PANIC_EXPECT = """\
fn run() -> usize {
    Some(1).expect("nope")
}
"""

PANIC_CFG_ANY_TARGET = """\
#[cfg(any(target_os = "linux", test))]
pub fn run() -> usize {
    Some(2).unwrap()
}
"""

PANIC_CFG_ANY_KANI = """\
#[cfg(any(test, kani))]
mod proofs {
    fn p() {
        Some(1).unwrap();
    }
}
"""


def _accused(text: str) -> list[int]:
    """The lines the sweep refuses in a file no `EXPECT_RATCHET` entry covers.

    The verdict comes from `panicfree.refuses` rather than a copy of it here,
    so the day the rule changes this block measures the new rule. The default
    path is under no ratchet entry, which is where `expect()` is held to what
    `unwrap()` has always been held to.
    """
    return [s.line for s in panicfree.sites(text) if panicfree.refuses(s)]


def check_panic_sweep_fires() -> None:
    print("panic-free sweep: the shapes a lint gracies are seen, and only those")

    for name, sample, line in (
        ("a bare unwrap in production", PANIC_BARE, 3),
        ("an unwrap under #[allow] alone", PANIC_ALLOW_ONLY, 4),
        ("an unwrap whose SAFETY line is four lines up", PANIC_SAFETY_TOO_FAR, 6),
        ("an unwrap after a closed test module", PANIC_AFTER_TEST_MOD, 10),
        ("an unwrap under cfg(any(<target>, test)), production on that target", PANIC_CFG_ANY_TARGET, 3),
        ("a bare expect in production", PANIC_EXPECT, 2),
    ):
        case(
            f"accuses {name}",
            _accused(sample) == [line],
            f"expected one accusation on line {line}, got {_accused(sample)!r}. "
            f"The sweep exists to name the site, and a site it cannot name is a "
            f"site nobody fixes",
        )

    exempted = panicfree.sites(PANIC_SAFETY_ABOVE)
    case(
        "stays silent on an unwrap whose SAFETY line is right above",
        len(exempted) == 1 and exempted[0].exempt,
        f"got {exempted!r}. A sweep that refuses the exemption the corpus grants "
        f"gets switched off, and then it guards nothing",
    )
    for name, sample in (
        ("a #[cfg(all(test, feature = ...))] module", PANIC_CLOUD_TEST_BLOCK),
        ("a #[cfg(any(test, kani))] module", PANIC_CFG_ANY_KANI),
        ("a doc example that unwraps", PANIC_DOC_EXAMPLE),
    ):
        case(
            f"finds no site in {name}",
            panicfree.sites(sample) == [],
            f"got {panicfree.sites(sample)!r}. Test code and prose are not "
            f"production, and counting them is how the first count of this "
            f"rule reached eighty-nine",
        )

    expect_sites = panicfree.sites(PANIC_EXPECT)
    case(
        "counts an expect as an expect, not as an unwrap",
        [(s.form, s.exempt) for s in expect_sites] == [("expect", False)],
        f"got {expect_sites!r}. `expect()` is the half of the rule no lint "
        f"covers, so a sweep that drops it leaves the larger hole open while "
        f"looking closed",
    )

    ratcheted = next(iter(panicfree.EXPECT_RATCHET), None)
    if ratcheted is not None:
        case(
            "counts, rather than refuses, an expect under a ratcheted crate",
            not panicfree.refuses(expect_sites[0], f"{ratcheted}src/lib.rs"),
            f"the sweep refused a site under {ratcheted}, which the ratchet "
            f"still covers. A ratchet that refuses what it is counting turns "
            f"the whole tree red and gets the guard switched off",
        )


def check_panic_sweep_scope() -> None:
    print("panic-free sweep: the exclusion drops test modules and keeps production")

    paths = panicfree.tracked_sources()
    case(
        "the inventory is readable and has a corpus",
        len(paths) >= 100,
        f"`git ls-files -- crates/*/src/*.rs` returned {len(paths)} path(s). A "
        f"sweep pointed at an empty tree reports success for the same reason a "
        f"clean one does",
    )
    if not paths:
        return

    cache: dict[str, str | None] = {}

    def read(path: str) -> str | None:
        if path not in cache:
            target = panicfree.REPO_ROOT / path
            cache[path] = (
                target.read_text(encoding="utf-8", errors="replace")
                if target.is_file()
                else None
            )
        return cache[path]

    excluded = panicfree.excluded_modules(paths, read)
    dropped = "crates/apollia-runtime/src/supervisor/tests.rs"
    case(
        "a module declared under #[cfg(test)] is dropped",
        dropped in excluded,
        f"{dropped} stayed in the sweep. Six files of this shape carried "
        f"seventy-five of the eighty-nine sites the first count reported",
    )
    kept = [
        "crates/apollia-runtime/src/audit_journal/signer.rs",
        "crates/apollia-runtime/src/audit_journal/subscriber.rs",
        "crates/apollia-desktop/src/main.rs",
        "crates/apollia-core/src/budget.rs",
        "crates/apollia-mcp/src/approvals.rs",
    ]
    present = [path for path in kept if path in paths]
    case(
        "the five production files named here are in the inventory",
        len(present) == len(kept),
        f"missing from `git ls-files`: {sorted(set(kept) - set(present))!r}. A "
        f"case whose subject is absent proves nothing about the exclusion",
    )
    leaked = [path for path in present if path in excluded]
    case(
        "production sitting under a gated sibling is kept",
        not leaked,
        f"dropped from the sweep: {leaked!r}. `audit_journal/mod.rs` gates "
        f"`proofs` and nothing else, and an inner `#![cfg_attr(test, allow(...))]` "
        f"gates nothing at all: reading either as a gate removes thirty-eight "
        f"production files while every printed count stays put",
    )

    # Positive control on the same query: without it, the case above would be
    # green whenever `excluded_modules` returns an empty set, which is the one
    # way it can be wrong and look right.
    case(
        "positive control: the exclusion is not simply empty",
        len(excluded) >= 10,
        f"the exclusion holds {len(excluded)} file(s), so the case above says "
        f"nothing: an exclusion that drops nothing keeps production by accident",
    )


# ── The design token guard: what can style, and a ratchet that only descends ──

# The showcase routes document the design system by naming its values, so the
# sweep this guard was promoted from reported the documentation as the defect.
# `hsl(28 11% 13%)` below sits in element text content, `text-[11px]` sits in a
# class attribute, and one line carries both: a detector that answered "one
# finding" by dropping the wrong one would pass a count-only assertion.
TOKENS_PROSE = (
    '<p>warmth dark (<code class="text-[11px]">surface-2 = hsl(28 11% 13%)</code>)</p>\n'
)
TOKENS_SCRIPT = '<script>\n  const cls = "bg-white";\n</script>\n<p>ok</p>\n'
TOKENS_STYLE = "<div></div>\n<style>\n  .x { font-size: 11px; }\n</style>\n"
TOKENS_ARROW = '<button onclick={() => go()} class="text-[9px]">go</button>\n'
TOKENS_CLEAN = '<p class="text-caption text-foreground">clean</p>\n'
# One pair per second-stage judgement: the same shape written on a value a
# token carries, and on a value none carries.
TOKENS_COVERED = (
    ".a { z-index: 40; }\n"
    ".b { box-shadow: 0 4px 16px -2px hsl(237 47% 57% / 0.35); }\n"
    ".c { border-radius: 8px; }\n"
    '<div class="w-[16px] duration-200"></div>\n'
)
TOKENS_UNCOVERED = (
    ".a { z-index: 64; }\n"
    ".b { box-shadow: 0 0 0 3px hsl(237 47% 57% / 0.35); }\n"
    ".c { border-radius: 32px; }\n"
    '<div class="w-[88px] duration-75"></div>\n'
)


def check_design_tokens_lens() -> None:
    print("design tokens: a literal counts where it can style, not where it is described")

    prose = designtokens.scan(TOKENS_PROSE, ".svelte", False, set())
    case(
        "the class attribute on the prose line is flagged",
        [f for f in prose if f[1] == "font-size"] != [],
        f"the guard read no attribute on a line that mixes prose and classes, "
        f"so the mask swallowed the defect with the documentation. "
        f"findings: {prose!r}",
    )
    case(
        "the literal in element text content is not flagged",
        [f for f in prose if f[1] == "color"] == [],
        f"documented `hsl(28 11% 13%)` was counted as a hardcoded colour. That "
        f"is the sweep's reading, and a guard that reports documentation as "
        f"debt is a guard someone excludes. findings: {prose!r}",
    )

    script = designtokens.scan(TOKENS_SCRIPT, ".svelte", False, set())
    case(
        "a palette class inside the <script> body is flagged",
        [f for f in script if f[1] == "color"] != [],
        f"the script body was read as prose. Class strings live there, in "
        f"variant tables and `class:` expressions. findings: {script!r}",
    )

    style = designtokens.scan(TOKENS_STYLE, ".svelte", False, set())
    case(
        "a declaration inside the <style> body is flagged",
        [f for f in style if f[1] == "font-size"] != [],
        f"the style body was read as prose, which would blind the guard to "
        f"every component-scoped stylesheet. findings: {style!r}",
    )

    arrow = designtokens.scan(TOKENS_ARROW, ".svelte", False, set())
    case(
        "an arrow function in an attribute does not end the tag early",
        [f for f in arrow if f[1] == "font-size"] != [],
        f"the `>` of `=>` closed the tag, so every class written after a "
        f"handler fell into what the guard calls prose. This is the most "
        f"common attribute order in the tree. findings: {arrow!r}",
    )

    clean = designtokens.scan(TOKENS_CLEAN, ".svelte", False, set())
    case(
        "positive control: a tokenised class is not flagged",
        clean == [],
        f"a compliant class was reported. A guard that fails on compliant "
        f"input gets switched off, and then guards nothing. findings: {clean!r}",
    )

    covered = designtokens.scan(TOKENS_COVERED, ".css", False, {40})
    verdicts = sorted({f[3] for f in covered if f[1] != "color"})
    case(
        "a value a token carries is a defect, whatever its family",
        verdicts == ["defect"],
        f"a layer, an elevation, a corner and a rung the tree declares were "
        f"not all called defects. The second stage is allowed to excuse a "
        f"value no token names, never one that has a token to move to. "
        f"verdicts: {verdicts!r}",
    )

    uncovered = designtokens.scan(TOKENS_UNCOVERED, ".css", False, {40})
    reasons = sorted({f[3] for f in uncovered if f[1] != "color"})
    case(
        "a value no token carries is counted with its reason, not as a defect",
        reasons == [
            "uncovered:no-layer-token",
            "uncovered:no-ring-token",
            "uncovered:off-motion-scale",
            "uncovered:off-radius-scale",
            "uncovered:off-spacing-scale",
        ],
        f"a rung that does not exist was reported as a defect, or dropped. "
        f"Reporting `w-[88px]` sends a reader looking for a token that was "
        f"never declared, and dropping it makes the count green for the wrong "
        f"reason. reasons: {reasons!r}",
    )

    scale = designtokens.spacing_px()
    layers = designtokens.declared_z_values(designtokens.APP_CSS.read_text(encoding="utf-8"))
    case(
        "the scales are read from the tree, not carried in the guard",
        scale > designtokens.SPACING_PX_BASE and len(layers) >= 3,
        f"the spacing rungs the config adds ({sorted(scale - designtokens.SPACING_PX_BASE)!r}) "
        f"or the layers app.css declares ({sorted(layers)!r}) did not reach the "
        f"judgement. A scale copied into the guard drifts from the one the "
        f"tree uses, and then the guard judges against a scale nobody writes",
    )


def check_design_tokens_ratchet() -> None:
    print("design tokens: the allowance fails above its number and below it")

    findings = [(1, "font-size", "text-[10px]")] * 3
    measured = {"src/x.svelte": findings}
    saved = designtokens.ALLOWED
    try:
        designtokens.ALLOWED = {"src/x.svelte": 2}
        over = designtokens.verdict(measured)
        case(
            "a file above its allowance fails",
            len(over) == 1 and "3 literal(s), 2 allowed" in over[0],
            f"a file that grew past its number passed, which makes the "
            f"allowance a licence rather than a ratchet. verdict: {over!r}",
        )

        designtokens.ALLOWED = {"src/x.svelte": 4}
        under = designtokens.verdict(measured)
        case(
            "a file below its allowance fails, naming the new number",
            len(under) == 1 and "Lower it to 3" in under[0],
            f"a file that improved kept its old number in silence. The list "
            f"then outlives the debt and nobody can retire it. "
            f"verdict: {under!r}",
        )

        designtokens.ALLOWED = {"src/x.svelte": 3}
        case(
            "positive control: a file exactly at its allowance passes",
            designtokens.verdict(measured) == [],
            "a file matching its number was reported, which would make the "
            "guard permanently red and unreadable",
        )

        designtokens.ALLOWED = {"src/gone.svelte": 1}
        stale = designtokens.verdict({})
        case(
            "an entry whose file is clean is reported as stale",
            len(stale) == 1 and "no literal left" in stale[0],
            f"an allowance survived the debt it covered. verdict: {stale!r}",
        )

        designtokens.ALLOWED = {}
        unlisted = designtokens.verdict(measured)
        case(
            "a file named by no entry is allowed nothing",
            len(unlisted) == 1 and "0 allowed" in unlisted[0],
            f"an unlisted file passed with findings, so `src/routes/` could "
            f"drift back the day after it was migrated. verdict: {unlisted!r}",
        )
    finally:
        designtokens.ALLOWED = saved

    case(
        "the real allowance still carries debt",
        sum(saved.values()) > 0,
        f"the allowance holds {len(saved)} entr(y/ies) for "
        f"{sum(saved.values())} literal(s), so the cases above exercise a "
        f"table the tree does not have: a ratchet over nothing ratchets "
        f"nothing. When the last entry goes, this case is what has to say so, "
        f"by being rewritten to demand an empty table",
    )


# ── The binary a guard judges ────────────────────────────────────────────────
# The same family as everything above, on the one input none of these guards
# owns. Four of them render a verdict by running `target/debug/apollia-os`, and
# each tested that the file exists and nothing else. Five times in one campaign
# the artefact at that path came from somewhere else, and the four answered
# 183 contract breaches, 6 refused documentation lines, 198 leaves instead of
# 199 and PASS 156 FAIL 16: precise, plausible, and about another tree. A
# rebuild turned all of it green without one file changing.
#
# `binary_freshness.py` is the control the four now read, and it is held to the
# property this file exists for: what it cannot establish is reported as
# nothing measured, with the command that repairs it, never as a defect of the
# tree. Every negative case is paired with the positive control, because a
# control that always refused would satisfy the negative half and take the four
# guards out of service.

FRESHNESS_CALLERS = (
    "scripts/check_cli_json_contract.py",
    "scripts/check_cli_e2e_coverage.py",
    "scripts/check_entry_doc_commands.py",
    "tests/cli/cli-e2e.sh",
)


def _freshness_tree(root: Path, *, anchor: bool = True) -> tuple[Path, Path]:
    """A tree of two sources, a binary, and the dep-info cargo writes beside it.

    The timestamps are set rather than inherited: the whole criterion is an
    order between them, and a fixture that let the filesystem decide would
    assert nothing.
    """
    sources = []
    if anchor:
        sources.append(root / binary_freshness.ANCHOR)
    sources.append(root / "crates/apollia-core/src/lib.rs")
    for source in sources:
        source.parent.mkdir(parents=True, exist_ok=True)
        source.write_text("fn main() {}\n", encoding="utf-8")

    binary = root / "target/debug/apollia-os"
    binary.parent.mkdir(parents=True, exist_ok=True)
    binary.write_bytes(b"\x7fELF")
    dep_info = binary_freshness.dep_info_of(binary)
    dep_info.write_text(
        f"{binary}: " + " ".join(str(s) for s in sources) + "\n",
        encoding="utf-8",
    )

    built = 1_700_000_000.0
    for source in sources:
        os.utime(source, (built - 10, built - 10))
    os.utime(binary, (built, built))
    os.utime(dep_info, (built + 0.03, built + 0.03))
    return binary, dep_info


def _refusal(binary: Path, tree: Path) -> tuple[int | None, str]:
    out = io.StringIO()
    code = binary_freshness.require(binary, tree, stream=out, report=out)
    return code, out.getvalue()


def check_binary_freshness() -> None:
    print("binary freshness: a guard proves its binary came from the tree it judges")

    # A space inside a path is escaped, and cargo writes one rule per source
    # with an empty right-hand side. A parser that read those targets as
    # sources would compare the artefact against itself.
    dep_info_sample = "/t/target/debug/apollia-os: /t/a.rs /t/with\\ space.rs\n/t/a.rs:\n"
    parsed = binary_freshness.dep_sources(dep_info_sample)
    case(
        "the dep-info parser reads prerequisites and skips rules without any",
        parsed == [Path("/t/a.rs"), Path("/t/with space.rs")],
        f"parsed {parsed!r} out of {dep_info_sample!r}",
    )

    with tempfile.TemporaryDirectory() as raw:
        tree = Path(raw)
        binary, dep_info = _freshness_tree(tree)

        code, said = _refusal(binary, tree)
        case(
            "positive control: a binary this tree produced is judged",
            code is None and "built from this tree, 2 sources" in said,
            f"code {code}, output {said!r}. A control that refuses a correct "
            f"build takes the four guards out of service, which is the same "
            f"loss as the false verdict it replaces",
        )

        # P2, currency. The measured shape: the artefact of 2026-08-06 came
        # back under a tree whose sources had moved on.
        source = tree / binary_freshness.ANCHOR
        os.utime(source, (1_700_000_100, 1_700_000_100))
        code, said = _refusal(binary, tree)
        case(
            "a source newer than the binary is nothing measured, not a defect",
            code == binary_freshness.NOTHING_MEASURED
            and "1 source(s) changed" in said
            and binary_freshness.REMEDY in said,
            f"code {code}, output {said!r}. Expected 2, never 1: a stale "
            f"artefact is not a defect of the tree, and the remedy has to be "
            f"printed or the reader is left with a red they cannot resolve",
        )
        os.utime(source, (1_699_999_990, 1_699_999_990))

        # P1, provenance. The measured shape: a build launched in another
        # working tree of the same repository rewrote the shared artefact, and
        # every timestamp stayed in order.
        dep_info.write_text(
            f"{binary}: /elsewhere/{binary_freshness.ANCHOR}\n", encoding="utf-8"
        )
        os.utime(dep_info, (1_700_000_000.03, 1_700_000_000.03))
        code, said = _refusal(binary, tree)
        case(
            "a binary built from another working tree is nothing measured",
            code == binary_freshness.NOTHING_MEASURED
            and "was not built from" in said
            and f"/elsewhere/{binary_freshness.ANCHOR}" in said
            and binary_freshness.REMEDY in said,
            f"code {code}, output {said!r}. This is the shape no timestamp can "
            f"see: the artefact was the newest file of the three and still came "
            f"from pre-campaign code",
        )

        # A source the build read and the tree no longer has. Same family as
        # the two above, and it must not read as an unchanged tree.
        gone = tree / "crates/apollia-core/src/lib.rs"
        dep_info.write_text(f"{binary}: {tree / binary_freshness.ANCHOR} {gone}\n", encoding="utf-8")
        os.utime(dep_info, (1_700_000_000.03, 1_700_000_000.03))
        gone.unlink()
        code, said = _refusal(binary, tree)
        case(
            "a source the build read and the tree lost is nothing measured",
            code == binary_freshness.NOTHING_MEASURED and "no longer exist" in said,
            f"code {code}, output {said!r}. A deleted module leaves the "
            f"remaining timestamps in order, so silence here would pass a "
            f"binary compiled from files the tree does not have",
        )

        dep_info.unlink()
        code, said = _refusal(binary, tree)
        case(
            "a binary with no dep-info beside it is nothing measured",
            code == binary_freshness.NOTHING_MEASURED
            and "is absent" in said
            and binary_freshness.REMEDY in said,
            f"code {code}, output {said!r}. Without the dep-info nothing states "
            f"which sources produced the artefact, and a control that passed on "
            f"absent evidence is the success bias itself",
        )

    # The control must not be able to stop controlling in silence. Rename the
    # anchor and the provenance predicate has nothing to recognise a build of
    # this tree by, which has to be reported rather than passed.
    with tempfile.TemporaryDirectory() as raw:
        tree = Path(raw)
        binary, _ = _freshness_tree(tree, anchor=False)
        code, said = _refusal(binary, tree)
        case(
            "an anchor absent from the tree is nothing measured",
            code == binary_freshness.NOTHING_MEASURED
            and binary_freshness.ANCHOR in said,
            f"code {code}, output {said!r}. The day the CLI entry point is "
            f"renamed, this control would recognise nothing and pass "
            f"everything, which is the guard that examines nothing reporting a "
            f"pass",
        )

    # The crossing, same shape as the guard-boundary one above: a control four
    # callers were meant to read is worth nothing in the caller that does not.
    absent = [
        rel
        for rel in FRESHNESS_CALLERS
        if "binary_freshness" not in (REPO_ROOT / rel).read_text(encoding="utf-8")
    ]
    case(
        "the four guards over this artefact all read the control",
        not absent,
        f"{len(absent)} caller(s) render a verdict on the binary without the "
        f"control: {absent!r}. The one that skips it keeps answering the "
        f"question the other three stopped answering wrongly",
    )
    case(
        "positive control: a file that does not read it is seen as absent",
        "scripts/check_prose.py"
        not in [
            rel
            for rel in ("scripts/check_prose.py",)
            if "binary_freshness" in (REPO_ROOT / rel).read_text(encoding="utf-8")
        ],
        "a guard that never touches the binary was counted as a reader, so the "
        "crossing above matches anything and proves nothing",
    )


def main() -> int:
    check_builder_sweep()
    check_claims_wired()
    check_zero_coverage_is_reported()
    check_font_cdn_detector_fires()
    print()
    check_font_cdn_scan_is_tree_invariant()
    print()
    check_prose_tracker_rule()
    print()
    check_worktree_comparator()
    print()
    check_red_tests_are_named()
    print()
    check_automation_report_reader()
    print()
    check_e2e_failure_detail()
    print()
    check_guards_are_launched()
    print()
    check_guards_recipe_exit()
    print()
    check_help_contract()
    print()
    check_design_tokens_lens()
    print()
    check_design_tokens_ratchet()
    print()
    check_panic_sweep_fires()
    print()
    check_panic_sweep_scope()
    print()
    check_binary_freshness()
    if FAILURES:
        print(f"\n{len(FAILURES)} self-test failure(s):\n", file=sys.stderr)
        for f in FAILURES:
            print(f"  {f}\n", file=sys.stderr)
        return 1
    print(
        "\nsixteen properties hold: neither a comment nor a re-export is a use, "
        "zero coverage says so, the font guard fires on a dirty tree and reads "
        "the same set whatever tree it runs in, the prose tracker rule fires "
        "and its one exemption is bounded from both sides, two equal exit codes "
        "over different measures are not the same verdict, and a failed "
        "assertion reaches the artifact with its cause while a passing one adds "
        "nothing to it, and the heavy-guard recorder names the tests that made "
        "a suite red where a green one invents none, every tracked guard is "
        "named by a file that launches "
        "it, the recipe that launches them answers 2 rather than 0 when one "
        "of them measured nothing, every tracked script answers --help "
        "without measuring, "
        "the panic-free sweep names the sites a lint gracies, refuses an "
        "expect on the same terms as an unwrap, and keeps the production an "
        "attribute read by proximity would drop, the desktop automation "
        "verdict is read with staleness treated as nothing measured, and the "
        "design token guard reads what can style something rather than what "
        "is written about it, tells a value a token carries from one no token "
        "names, and holds both behind a ratchet that fails in both directions, "
        "and the four guards that judge the built binary refuse to judge one "
        "their tree did not produce, with the command that repairs it"
    )
    return 0


if __name__ == "__main__":
    argparse.ArgumentParser(description=__doc__.splitlines()[0]).parse_args()
    sys.exit(main())
