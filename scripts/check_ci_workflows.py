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

Four more rules hold `release.yml` to the contract it broke silently: a
publish step that tolerated unmatched globs published nothing and said so to
no one, a `codesign --verify || true` rendered no verdict, six jobs sat on an
image GitHub retires on 2026-09-17, and the three desktop jobs could not
finish `cargo tauri build` at all while `createUpdaterArtifacts` demanded a
signing key no secret provided:

  unmatched-files   no step sets `fail_on_unmatched_files: false`; a publish
                    that skips silently is a publish nobody reviewed
  sign-verify       no `codesign`, `notarytool` or `stapler` line ends in
                    `|| true`; a verification that cannot fail verifies
                    nothing
  runner-deprecated no `ubuntu-22.04` anywhere in a workflow, comments and
                    embedded matrices included, since the raw text is what a
                    future copy-paste reads
  updater-guard     every `desktop-*` job of `release.yml` carries a step
                    that reads `TAURI_SIGNING_PRIVATE_KEY` and mentions
                    `createUpdaterArtifacts`, so a build without the secret
                    still finishes, explicitly and with a named warning

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
SIGN_SWALLOWED = re.compile(r"\b(codesign|notarytool|stapler)\b.*\|\|\s*true\b")
DEPRECATED_RUNNER = "ubuntu-22.04"

BOT_FILTER_FILE = "auto-close-prs.yml"
REQUIRED_BOT_EXCLUSIONS = ("dependabot[bot]", "github-actions[bot]")

RELEASE_FILE = "release.yml"
DESKTOP_JOB_PREFIX = "desktop-"


SOURCE_ONLY_EXTRACTORS = frozenset({"rust"})
"""CodeQL languages whose extractor refuses every build mode but `none`.

Measured on 2026-08-28: the `rust` entry carried `build-mode: manual`, and
every run since the workflow was written died on `A fatal error occurred:
Rust does not support the manual build mode`, while `python` and
`javascript-typescript` were analysed normally in the same run. Add a
language here only once its extractor has refused a mode in a real log.
"""


def _has_updater_guard(job: dict) -> bool:
    """True when one step reads the signing key and names the updater flag."""
    for step in _steps(job):
        env = step.get("env")
        env_keys = env.keys() if isinstance(env, dict) else ()
        run = str(step.get("run", ""))
        reads_key = "TAURI_SIGNING_PRIVATE_KEY" in env_keys or (
            "TAURI_SIGNING_PRIVATE_KEY" in run
        )
        if reads_key and "createUpdaterArtifacts" in run:
            return True
    return False


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


REGEN_CALL = re.compile(r"^\s*(?:bash|sh|\./)\s*\S*regen\.sh\b")
"""A line that runs `regen.sh`, as opposed to one that merely names it."""


REGEN_ONLY_INVOCATIONS = ("gen-cli-docs", "docs/site/scripts/gen_")
"""Generation commands a workflow must reach through `docs/site/regen.sh`.

Measured on 2026-08-28: the `docs-generated` job carried its own copy of the
five generation steps. The copy had drifted from the script in the way that
decides a verdict, `set -euo pipefail`: a failed `cargo run` was swallowed by
the `perl` closing its pipe, the page was written with its header alone, and
the drift check reported stale documentation over a build that never ran.
One source, or the copy drifts again.
"""


def _matrix_include(job: dict) -> list[dict]:
    """The `strategy.matrix.include` entries of one job, mappings only."""
    strategy = job.get("strategy")
    if not isinstance(strategy, dict):
        return []
    matrix = strategy.get("matrix")
    if not isinstance(matrix, dict):
        return []
    include = matrix.get("include")
    if not isinstance(include, list):
        return []
    return [entry for entry in include if isinstance(entry, dict)]


def _steps(job: dict) -> list[dict]:
    steps = job.get("steps")
    if not isinstance(steps, list):
        return []
    return [step for step in steps if isinstance(step, dict)]


def audit_file(path: Path) -> tuple[list[str], int]:
    """Defects of one workflow file, and the number of `uses:` examined."""
    defects: list[str] = []
    raw = path.read_text(encoding="utf-8")
    try:
        doc = yaml.safe_load(raw)
    except yaml.YAMLError as error:
        return [f"{path.name}: unparseable YAML ({error})"], 0
    if not isinstance(doc, dict):
        return [f"{path.name}: not a mapping, so it declares no job"], 0

    for number, line in enumerate(raw.splitlines(), start=1):
        if DEPRECATED_RUNNER in line:
            defects.append(
                f"{path.name}:{number}: [runner-deprecated] mentions "
                f"{DEPRECATED_RUNNER}, an image GitHub retires on 2026-09-17"
            )
        if SIGN_SWALLOWED.search(line):
            defects.append(
                f"{path.name}:{number}: [sign-verify] a codesign, notarytool "
                f"or stapler line ends in '|| true', so it renders no verdict"
            )

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

        for step in _steps(job):
            run = str(step.get("run", ""))
            # An invocation, not a mention: the step this rule was written for
            # names regen.sh inside an `echo` that stamps the generated header,
            # and a substring test excused it on the strength of its own banner.
            if any(REGEN_CALL.match(line) for line in run.splitlines()):
                continue
            for invocation in REGEN_ONLY_INVOCATIONS:
                if invocation in run:
                    defects.append(
                        f"{path.name}: job {job_id}: [regen-single-source] a "
                        f"step runs {invocation!r} itself instead of "
                        f"`bash docs/site/regen.sh`; the copy drops the "
                        f"script's `set -euo pipefail` and turns a failed "
                        f"generator into stale-looking documentation"
                    )
                    break

        for entry in _matrix_include(job):
            language = str(entry.get("language", ""))
            mode = str(entry.get("build-mode", ""))
            if language in SOURCE_ONLY_EXTRACTORS and mode and mode != "none":
                defects.append(
                    f"{path.name}: job {job_id}: [codeql-build-mode] language "
                    f"{language!r} declares build-mode {mode!r}; its extractor "
                    f"reads sources and accepts only 'none', so the job dies "
                    f"before it analyses a single file"
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

        if path.name == RELEASE_FILE and job_id.startswith(DESKTOP_JOB_PREFIX):
            if not _has_updater_guard(job):
                defects.append(
                    f"{path.name}: job {job_id}: [updater-guard] no step reads "
                    f"TAURI_SIGNING_PRIVATE_KEY and names "
                    f"createUpdaterArtifacts, so a build without the secret "
                    f"dies inside cargo tauri build instead of finishing "
                    f"without updater artifacts"
                )

        if path.name == BOT_FILTER_FILE:
            condition = str(job.get("if", ""))
            missing = [bot for bot in REQUIRED_BOT_EXCLUSIONS if bot not in condition]
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
            with_block = step.get("with")
            if (
                isinstance(with_block, dict)
                and with_block.get("fail_on_unmatched_files") is False
            ):
                defects.append(
                    f"{path.name}: job {job_id}: [unmatched-files] a step sets "
                    f"fail_on_unmatched_files: false, so a glob that matches "
                    f"nothing publishes nothing and says so to no one"
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

FAULTY_RELEASE = f"""\
name: Fixture release
on:
  push:
    tags: ["v*"]
jobs:
  desktop-macos:
    runs-on: ubuntu-22.04
    steps:
      - uses: actions/checkout@{CLEAN_SHA} # v5
      - run: cargo tauri build --target aarch64-apple-darwin
      - run: codesign --verify --verbose=2 "$APP" || true
  release:
    needs: desktop-macos
    runs-on: ubuntu-latest
    steps:
      - uses: softprops/action-gh-release@{CLEAN_SHA} # v2
        with:
          fail_on_unmatched_files: false
"""

CLEAN_RELEASE = f"""\
name: Fixture release
on:
  push:
    tags: ["v*"]
jobs:
  desktop-macos:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@{CLEAN_SHA} # v5
      - name: Guard the updater signing key
        env:
          TAURI_SIGNING_PRIVATE_KEY: dummy
        run: echo "createUpdaterArtifacts stays on when the key is present"
      - run: cargo tauri build --target aarch64-apple-darwin
      - run: codesign --verify --verbose=2 "$APP"
  release:
    needs: desktop-macos
    runs-on: ubuntu-latest
    steps:
      - uses: softprops/action-gh-release@{CLEAN_SHA} # v2
        with:
          fail_on_unmatched_files: true
"""

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
    "[unmatched-files]",
    "[sign-verify]",
    "[runner-deprecated]",
    "[updater-guard]",
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
        (faulty / "release.yml").write_text(FAULTY_RELEASE, encoding="utf-8")
        defects, files, uses = audit(faulty)
        case(
            "the faulty tree was measured",
            files == 3 and uses == 3,
            f"measured {files} file(s) and {uses} uses, expected 3 and 3",
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
        (clean / "release.yml").write_text(CLEAN_RELEASE, encoding="utf-8")
        defects, files, _ = audit(clean)
        case(
            "positive control: a conforming tree yields no defect",
            files == 3 and not defects,
            f"expected 3 files and no defect, got {files} file(s) and {defects!r}",
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
