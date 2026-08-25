#!/usr/bin/env python3
"""Record the verdict of the expensive guards, and compare two trees.

A guard declared "local" is only local if it answers the same thing wherever a
lot is realised. Eight of them did not. Measured on 6a3f59de, in a worktree
created by `git worktree add --detach`, against the same commit in the main tree:

    cargo check --workspace --all-targets     101 against 0
    cargo clippy --workspace --all-targets    101 against 0
    cargo test --workspace --no-fail-fast     0 test binaries against 80
    bash tests/cli/cli-e2e.sh                 PASS 1 FAIL 153 against PASS 154
    npm run build (ui)                        127 against 0
    npx svelte-check --threshold error        2050 errors over 853 files
                                              against 1 over 4943
    npx vitest run                            no vitest against 790 tests
    cd docs/site && npm run build             127 against 0

The `svelte-check` line is the one that decides the shape of this tool. Its exit
code is 1 on both sides, so a comparison of exit codes alone calls those two
verdicts equal, and a reader who trusts it goes on to believe a tree with 2050
fabricated errors is the tree the main one measures. What separates them is the
characteristic measure, `FILES` and `ERRORS`, so that is what is recorded and
compared, guard by guard.

`cargo test` is the other special case, and its measure is deliberately not its
exit code. A fresh worktree exits 101 in ten seconds without having compiled
anything, so no summary line is printed at all and nothing is measured; a
complete tree exits 101 in ninety nine seconds having run 4370 tests across 80
binaries, because one test is non-deterministic. A summary line reporting zero
executed tests is a third thing again, a tree whose tests are all `#[ignore]`,
and it is a measure. Comparing exit codes would call those equal, and comparing
the set of failed tests would make this tool hostage to that flaky test for a
cause that has nothing to do with the worktree. The number of test binaries and
the number of tests executed separate "died at the build" from "ran, and one
test flinched", which is the distinction that matters here.

Preconditions are probed on the spot rather than read from a state file, because
a state file believes and a probe measures. A guard whose precondition is
missing is recorded as `not prepared`, never as a verdict: a wrong verdict is
worse than an absent one, which is the whole subject of this tool.

A measure has a third state, for the same reason. A guard that ran but whose
characteristic measure could not be extracted records None on that key and
reads `not measured` in the table, where the reader looks for its value. Two
trees that measured nothing are not two trees that agree, and the comparison
tests that state before it tests equality.

One guard, `desktop-automation`, is exempt when it is not prepared: preparing
it means running the real desktop app through the gestural corpus, a manual
act (no hosted runner drives WKWebView), so demanding it of every comparison
would make the comparison unusable. The exemption is printed by name, never
silent, and the moment both records carry a fresh report its measures are
compared like any other guard's.

The `linux-test` line is the local mirror of the `Rust Tests` (Linux) job of
ci.yml, which does not run (billing). It is recorded like the others, and it
is the one line exempt from the comparison outright: it needs a Docker daemon
and a container build, so on a machine where the daemon is absent or silent
linux-check.sh answers 2 with nothing measured, and counting that absence as
a parity gap would fail every comparison for a cause foreign to the two trees.
The exemption is named on the guard itself and bounded by the selftest.

Usage:
    python3 scripts/worktree_verdicts.py --record <output.json>
    python3 scripts/worktree_verdicts.py --compare <main.json> <worktree.json>

Exit codes:
    --record    0 once the lines are recorded, whatever they say. It is a
                measurement, not a verdict.
    --compare   0 when every compared guard matches, 1 on a difference, on a
                non-exempt guard recorded as not prepared, or on a measure
                that was never extracted, 2 when it refuses to compare at all
                (the two records are not on the same commit).
"""

import argparse
import json
import re
import subprocess
import sys
import time
from pathlib import Path

ANSI = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")

UI = "crates/apollia-desktop/ui"
SITE = "docs/site"


def strip(text: str) -> str:
    return ANSI.sub("", text)


def measure_exit(_: str, code: int) -> dict[str, int | None]:
    return {"exit": code}


# One summary line per test binary and per doc-test pass. The `ok.`/`FAILED.`
# part is required: three test functions of this workspace live in a module
# named `result`, so their per-test lines read `test result::tests::...` and a
# bare `^test result:` prefix counts them as three extra binaries.
TEST_SUMMARY = re.compile(
    r"^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored", re.M
)


# Appended by scripts/check_test_home_isolation.py --wrap: the verdict of the
# sentinel HOME the wrapped command ran under. CLEAN is zero changes.
HOME_SENTINEL = re.compile(r"^HOME-SENTINEL: (?:CLEAN|CHANGED (\d+))$", re.M)


def measure_cargo_test(output: str, code: int) -> dict[str, int | None]:
    """Test binaries that reported a result, tests they executed, and the
    home-sentinel verdict.

    Ignored tests are counted out: they are compiled and skipped, so counting
    them would let a `#[ignore]` added on one side hide a test that stopped
    running on the other.

    No summary line at all means nothing was measured, and that is not the same
    as a tree whose tests are all `#[ignore]`: the second one reported, and it
    reported zero. `summaries` empty is the discriminant, not the total.

    `home_changes` counts what the run wrote into the sentinel `~/.apollia`
    the wrapper pointed HOME at; an absent verdict line records None, which
    reads `not measured`, because a run that never watched the sentinel is not
    a run that left it untouched.
    """
    summaries = TEST_SUMMARY.findall(output)
    sentinel = HOME_SENTINEL.search(output)
    home_changes = None if sentinel is None else int(sentinel.group(1) or 0)
    if not summaries:
        return {
            "exit": code,
            "binaries": None,
            "tests": None,
            "home_changes": home_changes,
        }
    executed = sum(int(passed) + int(failed) for passed, failed, _ in summaries)
    return {
        "exit": code,
        "binaries": len(summaries),
        "tests": executed,
        "home_changes": home_changes,
    }


def measure_linux_test(output: str, code: int) -> dict[str, int | None]:
    """The cargo test summaries of a containerised Linux run.

    Same summary-line reading as `measure_cargo_test`, without the home
    sentinel: the container mounts the tree read-only and never sees the
    operator's home, so a sentinel line cannot appear and demanding one would
    read every run as `not measured`. linux-check.sh exits 2 when nothing was
    measured (no docker, daemon silent, image or container failure); no
    summary line is printed on that path, so both counts stay None and the
    row reads `not measured` rather than passing for a verdict.
    """
    summaries = TEST_SUMMARY.findall(output)
    if not summaries:
        return {"exit": code, "binaries": None, "tests": None}
    executed = sum(int(passed) + int(failed) for passed, failed, _ in summaries)
    return {"exit": code, "binaries": len(summaries), "tests": executed}


def measure_cli_e2e(output: str, code: int) -> dict[str, int | None]:
    passed = re.search(r"^\s*PASS\s*:\s*(\d+)\s*$", output, re.M)
    failed = re.search(r"^\s*FAIL\s*:\s*(\d+)\s*$", output, re.M)
    return {
        "exit": code,
        "pass": int(passed.group(1)) if passed else None,
        "fail": int(failed.group(1)) if failed else None,
    }


def measure_svelte_check(output: str, code: int) -> dict[str, int | None]:
    done = re.search(r"COMPLETED (\d+) FILES (\d+) ERRORS", output)
    return {
        "exit": code,
        "files": int(done.group(1)) if done else None,
        "errors": int(done.group(2)) if done else None,
    }


def measure_vitest(output: str, code: int) -> dict[str, int | None]:
    total = re.search(r"^\s*Tests\s+.*\((\d+)\)\s*$", output, re.M)
    return {"exit": code, "tests": int(total.group(1)) if total else None}


# check_automation_report.py: "ok: <script>: N steps, M failed, ..." on green,
# "RED: <script>: ok=False, M failed step(s) out of N, ..." on red.
AUTOMATION_GREEN = re.compile(r"^ok: .*: (\d+) steps, (\d+) failed", re.M)
AUTOMATION_RED = re.compile(
    r"^RED: .*: ok=\w+, (\d+) failed step\(s\) out of (\d+)", re.M
)


def measure_automation(output: str, code: int) -> dict[str, int | None]:
    green = AUTOMATION_GREEN.search(output)
    if green:
        return {
            "exit": code,
            "steps": int(green.group(1)),
            "failed": int(green.group(2)),
        }
    red = AUTOMATION_RED.search(output)
    if red:
        return {"exit": code, "steps": int(red.group(2)), "failed": int(red.group(1))}
    return {"exit": code, "steps": None, "failed": None}


class Probe:
    """A precondition of one guard, checked on the spot rather than believed.

    `paths` are alternatives: the probe is satisfied when any one of them
    exists. tests/cli/cli-e2e.sh resolves a release binary then a debug one, so
    either answers its need.
    """

    def __init__(self, paths: tuple[str, ...], reason: str) -> None:
        self.paths = paths
        self.reason = reason

    def satisfied(self, tree: Path) -> bool:
        return any((tree / p).exists() for p in self.paths)


class Guard:
    def __init__(
        self,
        key: str,
        label: str,
        command: list[str],
        cwd: str,
        probes: tuple[Probe, ...],
        measure,
        compared: tuple[str, ...],
        render,
        *,
        exempt_when_unprepared: str | None = None,
        exempt: str | None = None,
    ) -> None:
        self.key = key
        self.label = label
        self.command = command
        self.cwd = cwd
        self.probes = probes
        self.measure = measure
        self.compared = compared
        self.render = render
        # A guard whose preparation is a manual act (a run of the real desktop
        # app) cannot be demanded of every comparison: exempting its
        # "not prepared" state keeps the comparison usable while still
        # comparing the measures whenever both trees carry one. The reason is
        # printed so the exemption never reads as agreement.
        self.exempt_when_unprepared = exempt_when_unprepared
        # A non-None value takes this guard out of the two-tree comparison and
        # says why. It is still recorded; its row still prints, marked `~`.
        self.exempt = exempt

    def missing(self, tree: Path) -> Probe | None:
        for probe in self.probes:
            if not probe.satisfied(tree):
                return probe
        return None


def render_exit(m: dict[str, int | None]) -> str:
    return f"exit {m['exit']}"


def render_cargo_test(m: dict[str, int | None]) -> str:
    # `.get`: a record written by the pre-sentinel version of this script has
    # no `home_changes` key, and rendering it must not crash the comparison.
    return f"exit {m['exit']}, {m['binaries']} bin, {m['tests']} tst, home {m.get('home_changes')}"


def render_linux_test(m: dict[str, int | None]) -> str:
    return f"exit {m['exit']}, {m['binaries']} bin, {m['tests']} tst"


def render_cli_e2e(m: dict[str, int | None]) -> str:
    return f"exit {m['exit']}, PASS {m['pass']}, FAIL {m['fail']}"


def render_svelte_check(m: dict[str, int | None]) -> str:
    return f"exit {m['exit']}, {m['files']} FILES, {m['errors']} ERR"


def render_vitest(m: dict[str, int | None]) -> str:
    return f"exit {m['exit']}, {m['tests']} tests"


def render_automation(m: dict[str, int | None]) -> str:
    return f"exit {m['exit']}, {m['steps']} steps, {m['failed']} failed"


# The eight guards of the acceptance criterion, in the order the criterion
# lists them, plus the desktop-automation reader (exempt when unprepared, see
# the module docstring). `probe` is the path whose absence makes the line
# unmeasurable in this tree; `compared` names the measure keys that enter the
# comparison, which is
# how `cargo test` keeps its exit code visible without letting a flaky test
# decide the verdict.
GUARDS = [
    Guard(
        "cargo-check",
        "cargo check --workspace --all-targets",
        ["cargo", "check", "--workspace", "--all-targets"],
        ".",
        (),
        measure_exit,
        ("exit",),
        render_exit,
    ),
    Guard(
        "cargo-clippy",
        "cargo clippy --workspace --all-targets",
        ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
        ".",
        (),
        measure_exit,
        ("exit",),
        render_exit,
    ),
    Guard(
        "cargo-test",
        "cargo test --workspace --no-fail-fast (sentinel HOME)",
        # The envelope points HOME at a seeded sentinel and appends the
        # HOME-SENTINEL verdict line this file measures as `home_changes`:
        # a test that writes into the operator's real ~/.apollia is a
        # difference between trees whatever the test summary says.
        [
            "python3",
            "scripts/check_test_home_isolation.py",
            "--wrap",
            "cargo",
            "test",
            "--workspace",
            "--no-fail-fast",
        ],
        ".",
        (),
        measure_cargo_test,
        ("binaries", "tests", "home_changes"),
        render_cargo_test,
    ),
    Guard(
        "cli-e2e",
        "bash tests/cli/cli-e2e.sh",
        ["bash", "tests/cli/cli-e2e.sh"],
        ".",
        (
            Probe(
                ("target/Resources/python",),
                "the Python bundle link is missing, so every invocation of the "
                "CLI binary dies before main() and the suite reports 153 "
                "failures out of 154 cases. Run: bash scripts/worktree-prep.sh rust",
            ),
            Probe(
                ("target/release/apollia-os", "target/debug/apollia-os"),
                "no apollia-os binary. The suite builds nothing itself and "
                "exits 2 without running a single case. Run: bash "
                "scripts/worktree-prep.sh rust",
            ),
        ),
        measure_cli_e2e,
        ("exit", "pass", "fail"),
        render_cli_e2e,
    ),
    Guard(
        "ui-build",
        "npm run build (ui)",
        ["npm", "run", "build"],
        UI,
        (
            Probe(
                (f"{UI}/node_modules/.bin/vite",),
                "vite is not installed. Run: bash scripts/worktree-prep.sh ui",
            ),
        ),
        measure_exit,
        ("exit",),
        render_exit,
    ),
    Guard(
        "svelte-check",
        "npx svelte-check --threshold error",
        ["npx", "svelte-check", "--threshold", "error"],
        UI,
        (
            Probe(
                (f"{UI}/node_modules/.bin/svelte-check",),
                "svelte-check is not installed, and without it this line "
                "reports 2050 errors over 853 files instead of the verdict of "
                "the repository. Run: bash scripts/worktree-prep.sh ui",
            ),
        ),
        measure_svelte_check,
        ("exit", "files", "errors"),
        render_svelte_check,
    ),
    Guard(
        "vitest",
        "npx vitest run",
        ["npx", "vitest", "run"],
        UI,
        (
            Probe(
                (f"{UI}/node_modules/.bin/vitest",),
                "vitest is not installed. Run: bash scripts/worktree-prep.sh ui",
            ),
        ),
        measure_vitest,
        ("exit", "tests"),
        render_vitest,
    ),
    Guard(
        "docs-build",
        "cd docs/site && npm run build",
        ["npm", "run", "build"],
        SITE,
        (
            Probe(
                (f"{SITE}/node_modules/.bin/docusaurus",),
                "docusaurus is not installed. Run: bash scripts/worktree-prep.sh docs",
            ),
        ),
        measure_exit,
        ("exit",),
        render_exit,
    ),
    # The ninth guard is exempt when unprepared: its preparation is a run of
    # the real desktop app through the 2400-step gestural corpus, a manual act
    # no hosted runner can perform (there is no WebDriver for WKWebView), so a
    # comparison must stay usable without it. When both trees carry a fresh
    # report the measures are compared like any other guard's.
    Guard(
        "desktop-automation",
        "python3 scripts/check_automation_report.py",
        ["python3", "scripts/check_automation_report.py"],
        ".",
        (
            Probe(
                (".apollia-automation/report.json",),
                "no automation report. The gestural corpus renders a runtime "
                "verdict only after a run of the real app: just "
                "desktop-dev-automation-seeded scripts/automation/master-det.json",
            ),
        ),
        measure_automation,
        ("exit", "steps", "failed"),
        render_automation,
        exempt_when_unprepared=(
            "preparing it is a manual run of the real desktop app; its "
            "absence states nothing and does not decide this comparison"
        ),
    ),
    # The tenth line, added after the last CI run that executed answered red on
    # six tools::python_executor tests on Linux while every local guard stayed
    # green on macOS. It is the local frontier for the `Rust Tests` (Linux) job
    # of ci.yml, which does not run (billing). Heavy on purpose: a full
    # workspace test build inside the container. linux-check.sh answers 2
    # without measuring when the daemon is missing; the 60-second bound on a
    # silent daemon belongs to the script itself.
    Guard(
        "linux-test",
        "bash scripts/linux-check.sh arm test (Docker)",
        ["bash", "scripts/linux-check.sh", "arm", "test"],
        ".",
        (),
        measure_linux_test,
        ("exit", "binaries", "tests"),
        render_linux_test,
        exempt=(
            "needs a Docker daemon and a container build: on a machine where "
            "the daemon is absent or silent the run measures nothing (exit 2), "
            "and counting that absence as a parity gap would fail every "
            "comparison for a cause foreign to the two trees"
        ),
    ),
]


# ── Recording ────────────────────────────────────────────────────────────────


def git(tree: Path, *args: str) -> str:
    return subprocess.run(
        ["git", *args], cwd=tree, capture_output=True, text=True, check=True
    ).stdout.strip()


def python_bundle(tree: Path) -> str | None:
    """The bundle the CLI binary will resolve at run time, or None."""
    link = tree / "target/Resources/python"
    if not link.exists():
        return None
    return str(link.resolve())


def record(tree: Path, out: Path) -> int:
    porcelain = git(tree, "status", "--porcelain")
    data: dict[str, object] = {
        "tree": str(tree),
        "head": git(tree, "rev-parse", "HEAD"),
        "porcelain_lines": len([ln for ln in porcelain.splitlines() if ln]),
        "python_bundle": python_bundle(tree),
        "guards": {},
    }
    guards: dict[str, object] = data["guards"]  # type: ignore[assignment]

    for guard in GUARDS:
        missing = guard.missing(tree)
        if missing is not None:
            print(f"  not prepared  {guard.label}")
            print(f"                {missing.reason}")
            guards[guard.key] = {
                "prepared": False,
                "reason": missing.reason,
                "probe": list(missing.paths),
            }
            continue

        print(f"  running       {guard.label}", flush=True)
        started = time.monotonic()
        run = subprocess.run(
            guard.command,
            cwd=tree / guard.cwd,
            capture_output=True,
            text=True,
        )
        elapsed = round(time.monotonic() - started, 1)
        output = strip(run.stdout + run.stderr)
        measures = guard.measure(output, run.returncode)
        guards[guard.key] = {
            "prepared": True,
            "measures": measures,
            "seconds": elapsed,
        }
        print(f"                {guard.render(measures)}   {elapsed}s", flush=True)

    out.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    print(f"\nrecorded {len(GUARDS)} guards to {out}")
    return 0


# ── Comparing ────────────────────────────────────────────────────────────────


def load(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def unmeasured(entry: dict) -> bool:
    """True when the guard ran but one of its measures was never extracted.

    The key stays in the record and carries None. Dropping it instead would
    make two records that measured nothing compare equal, since both sides
    would answer None to `.get`, which is the defect this state exists to end.
    """
    return any(value is None for value in entry.get("measures", {}).values())


def cell(entry: dict, guard: Guard) -> str:
    if not entry.get("prepared", False):
        return "not prepared"
    if unmeasured(entry):
        return "not measured"
    return guard.render(entry["measures"])


def compare(left_path: Path, right_path: Path) -> int:
    left = load(left_path)
    right = load(right_path)

    if left["head"] != right["head"]:
        print(
            "refusing to compare: the two records are not on the same commit.\n"
            f"  {left['tree']}  HEAD {left['head']}\n"
            f"  {right['tree']}  HEAD {right['head']}\n"
            "The criterion is that a prepared worktree answers the verdict of "
            "the main tree on the same commit. Two commits make any difference "
            "unattributable.",
            file=sys.stderr,
        )
        return 2

    print(f"Main tree : {left['tree']}")
    print(
        f"            HEAD {left['head'][:8]}   porcelain {left['porcelain_lines']} line(s)"
    )
    print(f"            Python bundle {left['python_bundle']}")
    print(f"Worktree  : {right['tree']}")
    print(
        f"            HEAD {right['head'][:8]}   porcelain {right['porcelain_lines']} line(s)"
    )
    print(f"            Python bundle {right['python_bundle']}")
    print()

    width = max(len(g.label) for g in GUARDS)
    header = f"| {'Guard'.ljust(width)} | {'Main tree'.ljust(26)} | {'Worktree'.ljust(26)} |   |"
    print(header)
    print(f"|{'-' * (width + 2)}|{'-' * 28}|{'-' * 28}|---|")

    gaps: list[str] = []
    unprepared: list[str] = []
    unextracted: list[str] = []
    exempted: list[tuple[str, str]] = []
    outside: list[Guard] = []

    for guard in GUARDS:
        left_entry = left["guards"].get(guard.key, {})
        right_entry = right["guards"].get(guard.key, {})
        left_cell = cell(left_entry, guard)
        right_cell = cell(right_entry, guard)

        if guard.exempt is not None:
            # Printed, never counted: the reason is on the guard and listed
            # below the table, so the exemption stays visible on every run.
            outside.append(guard)
            mark = "~"
            print(
                f"| {guard.label.ljust(width)} | {left_cell.ljust(26)} "
                f"| {right_cell.ljust(26)} | {mark} |"
            )
            continue
        if not left_entry.get("prepared", False) or not right_entry.get(
            "prepared", False
        ):
            if guard.exempt_when_unprepared is not None:
                mark = "-"
                exempted.append((guard.label, guard.exempt_when_unprepared))
                print(
                    f"| {guard.label.ljust(width)} | {left_cell.ljust(26)} "
                    f"| {right_cell.ljust(26)} | {mark} |"
                )
                continue
            mark = "?"
            unprepared.append(guard.label)
        elif unmeasured(left_entry) or unmeasured(right_entry):
            # Tested before equality, and deliberately so: two records that
            # extracted nothing would otherwise compare equal and read as
            # conformity.
            mark = "?"
            unextracted.append(guard.label)
        else:
            same = all(
                left_entry["measures"].get(k) == right_entry["measures"].get(k)
                for k in guard.compared
            )
            mark = "=" if same else "X"
            if not same:
                differing = [
                    f"{k} {left_entry['measures'].get(k)} against "
                    f"{right_entry['measures'].get(k)}"
                    for k in guard.compared
                    if left_entry["measures"].get(k) != right_entry["measures"].get(k)
                ]
                gaps.append(f"{guard.label}: " + ", ".join(differing))

        print(
            f"| {guard.label.ljust(width)} | {left_cell.ljust(26)} "
            f"| {right_cell.ljust(26)} | {mark} |"
        )

    print()
    if outside:
        print(f"{len(outside)} guard(s) recorded outside the comparison:")
        for guard in outside:
            print(f"  {guard.label}: {guard.exempt}")
    if exempted:
        for label, reason in exempted:
            print(f"exempt, not compared: {label} ({reason})")
    if unprepared:
        print(f"{len(unprepared)} guard(s) recorded as not prepared:", file=sys.stderr)
        for label in unprepared:
            print(f"  {label}", file=sys.stderr)
        print(
            "A guard that could not run states nothing about the two trees.",
            file=sys.stderr,
        )
    if unextracted:
        print(
            f"{len(unextracted)} guard(s) ran without their measure being extracted:",
            file=sys.stderr,
        )
        for label in unextracted:
            print(f"  {label}", file=sys.stderr)
        print(
            "A measure that was never extracted states nothing about the two "
            "trees either, and two absences are not an agreement.",
            file=sys.stderr,
        )
    if gaps:
        print(f"{len(gaps)} guard(s) answer a different verdict:", file=sys.stderr)
        for gap in gaps:
            print(f"  {gap}", file=sys.stderr)
    if unprepared or unextracted or gaps:
        return 1
    compared = len(GUARDS) - len(exempted) - len(outside)
    print(f"the {compared} compared guards answer the same verdict in both trees")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument(
        "--record", metavar="OUT", help="measure this tree into a JSON file"
    )
    group.add_argument(
        "--compare",
        nargs=2,
        metavar=("MAIN", "WORKTREE"),
        help="compare two records made on the same commit",
    )
    args = parser.parse_args()

    if args.record:
        tree = Path(
            subprocess.run(
                ["git", "rev-parse", "--show-toplevel"],
                capture_output=True,
                text=True,
                check=True,
            ).stdout.strip()
        )
        return record(tree, Path(args.record).resolve())
    return compare(Path(args.compare[0]), Path(args.compare[1]))


if __name__ == "__main__":
    sys.exit(main())
