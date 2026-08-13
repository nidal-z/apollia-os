#!/usr/bin/env python3
"""Pin that five declared instruments answer the question they are asked.

`scripts/check_guard_verdicts.py` pins a different property: that a command
declared a guard *can* render a negative verdict. The five instruments here all
render one. The verdict is plausible, and it is about something else:

  - `just _bundle-python` was asked whether the Python bundle link was laid
    down. It exited 0 after laying down nothing, and its four callers read its
    standard output rather than its exit code, so they carried on with no
    interpreter.
  - `scripts/worktree_verdicts.py --compare` was asked whether two trees answer
    the same verdict. It compared two sentinels that both meant "not
    extracted", found them equal, and concluded conformity.
  - `just --list` was asked what a recipe does. `just` renders the last line of
    the comment block attached to a recipe, so fifteen recipes advertised a
    usage example or a continuation line instead of their purpose.
  - `cargo fmt --check` was asked whether the tree is well formatted. From a
    directory of the repository that holds no `Cargo.toml` it exits 1 without
    having looked at any file, because a workspace with several members and no
    `default-members` leaves it without a current package.
  - `tests/cli/cli-e2e.sh` was asked whether the command line behaves. A
    relative `APOLLIA_BIN` was carried unresolved through 153 assertions and
    failed on the only one that changes directory first, so the red named an
    assertion rather than a malformed input.

Each instrument carries a pair in both directions on the same subject: the
command answers green on a conforming subject and red on a deliberately faulty
one, and both outputs are printed. One direction proves nothing, since an
instrument that always answered "faulty" would satisfy the negative half while
being worthless.

Two properties this file holds itself to, both inherited from
`scripts/check_guard_verdicts.py`:

  1. Faulty subjects are built in a temporary directory. The real tree is never
     written to in order to produce a proof.
  2. A pair whose subject is absent is reported as zero coverage and makes this
     command exit non-zero. A pair that examined nothing is not a pair that
     holds.

What this file does not see, declared rather than discovered:

  - The fifteen repaired descriptions are checked for shape, not for meaning. A
    command can verify that the block attached to a recipe is one line; it
    cannot verify that the line says what the recipe does.
  - A recipe carrying no comment block at all satisfies the shape rule without
    ever being examined. Five do today.
  - The `cargo fmt` caller sweep reads tracked files, so an untracked file
    declaring the command is out of its reach.
  - That sweep also skips this file, whose fixtures carry the faulty form on
    purpose. A real declaration written here would go unseen.

Usage:
    python3 scripts/check_instrument_verdicts.py
"""

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import worktree_verdicts  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[1]
SELF = Path(__file__).resolve().relative_to(REPO_ROOT).as_posix()

ANSI = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")


class Pair:
    """One instrument, its two directions, and whatever controls it needs.

    `state` is the terminal state the instrument reaches. `falsifiable` means
    it answers the question in both directions. `declared silent` would mean it
    cannot honestly answer and says so where its reader reads its verdict; no
    instrument here is in that state, and the count prints the zero rather than
    hiding it.
    """

    def __init__(self, name: str, question: str) -> None:
        self.name = name
        self.question = question
        self.state = "falsifiable"
        self.failures: list[str] = []
        self.uncovered: list[str] = []
        self.directions: set[str] = set()

    def case(self, direction: str, label: str, ok: bool, detail: str) -> None:
        self.directions.add(direction)
        if ok:
            print(f"  ok    [{direction}] {label}")
        else:
            print(f"  FAIL  [{direction}] {label}")
            self.failures.append(f"{self.name}, {label}: {detail}")

    def zero_coverage(self, reason: str) -> None:
        print(f"  ZERO COVERAGE  {self.name}")
        print(f"                 {reason}")
        self.uncovered.append(f"{self.name}: {reason}")

    def holds(self) -> bool:
        return (
            not self.failures
            and not self.uncovered
            and {"green", "red"} <= self.directions
        )

    def missing_directions(self) -> list[str]:
        return sorted({"green", "red"} - self.directions)


def announce(pair: Pair) -> None:
    print(f"{pair.name}: {pair.question}")


# ── Site 1, the recipe that prepares the desk ────────────────────────────────


def _just_bundle_python(just: str, root: Path) -> subprocess.CompletedProcess:
    return subprocess.run(
        [
            just,
            "--justfile",
            str(REPO_ROOT / "justfile"),
            "--working-directory",
            str(root),
            "_bundle-python",
        ],
        capture_output=True,
        text=True,
    )


def check_bundle_python(pair: Pair) -> None:
    announce(pair)
    just = shutil.which("just")
    if just is None:
        pair.zero_coverage(
            "just is not on PATH, so neither direction of this pair could run "
            "the recipe. Install it with: cargo install just"
        )
        return
    if not (REPO_ROOT / "justfile").exists():
        pair.zero_coverage(f"no justfile at {REPO_ROOT}, so this pair examined nothing")
        return

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        bundle = root / "target/debug/python"
        interpreter = bundle / "bin/python3.13"
        interpreter.parent.mkdir(parents=True)
        interpreter.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        interpreter.chmod(0o755)

        run = _just_bundle_python(just, root)
        lines = [ln for ln in run.stdout.splitlines() if ln.strip()]
        last = lines[-1] if lines else ""
        pair.case(
            "green",
            "a tree holding a bundle gets exit 0, the resolved path and the link",
            run.returncode == 0
            and last == str(bundle)
            and (root / "target/Resources/python").is_symlink(),
            f"exit {run.returncode}, last stdout line {last!r}, link exists "
            f"{(root / 'target/Resources/python').is_symlink()}. Expected exit 0, "
            f"{str(bundle)!r} and True.\n{run.stdout}{run.stderr}",
        )

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp).resolve()
        run = _just_bundle_python(just, root)
        pair.case(
            "red",
            "a tree holding no bundle gets exit 1, the remedy, and no link",
            run.returncode == 1
            and "build-python-bundle.sh" in run.stderr
            and not (root / "target/Resources").exists(),
            f"exit {run.returncode}, remedy named "
            f"{'build-python-bundle.sh' in run.stderr}, target/Resources exists "
            f"{(root / 'target/Resources').exists()}. Expected exit 1, True, "
            f"False.\n{run.stdout}{run.stderr}",
        )

    # The callers read standard output under `set -euo pipefail`, so the exit
    # code has to reach them through the command substitution rather than
    # through a string they test afterwards.
    callers = [
        line.strip()
        for line in (REPO_ROOT / "justfile").read_text(encoding="utf-8").splitlines()
        if "just _bundle-python" in line
    ]
    pair.case(
        "control",
        "every caller reads the recipe through a command substitution",
        bool(callers) and all('$(just _bundle-python' in line for line in callers),
        f"call sites found: {callers}",
    )


# ── Site 2, the comparator ───────────────────────────────────────────────────

# One full set of measures per guard, in the shape the recorder writes. The key
# set is checked against GUARDS below, so a guard added there without a fixture
# here is reported rather than silently skipped.
FULL_MEASURES: dict[str, dict[str, int | None]] = {
    "cargo-check": {"exit": 0},
    "cargo-clippy": {"exit": 0},
    "cargo-test": {"exit": 0, "binaries": 80, "tests": 4370},
    "cli-e2e": {"exit": 0, "pass": 154, "fail": 0},
    "ui-build": {"exit": 0},
    "svelte-check": {"exit": 1, "files": 4943, "errors": 1},
    "vitest": {"exit": 0, "tests": 790},
    "docs-build": {"exit": 0},
}

HEAD = "0f8b2c1d4a6e9f30b5c7d8e1a2f3b4c5d6e7f809"


def _record_file(path: Path, overrides: dict[str, dict[str, int | None]]) -> None:
    guards = {
        key: {"prepared": True, "measures": dict(overrides.get(key, measures)), "seconds": 1.0}
        for key, measures in FULL_MEASURES.items()
    }
    path.write_text(
        json.dumps(
            {
                "tree": str(path.parent),
                "head": HEAD,
                "porcelain_lines": 0,
                "python_bundle": "/fixture/python",
                "guards": guards,
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )


def _compare(left: Path, right: Path) -> subprocess.CompletedProcess:
    return subprocess.run(
        [
            sys.executable,
            str(REPO_ROOT / "scripts/worktree_verdicts.py"),
            "--compare",
            str(left),
            str(right),
        ],
        capture_output=True,
        text=True,
    )


def check_worktree_comparator(pair: Pair) -> None:
    announce(pair)

    declared = {guard.key for guard in worktree_verdicts.GUARDS}
    if declared != set(FULL_MEASURES):
        pair.zero_coverage(
            "the fixture no longer covers the declared guards: "
            f"{sorted(declared ^ set(FULL_MEASURES))} differ, so this pair "
            "would compare records the recorder never writes"
        )
        return

    unextracted = {"exit": 0, "binaries": None, "tests": None}

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        left, right = root / "main.json", root / "worktree.json"

        _record_file(left, {})
        _record_file(right, {})
        run = _compare(left, right)
        pair.case(
            "green",
            "two records whose measures were all extracted compare equal",
            run.returncode == 0,
            f"exit {run.returncode}\n{run.stdout}{run.stderr}",
        )

        # The direction that decides whether the repair holds. Both sides failed
        # to extract, which is the shape of the original defect: two sentinels
        # compared as ordinary values and found equal.
        _record_file(left, {"cargo-test": unextracted})
        _record_file(right, {"cargo-test": unextracted})
        run = _compare(left, right)
        pair.case(
            "red",
            "two records that both extracted nothing do not compare equal",
            run.returncode == 1 and "not measured" in run.stdout,
            f"exit {run.returncode}, cell rendered "
            f"{'not measured' in run.stdout}. Expected exit 1 and True.\n"
            f"{run.stdout}{run.stderr}",
        )

        _record_file(left, {})
        _record_file(right, {"cli-e2e": {"exit": 1, "pass": None, "fail": None}})
        run = _compare(left, right)
        pair.case(
            "red",
            "one side that extracted nothing is refused too",
            run.returncode == 1 and "not measured" in run.stdout,
            f"exit {run.returncode}\n{run.stdout}{run.stderr}",
        )

    # What separates this repair from a rename of the old sentinel: the absence
    # of an extraction is distinguished from a measure that is legitimately
    # zero. `cargo test` had no sentinel at all, and answered `0 bin, 0 tst`.
    nothing = worktree_verdicts.measure_cargo_test("", 101)
    ran = worktree_verdicts.measure_cargo_test(
        "test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n", 0
    )
    all_ignored = worktree_verdicts.measure_cargo_test(
        "test result: ok. 0 passed; 0 failed; 5 ignored; 0 measured; 0 filtered out\n", 0
    )
    pair.case(
        "control",
        "a run with no summary line measures nothing, a run reporting zero measures zero",
        nothing["binaries"] is None
        and nothing["tests"] is None
        and ran["binaries"] == 1
        and ran["tests"] == 3
        and all_ignored["binaries"] == 1
        and all_ignored["tests"] == 0,
        f"no summary {nothing}, one summary {ran}, all ignored {all_ignored}",
    )


# ── Site 3, the descriptions `just --list` renders ───────────────────────────

RECIPE_NAME = re.compile(r"^@?([A-Za-z_][A-Za-z0-9_-]*)")


def _recipe_line(line: str) -> str | None:
    """The name a recipe definition line declares, or None.

    A recipe line carries a `:` that is not the `:=` of an assignment. Quoted
    parameter defaults may hold either character, so the scan tracks quotes:
    `linux-check arch="x86":` is a recipe, `llama_port := "8899"` is not.
    """
    if not line or line[0].isspace() or line.startswith("#"):
        return None
    match = RECIPE_NAME.match(line)
    if match is None:
        return None
    rest = line[match.end() :]
    quote = None
    for index, char in enumerate(rest):
        if quote is not None:
            if char == quote:
                quote = None
            continue
        if char in "\"'":
            quote = char
            continue
        if char == ":":
            return None if rest[index : index + 2] == ":=" else match.group(1)
    return None


def _rendered_docs(justfile: Path) -> dict[str, str] | None:
    """What `just` itself says each recipe does, or None when it refuses."""
    run = subprocess.run(
        [
            "just",
            "--justfile",
            str(justfile),
            "--working-directory",
            str(justfile.parent),
            "--dump",
            "--dump-format",
            "json",
        ],
        capture_output=True,
        text=True,
    )
    if run.returncode != 0:
        return None
    recipes = json.loads(run.stdout)["recipes"]
    return {name: rec.get("doc") or "" for name, rec in recipes.items()}


def overlong_blocks(justfile: Path) -> list[tuple[str, str, str]] | None:
    """Recipes whose attached comment block does not fit on a single line.

    `just` renders the last line of the attached block as the description, so a
    block of several lines advertises its own last line. Returns the recipe
    name, what is rendered, and the first line of the block, which is what the
    reader expected to see.
    """
    docs = _rendered_docs(justfile)
    if docs is None:
        return None
    lines = justfile.read_text(encoding="utf-8").splitlines()
    offenders: list[tuple[str, str, str]] = []
    for index, line in enumerate(lines):
        name = _recipe_line(line)
        if name is None or name not in docs:
            continue
        block: list[str] = []
        cursor = index - 1
        while cursor >= 0 and lines[cursor].lstrip().startswith("#"):
            block.insert(0, lines[cursor].lstrip().lstrip("#").strip())
            cursor -= 1
        if len(block) > 1:
            offenders.append((name, docs[name], block[0]))
    return offenders


FAULTY_JUSTFILE = """\
# Run alpha against the tree.
# Usage: just alpha /path/to/thing
alpha:
    @echo alpha

# Run beta against the tree.
beta:
    @echo beta
"""


def check_recipe_descriptions(pair: Pair) -> None:
    announce(pair)
    if shutil.which("just") is None:
        pair.zero_coverage(
            "just is not on PATH, so nothing could render a description. "
            "Install it with: cargo install just"
        )
        return
    justfile = REPO_ROOT / "justfile"
    if not justfile.exists():
        pair.zero_coverage(f"no justfile at {REPO_ROOT}, so this pair examined nothing")
        return

    offenders = overlong_blocks(justfile)
    if offenders is None:
        pair.zero_coverage(
            "just refused to dump this justfile, so the green direction "
            "examined nothing"
        )
        return
    pair.case(
        "green",
        "every recipe of this tree advertises its whole comment block",
        not offenders,
        "these recipes render their last comment line instead of their purpose: "
        + ", ".join(f"{n} renders {r!r}, block starts {f!r}" for n, r, f in offenders),
    )

    with tempfile.TemporaryDirectory() as tmp:
        fixture = Path(tmp) / "justfile"
        fixture.write_text(FAULTY_JUSTFILE, encoding="utf-8")
        found = overlong_blocks(fixture)
        if found is None:
            pair.zero_coverage(
                "just refused to dump the fixture, so the red direction "
                "examined nothing"
            )
            return
        names = [name for name, _, _ in found]
        pair.case(
            "red",
            "a recipe whose block ends on a usage example is reported",
            names == ["alpha"]
            and found[0][1] == "Usage: just alpha /path/to/thing"
            and found[0][2] == "Run alpha against the tree.",
            f"reported {found}, expected alpha with its rendered line and its "
            f"first line",
        )
        pair.case(
            "control",
            "a recipe whose block is one line is not reported",
            "beta" not in names,
            f"beta was reported although its block is a single line: {found}",
        )


# ── Site 4, the format guard ─────────────────────────────────────────────────

# `cargo fmt` without `--all` needs a current package. Spelled as a class so a
# sweep over this tree does not match the pattern's own text.
FMT_INVOCATION = re.compile(r"cargo\s+fm[t]\b")

WORKSPACE_MANIFEST = '[workspace]\nresolver = "2"\nmembers = ["alpha", "beta"]\n'
MEMBER_MANIFEST = '[package]\nname = "{name}"\nversion = "0.1.0"\nedition = "2021"\n'
WELL_FORMATTED = "pub fn value() -> u8 {\n    1\n}\n"
BADLY_FORMATTED = "pub fn value()->u8{1}\n"


def fmt_check_without_all(line: str) -> bool:
    """True when a line declares the format check in its directory-dependent form.

    `--all --check` and `--all -- --check` are both accepted: what the flag has
    to do is take the workspace out of the caller's current directory, and
    either spelling does.
    """
    if FMT_INVOCATION.search(line) is None:
        return False
    if "--check" not in line:
        return False
    return "--all" not in line


def _tracked_files() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "-z"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return [path for path in out.split("\0") if path]


def _cargo_fmt(cwd: Path, *flags: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["cargo", "fmt", *flags], cwd=cwd, capture_output=True, text=True
    )


def check_format_guard(pair: Pair) -> None:
    announce(pair)

    if shutil.which("cargo") is None:
        pair.zero_coverage(
            "cargo is not on PATH, so the tool half of this pair examined nothing"
        )
    else:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp).resolve()
            (root / "Cargo.toml").write_text(WORKSPACE_MANIFEST, encoding="utf-8")
            for member in ("alpha", "beta"):
                src = root / member / "src"
                src.mkdir(parents=True)
                (root / member / "Cargo.toml").write_text(
                    MEMBER_MANIFEST.format(name=member), encoding="utf-8"
                )
                (src / "lib.rs").write_text(WELL_FORMATTED, encoding="utf-8")
            # Two members, because the cadrage measured that a single-member
            # workspace does not reproduce the defect.
            elsewhere = root / "notes"
            elsewhere.mkdir()

            clean_all = _cargo_fmt(elsewhere, "--all", "--check")
            pair.case(
                "green",
                "a well formatted workspace answers 0 from a directory holding no manifest",
                clean_all.returncode == 0,
                f"exit {clean_all.returncode}\n{clean_all.stdout}{clean_all.stderr}",
            )

            (root / "beta/src/lib.rs").write_text(BADLY_FORMATTED, encoding="utf-8")
            faulty_all = _cargo_fmt(elsewhere, "--all", "--check")
            pair.case(
                "red",
                "one badly formatted member answers 1 from the same directory",
                faulty_all.returncode == 1,
                f"exit {faulty_all.returncode}\n{faulty_all.stdout}{faulty_all.stderr}",
            )

            faulty_plain = _cargo_fmt(elsewhere, "--check")
            (root / "beta/src/lib.rs").write_text(WELL_FORMATTED, encoding="utf-8")
            clean_plain = _cargo_fmt(elsewhere, "--check")
            pair.case(
                "control",
                "without --all the same command answers 1 in both directions",
                clean_plain.returncode == 1 and faulty_plain.returncode == 1,
                f"well formatted exit {clean_plain.returncode}, badly formatted "
                f"exit {faulty_plain.returncode}. Both should be 1, which is what "
                f"makes the flag a repair rather than a preference.\n"
                f"{clean_plain.stdout}{clean_plain.stderr}",
            )

    declarations: list[str] = []
    for relative in _tracked_files():
        if relative == SELF:
            continue
        path = REPO_ROOT / relative
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        for number, line in enumerate(text.splitlines(), start=1):
            if fmt_check_without_all(line):
                declarations.append(f"{relative}:{number}: {line.strip()}")
    pair.case(
        "green",
        f"no tracked file declares the check without --all (this file excepted, {SELF})",
        not declarations,
        "these declarations answer 1 from any directory holding no manifest:\n  "
        + "\n  ".join(declarations),
    )
    pair.case(
        "red",
        "the same detector fires on a declaration that carries the faulty form",
        fmt_check_without_all("        entry: cargo fmt --check"),
        "the detector stayed silent on the form the pre-commit hook used to "
        "carry, so its silence on the tracked files says nothing",
    )
    pair.case(
        "control",
        "the detector accepts both spellings of the repaired form",
        not fmt_check_without_all("cargo fmt --all --check")
        and not fmt_check_without_all("      - run: cargo fmt --all -- --check"),
        "the detector fires on a repaired declaration, so the green direction "
        "above is unreachable",
    )


# ── Site 5, the end-to-end suite ─────────────────────────────────────────────


def _suite(binary: str, report_dir: Path) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["bash", "tests/cli/cli-e2e.sh"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        env={
            **os.environ,
            "APOLLIA_BIN": binary,
            # The report goes to a temporary directory in both directions: a
            # proof never writes into tests/cli/report/ of the real tree.
            "APOLLIA_E2E_REPORT_DIR": str(report_dir),
        },
    )


def check_cli_suite(pair: Pair) -> None:
    announce(pair)

    relative = next(
        (
            candidate
            for candidate in ("target/release/apollia-os", "target/debug/apollia-os")
            if (REPO_ROOT / candidate).is_file()
        ),
        None,
    )
    if relative is None:
        pair.zero_coverage(
            "no apollia-os binary in this tree, so the green direction cannot "
            "run a single assertion. Run: bash scripts/worktree-prep.sh rust"
        )
        return
    if not (REPO_ROOT / "target/Resources/python").exists():
        pair.zero_coverage(
            "target/Resources/python is missing, so every invocation of the CLI "
            "binary dies before main() and the green direction would measure "
            "the absent link rather than the suite. Run: bash "
            "scripts/worktree-prep.sh rust"
        )
        return
    print(f"        subject: {relative}, run from {REPO_ROOT}")

    with tempfile.TemporaryDirectory() as tmp:
        run = _suite(relative, Path(tmp))
        plain = ANSI.sub("", run.stdout)
        # The single assertion that changes directory before invoking the
        # binary, named rather than counted: it is the one a relative path used
        # to break, and a count of 154 could be reached without it.
        scratch_cwd = re.search(
            r"^\s*✔\s+workspace init --force \(scratch cwd\)\s*$", plain, re.M
        )
        pair.case(
            "green",
            "a relative APOLLIA_BIN runs every assertion, including the one that changes directory",
            run.returncode == 0
            and re.search(r"^\s*PASS\s*:\s*154\s*$", plain, re.M) is not None
            and re.search(r"^\s*FAIL\s*:\s*0\s*$", plain, re.M) is not None
            and scratch_cwd is not None,
            f"exit {run.returncode}, the scratch-cwd assertion passed "
            f"{scratch_cwd is not None}. Tail:\n"
            + "\n".join(plain.splitlines()[-25:]),
        )

    with tempfile.TemporaryDirectory() as tmp:
        missing = "target/debug/apollia-os-that-does-not-exist"
        run = _suite(missing, Path(tmp))
        combined = run.stdout + run.stderr
        pair.case(
            "red",
            "an APOLLIA_BIN that resolves to nothing refuses with 2 and names the resolved path",
            run.returncode == 2
            and str(REPO_ROOT / missing) in combined
            and re.search(r"^\s*PASS\s*:", run.stdout, re.M) is None,
            f"exit {run.returncode}, path named "
            f"{str(REPO_ROOT / missing) in combined}. Expected exit 2, True and "
            f"no assertion count.\n{combined}",
        )


# ── The count ────────────────────────────────────────────────────────────────

PAIRS = [
    (
        Pair("just _bundle-python", "is the Python bundle link laid down?"),
        check_bundle_python,
    ),
    (
        Pair(
            "scripts/worktree_verdicts.py --compare",
            "do the two trees answer the same verdict?",
        ),
        check_worktree_comparator,
    ),
    (Pair("just --list", "what does this recipe do?"), check_recipe_descriptions),
    (Pair("cargo fmt --check", "is the tree well formatted?"), check_format_guard),
    (
        Pair("tests/cli/cli-e2e.sh", "does the command line behave?"),
        check_cli_suite,
    ),
]


def main() -> int:
    for pair, check in PAIRS:
        check(pair)
        print()

    pairs = [pair for pair, _ in PAIRS]
    failures = [entry for pair in pairs for entry in pair.failures]
    uncovered = [entry for pair in pairs for entry in pair.uncovered]
    incomplete = [
        f"{pair.name}: missing the {' and '.join(pair.missing_directions())} direction"
        for pair in pairs
        if not pair.uncovered and pair.missing_directions()
    ]

    holding = [pair for pair in pairs if pair.holds()]
    falsifiable = [pair for pair in holding if pair.state == "falsifiable"]
    declared_silent = [pair for pair in holding if pair.state == "declared silent"]

    if uncovered:
        print(f"{len(uncovered)} pair(s) examined nothing:\n", file=sys.stderr)
        for entry in uncovered:
            print(f"  {entry}\n", file=sys.stderr)
    if incomplete:
        print(f"{len(incomplete)} pair(s) ran a single direction:\n", file=sys.stderr)
        for entry in incomplete:
            print(f"  {entry}\n", file=sys.stderr)
    if failures:
        print(f"{len(failures)} instrument verdict failure(s):\n", file=sys.stderr)
        for entry in failures:
            print(f"  {entry}\n", file=sys.stderr)

    total = len(falsifiable) + len(declared_silent)
    print(
        f"{len(PAIRS)} instruments: {len(falsifiable)} falsifiable, "
        f"{len(declared_silent)} declared silent"
    )
    if total != len(PAIRS):
        print(
            f"{total} instrument(s) reached a terminal state out of {len(PAIRS)}.",
            file=sys.stderr,
        )
        return 1
    if uncovered or incomplete or failures:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
