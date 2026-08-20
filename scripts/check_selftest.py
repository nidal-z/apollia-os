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
  4. A guard that reports a red says why. The CLI E2E suite computed the cause
     of every failed assertion and threw it away before writing its artifact,
     which is the same bias one step further on: the reader who cannot resolve
     the red falls back on believing the green.
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

Each case asserts both directions. A check that always answered "not wired", or
that printed a coverage table of zeros, would satisfy the negative half while
being worthless, so every negative case is paired with a positive control.

Usage:
    python3 scripts/check_selftest.py
"""

import json
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
import check_prose  # noqa: E402
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
    under_root = any(root == path or root in path.parents for root in fontcdn.SCAN_ROOTS)
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
        ("a tracked file beside the generated map", "crates/apollia-desktop/ui/figma/README.md"),
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
        print(f"  note  {len(pending)} untracked source(s) read in this tree only: {pending[:3]}")

    hidden = sorted(rel for rel in tracked if _in_scope(rel) and fontcdn.is_excluded(Path(rel)))
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
        regrown = [rel for rel in tracked if _in_scope(rel) and fontcdn.is_excluded(Path(rel))]
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
                "docs/shot.png": b"\x89PNG\r\n\x1a\n\xff\xfe" + TRACKED_SAMPLE.encode("utf-8"),
                DIRTY_NAME: CLEAN_SAMPLE.encode("utf-8"),
                CLEAN_NAME: CLEAN_SAMPLE.encode("utf-8"),
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
            "flags the vocabulary carried by a file name",
            any(f == f"{DIRTY_NAME}: " + check_prose.RULES[-1].label + ", in the file name" for f in found),
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
    "cargo-test": {"exit": 101, "binaries": 77, "tests": 4370},
    "cli-e2e": {"exit": 0, "pass": 154, "fail": 0},
    "ui-build": {"exit": 0},
    "svelte-check": {"exit": 1, "files": 4943, "errors": 1},
    "vitest": {"exit": 0, "tests": 790},
    "docs-build": {"exit": 0},
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
                    "measures": {"exit": 0, "binaries": 77, "tests": 4370},
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
                    "measures": {"exit": 101, "binaries": 0, "tests": 0},
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
    check_e2e_failure_detail()
    if FAILURES:
        print(f"\n{len(FAILURES)} self-test failure(s):\n", file=sys.stderr)
        for f in FAILURES:
            print(f"  {f}\n", file=sys.stderr)
        return 1
    print(
        "\neight properties hold: neither a comment nor a re-export is a use, "
        "zero coverage says so, the font guard fires on a dirty tree and reads "
        "the same set whatever tree it runs in, the prose tracker rule fires "
        "and its one exemption is bounded from both sides, two equal exit codes "
        "over different measures are not the same verdict, and a failed "
        "assertion reaches the artifact with its cause while a passing one adds "
        "nothing to it"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
