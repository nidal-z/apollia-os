#!/usr/bin/env python3
"""Hold the GitHub workflows to the form the tree already promised.

The workflows were written job by job, and nothing ever read them as a set.
Four defects lived there at once, each invisible from inside the file that
carried it: nine actions pinned by a movable tag while the other hundred and
forty were pinned by SHA, two nightly jobs whose manual trigger tested a value
the dispatch list never offered, an auto-close policy that closed the
Dependabot PRs it was never aimed at, and three test suites forced onto one
thread, the exact remedy `docs/agents/TESTING.md` forbids because it hides
deadlocks instead of reporting them.

Rules, each measured on every `.yml` and `.yaml` under `.github/workflows/`:

  uses-sha        every `uses:` is pinned to a 40-character commit SHA
  needs-resolved  every `needs:` names a job that exists in the same file
  dispatch-list   a workflow declaring `workflow_dispatch.inputs.job` offers
                  every job id in its options, and every value a job's `if`
                  tests through `inputs.job` is offered too
  bot-filter      every job of `auto-close-prs.yml` excludes `dependabot[bot]`
                  and `github-actions[bot]` in its `if`
  advisory-name   a job with `continue-on-error: true` says `advisory` in its
                  name, so a red nobody blocks on is a red nobody misreads
  test-threads    no `run:` forces `--test-threads=1` (or `--test-threads 1`);
                  a suite that only passes serialised carries a bug, and the
                  flag files it away instead of reporting it

Exit codes, distinct on purpose so a run that measured nothing cannot pass for
a run that found nothing:

  0  every rule holds on at least one measured file
  1  at least one defect, each printed with file, job and rule
  2  nothing was measured: no workflow file, or PyYAML is absent

Usage:
    python3 scripts/check_ci_workflows.py
    python3 scripts/check_ci_workflows.py --selftest
"""

import argparse
import re
import sys
import tempfile
from pathlib import Path

try:
    import yaml
except ImportError:  # pragma: no cover - the environment, not the tree
    print(
        "check_ci_workflows: PyYAML is absent, so nothing was measured",
        file=sys.stderr,
    )
    sys.exit(2)

REPO_ROOT = Path(__file__).resolve().parents[1]
WORKFLOWS = REPO_ROOT / ".github" / "workflows"

SHA_PIN = re.compile(r"@[0-9a-f]{40}$")
TEST_THREADS = re.compile(r"--test-threads[= ]1\b")
INPUTS_JOB_EQ = re.compile(r"inputs\.job\s*==\s*'([^']+)'")

BOT_FILTER_FILE = "auto-close-prs.yml"
REQUIRED_BOT_EXCLUSIONS = ("dependabot[bot]", "github-actions[bot]")


def _jobs(doc: dict) -> dict:
    jobs = doc.get("jobs")
    return jobs if isinstance(jobs, dict) else {}


def _dispatch_options(doc: dict) -> list[str] | None:
    """The `workflow_dispatch.inputs.job` options, or None when undeclared."""
    on = doc.get("on") or doc.get(True)
    if not isinstance(on, dict):
        return None
    dispatch = on.get("workflow_dispatch")
    if not isinstance(dispatch, dict):
        return None
    inputs = dispatch.get("inputs") or {}
    job_input = inputs.get("job")
    if not isinstance(job_input, dict):
        return None
    options = job_input.get("options")
    return [str(option) for option in options] if isinstance(options, list) else None


def _steps(job: dict) -> list[dict]:
    steps = job.get("steps")
    if not isinstance(steps, list):
        return []
    return [step for step in steps if isinstance(step, dict)]


def audit_file(path: Path) -> tuple[list[str], int]:
    """Defects of one workflow file, and the number of `uses:` examined."""
    defects: list[str] = []
    try:
        doc = yaml.safe_load(path.read_text(encoding="utf-8"))
    except yaml.YAMLError as error:
        return [f"{path.name}: unparseable YAML ({error})"], 0
    if not isinstance(doc, dict):
        return [f"{path.name}: not a mapping, so it declares no job"], 0

    jobs = _jobs(doc)
    options = _dispatch_options(doc)
    uses_seen = 0

    for job_id, job in jobs.items():
        if not isinstance(job, dict):
            defects.append(f"{path.name}: job {job_id}: not a mapping")
            continue

        needs = job.get("needs", [])
        if isinstance(needs, str):
            needs = [needs]
        for needed in needs:
            if needed not in jobs:
                defects.append(
                    f"{path.name}: job {job_id}: [needs-resolved] needs "
                    f"{needed!r}, which no job of this file declares"
                )

        name = str(job.get("name", job_id))
        if job.get("continue-on-error") is True and "advisory" not in name.lower():
            defects.append(
                f"{path.name}: job {job_id}: [advisory-name] continue-on-error "
                f"without 'advisory' in its name {name!r}"
            )

        if options is not None:
            if job_id not in options:
                defects.append(
                    f"{path.name}: job {job_id}: [dispatch-list] absent from the "
                    f"workflow_dispatch job options, so it cannot be triggered "
                    f"alone"
                )
            for tested in INPUTS_JOB_EQ.findall(str(job.get("if", ""))):
                if tested not in options:
                    defects.append(
                        f"{path.name}: job {job_id}: [dispatch-list] tests "
                        f"inputs.job == {tested!r}, a value the options never "
                        f"offer"
                    )

        if path.name == BOT_FILTER_FILE:
            condition = str(job.get("if", ""))
            missing = [
                bot for bot in REQUIRED_BOT_EXCLUSIONS if bot not in condition
            ]
            if missing:
                defects.append(
                    f"{path.name}: job {job_id}: [bot-filter] its `if` does not "
                    f"exclude {', '.join(missing)}, so the policy closes PRs it "
                    f"was never aimed at"
                )

        for step in _steps(job):
            uses = step.get("uses")
            if uses is not None:
                uses_seen += 1
                if not SHA_PIN.search(str(uses)):
                    defects.append(
                        f"{path.name}: job {job_id}: [uses-sha] {uses!r} is "
                        f"pinned by a movable reference, not a 40-character SHA"
                    )
            run = step.get("run")
            if run is not None and TEST_THREADS.search(str(run)):
                defects.append(
                    f"{path.name}: job {job_id}: [test-threads] a `run:` forces "
                    f"--test-threads=1, which hides the bug a parallel run "
                    f"would report"
                )

    return defects, uses_seen


def audit(workflows_dir: Path) -> tuple[list[str], int, int]:
    """Defects, files measured, and `uses:` examined under one directory."""
    paths = sorted(workflows_dir.glob("*.yml")) + sorted(workflows_dir.glob("*.yaml"))
    defects: list[str] = []
    uses_total = 0
    for path in paths:
        file_defects, uses_seen = audit_file(path)
        defects.extend(file_defects)
        uses_total += uses_seen
    return defects, len(paths), uses_total


# ── Selftest ─────────────────────────────────────────────────────────────────
# One fabricated tree per direction. The faulty tree must trip every rule and
# the clean tree none, otherwise a green run on the real tree proves nothing.

FAULTY_NIGHTLY = """\
name: Fixture nightly
on:
  workflow_dispatch:
    inputs:
      job:
        type: choice
        options: ["all", "alpha"]
jobs:
  alpha:
    if: github.event.inputs.job == 'all' || github.event.inputs.job == 'alpha'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - run: cargo test -- --test-threads=1
  beta:
    name: Beta
    if: github.event.inputs.job == 'beta'
    needs: gamma
    continue-on-error: true
    runs-on: ubuntu-latest
    steps:
      - run: echo beta
"""

FAULTY_AUTOCLOSE = """\
name: Fixture auto-close
on:
  pull_request_target:
    types: [opened]
jobs:
  close:
    if: github.event.pull_request.user.login != github.repository_owner
    runs-on: ubuntu-latest
    steps:
      - run: echo close
"""

CLEAN_SHA = "0123456789abcdef0123456789abcdef01234567"

CLEAN_NIGHTLY = f"""\
name: Fixture nightly
on:
  workflow_dispatch:
    inputs:
      job:
        type: choice
        options: ["all", "alpha"]
jobs:
  alpha:
    if: github.event.inputs.job == 'all' || github.event.inputs.job == 'alpha'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@{CLEAN_SHA} # v5
      - run: cargo test --workspace --no-fail-fast
"""

CLEAN_AUTOCLOSE = """\
name: Fixture auto-close
on:
  pull_request_target:
    types: [opened]
jobs:
  close:
    if: >
      github.event.pull_request.user.login != github.repository_owner &&
      github.event.pull_request.user.login != 'dependabot[bot]' &&
      github.event.pull_request.user.login != 'github-actions[bot]'
    runs-on: ubuntu-latest
    steps:
      - run: echo close
"""

EXPECTED_RULES = (
    "[uses-sha]",
    "[needs-resolved]",
    "[dispatch-list]",
    "[bot-filter]",
    "[advisory-name]",
    "[test-threads]",
)


def selftest() -> int:
    failures: list[str] = []

    def case(label: str, condition: bool, detail: str) -> None:
        if condition:
            print(f"  ok    {label}")
        else:
            print(f"  FAIL  {label}")
            failures.append(f"{label}: {detail}")

    with tempfile.TemporaryDirectory() as tmp:
        faulty = Path(tmp) / "faulty"
        faulty.mkdir()
        (faulty / "nightly.yml").write_text(FAULTY_NIGHTLY, encoding="utf-8")
        (faulty / "auto-close-prs.yml").write_text(FAULTY_AUTOCLOSE, encoding="utf-8")
        defects, files, uses = audit(faulty)
        case(
            "the faulty tree was measured",
            files == 2 and uses == 1,
            f"measured {files} file(s) and {uses} uses, expected 2 and 1",
        )
        for rule in EXPECTED_RULES:
            case(
                f"the faulty tree trips {rule}",
                any(rule in defect for defect in defects),
                f"no defect carries {rule}; the rule cannot fire, so its green "
                f"on the real tree says nothing. Defects: {defects!r}",
            )

        clean = Path(tmp) / "clean"
        clean.mkdir()
        (clean / "nightly.yml").write_text(CLEAN_NIGHTLY, encoding="utf-8")
        (clean / "auto-close-prs.yml").write_text(CLEAN_AUTOCLOSE, encoding="utf-8")
        defects, files, _ = audit(clean)
        case(
            "positive control: a conforming tree yields no defect",
            files == 2 and not defects,
            f"expected 2 files and no defect, got {files} file(s) and "
            f"{defects!r}",
        )

        empty = Path(tmp) / "empty"
        empty.mkdir()
        _, files, _ = audit(empty)
        case(
            "an empty directory reports zero files measured",
            files == 0,
            f"measured {files} file(s) in an empty directory",
        )

    if failures:
        print(f"\nselftest: {len(failures)} case(s) failed", file=sys.stderr)
        return 1
    print("\nselftest: every case holds")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Verify the form of the GitHub workflows: SHA pins, "
        "resolved needs, dispatch coverage, bot exclusions, advisory naming, "
        "no forced single-thread test runs."
    )
    parser.add_argument(
        "--selftest",
        action="store_true",
        help="run both directions on fabricated workflow trees and exit",
    )
    args = parser.parse_args()

    if args.selftest:
        return selftest()

    if not WORKFLOWS.is_dir():
        print(
            f"check_ci_workflows: {WORKFLOWS} is absent, so nothing was measured",
            file=sys.stderr,
        )
        return 2

    defects, files, uses = audit(WORKFLOWS)
    if files == 0:
        print(
            f"check_ci_workflows: no workflow file under {WORKFLOWS}, so "
            f"nothing was measured",
            file=sys.stderr,
        )
        return 2

    if defects:
        print(f"{len(defects)} defect(s) across {files} workflow file(s):")
        for defect in defects:
            print(f"  - {defect}")
        return 1

    print(
        f"check_ci_workflows: {files} workflow file(s), {uses} `uses:` "
        f"examined, every rule holds"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
