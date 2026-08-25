#!/usr/bin/env python3
"""Pin that four declared guards can still render a negative verdict.

A command is only a guard if it can fail. Three in this tree could not, each in
a different way, and each failure mode is invisible to a reader of the command
itself:

  - `scripts/automation/tools/validate.py` printed `TOTAL scripts with issues: N`
    and exited 0, because nothing ever set an exit code.
  - `scripts/test-build-macos.sh` ran the only signature verification of the
    tree and followed it with `|| true`, so the caller swallowed the verdict the
    tool had correctly produced.
  - `npx svelte-check` answered 823 errors of which 822 came from
    `node_modules`, so its verdict was never read. A guard nobody reads is a
    guard nobody can fail.

A fourth arrived by the other door. `detect-private-key` could always fail, and
did, on every commit touching the module that held the TLS test pair, whatever
the diff; the way through was `--no-verify`, which switches off all sixteen
hooks. Moving the pair to a path of its own and excusing that one path is the
fix, and an exemption is the cheapest way to blind a detector, so it is pinned
here from three directions rather than trusted.

Fixing each instance does nothing for the fourth guard someone writes next, so
this file pins the property rather than the fix: every guard enrolled here
carries a pair of cases in both directions. A guard that always answered
"faulty" would satisfy the negative half while being worthless, so each negative
case is paired with a positive control on the same subject.

Two properties this file holds itself to:

  1. Fixtures are built in a temporary directory and the real tree is never
     written to. Every case that needs a faulty subject builds its own.
  2. A pair whose subject is absent is reported as zero coverage and makes this
     command exit non-zero. A rule that examined nothing is not a rule that
     holds, and reporting it as a pass is the exact bias
     `scripts/check_selftest.py` exists to pin.

What the neutraliser detector does not see: it reads one line at a time, so a
`|| true` carried on a continuation, or a `set +e` spanning the call site, would
escape it. No such form exists in this tree today, which is why this is written
here rather than filed as an observed defect. A guard that is silent about its
own blind spot is the next form of the defect this file exists to pin.

Where it runs: local, on a complete development machine. Two of the three
subjects are absent from a fresh clone by construction. The gesture corpus is
excluded by `.gitignore`, and `node_modules` is an install artefact. Both are
looked up in this tree first and in the main working tree second, and the pair
says which one it measured.

Usage:
    python3 scripts/check_guard_verdicts.py
"""

import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

FAILURES: list[str] = []
UNCOVERED: list[str] = []


def case(name: str, condition: bool, detail: str) -> None:
    if condition:
        print(f"  ok    {name}")
    else:
        print(f"  FAIL  {name}")
        FAILURES.append(f"{name}: {detail}")


def zero_coverage(guard: str, reason: str) -> None:
    """Report a pair that could not examine its subject, and fail the run."""
    print(f"  ZERO COVERAGE  {guard}")
    print(f"                 {reason}")
    UNCOVERED.append(f"{guard}: {reason}")


def _main_working_tree() -> Path | None:
    """The main working tree, when this file is read from a linked worktree."""
    try:
        out = subprocess.run(
            ["git", "rev-parse", "--path-format=absolute", "--git-common-dir"],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return None
    if not out:
        return None
    parent = Path(out).parent
    return parent if parent != REPO_ROOT else None


def subject(relative: str) -> tuple[Path | None, str]:
    """Locate a subject that git may not track, and say where it was found."""
    here = REPO_ROOT / relative
    if here.exists():
        return here, "this tree"
    main = _main_working_tree()
    if main is not None and (main / relative).exists():
        return main / relative, f"the main working tree, {main}"
    return None, ""


# ── The gesture corpus validator ─────────────────────────────────────────────
# Its roots are relative, `UI = "crates/apollia-desktop/ui/src"` and
# `SCRIPTS = "scripts/automation"`, and it resolves nothing from `__file__`, so
# it is driven by the working directory of a throwaway tree.

CLEAN_STEP = {"kind": "click", "testid": "fixture-anchor"}
FAULTY_STEP = {"kind": "click", "testid": "fixture-anchor-that-nothing-renders"}


def _corpus_fixture(root: Path, step: dict[str, str]) -> None:
    ui = root / "crates/apollia-desktop/ui/src"
    scripts = root / "scripts/automation"
    ui.mkdir(parents=True, exist_ok=True)
    scripts.mkdir(parents=True, exist_ok=True)
    (ui / "Fixture.svelte").write_text(
        '<div data-testid="fixture-anchor">fixture</div>\n', encoding="utf-8"
    )
    (scripts / "fixture-script.json").write_text(
        json.dumps({"name": "fixture", "steps": [step]}), encoding="utf-8"
    )


def check_corpus_validator() -> None:
    print("gesture corpus validator: a script naming an absent anchor is rejected")
    validator, where = subject("scripts/automation/tools/validate.py")
    if validator is None:
        zero_coverage(
            "gesture corpus validator",
            "scripts/automation/tools/validate.py is absent from this tree and "
            "from the main working tree. It is excluded by .gitignore, so a "
            "clone has no copy of it and this pair examined nothing",
        )
        return
    print(f"        subject found in {where}")
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        for label, step, expected in (
            ("a script whose anchor the source renders", CLEAN_STEP, 0),
            ("a script naming an anchor nothing renders", FAULTY_STEP, 1),
        ):
            _corpus_fixture(root, step)
            run = subprocess.run(
                [sys.executable, str(validator)],
                cwd=root,
                capture_output=True,
                text=True,
            )
            direction = "passes" if expected == 0 else "is rejected"
            case(
                f"{label} {direction}",
                run.returncode == expected,
                f"expected exit {expected}, got {run.returncode}. Output:\n"
                f"{run.stdout}{run.stderr}",
            )
    # An empty measurement is not a pass. Launched from the wrong directory the
    # validator builds a corpus of zero anchors and validates zero scripts, and
    # it used to answer the same "ALL CLEAN" exit 0 as a sane tree. The empty
    # case must answer its own code, distinct from both the pass and the red.
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        _corpus_fixture(root, CLEAN_STEP)
        (root / "crates/apollia-desktop/ui/src/Fixture.svelte").unlink()
        run = subprocess.run(
            [sys.executable, str(validator)],
            cwd=root,
            capture_output=True,
            text=True,
        )
        case(
            "a tree with no anchor at all answers 2, not a pass",
            run.returncode == 2,
            f"expected exit 2 (nothing measured), got {run.returncode}. Output:\n"
            f"{run.stdout}{run.stderr}",
        )


# ── The signature verification ───────────────────────────────────────────────
# The defect had two halves, so the pair has two. The tool renders a negative
# verdict on a tampered bundle, and the caller no longer swallows it.

INFO_PLIST = """<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" \
"http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>Fixture</string>
<key>CFBundleIdentifier</key><string>fixture.guard.verdicts</string>
<key>CFBundleName</key><string>Fixture</string>
<key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>
"""

# `|| true`, `|| :` and a trailing `; true` all turn a failing command into a
# passing step. The tree carried the first form.
NEUTRALISER = re.compile(r"(\|\|\s*(true|:)|;\s*true)\s*$")


def check_signature_verification() -> None:
    print("signature verification: a tampered bundle is rejected, and not swallowed")

    if shutil.which("codesign") is None:
        zero_coverage(
            "signature verification, tool half",
            "codesign is not on this machine, so the tool half examined nothing",
        )
    else:
        with tempfile.TemporaryDirectory() as tmp:
            app = Path(tmp) / "Fixture.app"
            binary = app / "Contents/MacOS/Fixture"
            binary.parent.mkdir(parents=True)
            shutil.copy("/bin/echo", binary)
            (app / "Contents/Info.plist").write_text(INFO_PLIST, encoding="utf-8")
            subprocess.run(
                ["codesign", "--force", "--sign", "-", str(app)],
                capture_output=True,
                text=True,
                check=False,
            )
            intact = subprocess.run(
                ["codesign", "--verify", "--verbose=2", str(app)],
                capture_output=True,
                text=True,
            )
            case(
                "an intact ad-hoc signed bundle verifies",
                intact.returncode == 0,
                f"expected exit 0, got {intact.returncode}: {intact.stderr}",
            )
            raw = bytearray(binary.read_bytes())
            raw[len(raw) // 2] ^= 0xFF
            binary.write_bytes(bytes(raw))
            tampered = subprocess.run(
                ["codesign", "--verify", "--verbose=2", str(app)],
                capture_output=True,
                text=True,
            )
            case(
                "one flipped byte in the binary is rejected",
                tampered.returncode != 0,
                "codesign exited 0 on a bundle whose binary was modified after "
                "signing, so the tool half of this guard proves nothing",
            )

    script, where = subject("scripts/test-build-macos.sh")
    if script is None:
        zero_coverage(
            "signature verification, caller half",
            "scripts/test-build-macos.sh is absent, so the caller half "
            "examined nothing",
        )
        return
    print(f"        caller found in {where}")
    calls = [
        line.strip()
        for line in script.read_text(encoding="utf-8").splitlines()
        if "codesign" in line and "--verify" in line
    ]
    if not calls:
        zero_coverage(
            "signature verification, caller half",
            "no codesign --verify call remains in scripts/test-build-macos.sh, "
            "so there is no caller left to examine",
        )
        return
    swallowed = [line for line in calls if NEUTRALISER.search(line)]
    case(
        "the build script calls the verification without a neutraliser",
        not swallowed,
        f"these call sites swallow their verdict: {swallowed}",
    )
    case(
        "the same detector fires on a call site that carries one",
        bool(NEUTRALISER.search('codesign --verify --verbose=2 "$APP" || true')),
        "the detector stayed silent on a line that ends in `|| true`, so its "
        "silence on the real call site says nothing",
    )


# ── The frontend type verdict ────────────────────────────────────────────────
# 822 of the 823 errors came from declaration files under node_modules, and
# `skipLibCheck` drops exactly those. The pair asserts that the faulty
# declaration goes quiet while a faulty source file is still caught, and a
# control proves the declaration was genuinely faulty in the first place.

FIXTURE_TSCONFIG = {
    "compilerOptions": {
        "strict": True,
        "noEmit": True,
        "skipLibCheck": True,
        "moduleResolution": "bundler",
        "module": "ESNext",
        "target": "ESNext",
    },
    "include": ["src/**/*.ts", "src/**/*.d.ts"],
}
FAULTY_DECLARATION = "declare const brokenDeclaration: NoSuchTypeAnywhere;\n"
FAULTY_SOURCE = "export const wrong: string = 42;\n"


def _svelte_check(binary: Path, project: Path) -> str:
    run = subprocess.run(
        [str(binary), "--workspace", str(project), "--output", "human",
         "--threshold", "error"],
        capture_output=True,
        text=True,
    )
    return run.stdout + run.stderr


def check_frontend_type_verdict() -> None:
    print("frontend type verdict: library noise is dropped, repository errors are not")
    binary, where = subject("crates/apollia-desktop/ui/node_modules/.bin/svelte-check")
    if binary is None:
        zero_coverage(
            "frontend type verdict",
            "crates/apollia-desktop/ui/node_modules/.bin/svelte-check is absent "
            "from this tree and from the main working tree. It is an install "
            "artefact, so a fresh worktree has no copy and this pair examined "
            "nothing",
        )
        return
    print(f"        subject found in {where}")
    with tempfile.TemporaryDirectory() as tmp:
        project = Path(tmp)
        (project / "src").mkdir()
        (project / "src/vendor.d.ts").write_text(FAULTY_DECLARATION, encoding="utf-8")
        (project / "src/app.ts").write_text(FAULTY_SOURCE, encoding="utf-8")
        config = json.loads(json.dumps(FIXTURE_TSCONFIG))
        (project / "tsconfig.json").write_text(json.dumps(config), encoding="utf-8")
        skipping = _svelte_check(binary, project)
        case(
            "a faulty source file is reported",
            "app.ts" in skipping,
            f"the error in src/app.ts went unreported, so skipping library "
            f"checks silenced more than declaration files. Output:\n{skipping}",
        )
        case(
            "a faulty declaration file is not reported",
            "vendor.d.ts" not in skipping,
            f"the declaration file still reported an error, so the option does "
            f"not cover the 822 lines it was chosen for. Output:\n{skipping}",
        )
        config["compilerOptions"]["skipLibCheck"] = False
        (project / "tsconfig.json").write_text(json.dumps(config), encoding="utf-8")
        checking = _svelte_check(binary, project)
        case(
            "control: that declaration file is genuinely faulty",
            "vendor.d.ts" in checking,
            f"the declaration file reported nothing even when checked, so its "
            f"silence above proves nothing. Output:\n{checking}",
        )

    config_path, _ = subject("crates/apollia-desktop/ui/tsconfig.json")
    if config_path is None:
        zero_coverage(
            "frontend type verdict, caller half",
            "crates/apollia-desktop/ui/tsconfig.json is absent, so nothing "
            "says the repository asks for the behaviour proven above",
        )
        return
    declared = json.loads(config_path.read_text(encoding="utf-8"))
    case(
        "the repository asks for that behaviour",
        declared.get("compilerOptions", {}).get("skipLibCheck") is True,
        "crates/apollia-desktop/ui/tsconfig.json does not set skipLibCheck, so "
        "the pair above proves a behaviour the tree does not request",
    )


# ── The private-key detector ─────────────────────────────────────────────────
# The hook refuses every commit touching a file that carries a private key,
# whatever the diff, so the TLS pair inlined in `api/server.rs` made that
# module uncommittable and the way through was `--no-verify`, which switches
# off all sixteen hooks. The pair now lives on a path of its own and that one
# path is excused. An exemption is the cheapest way to blind a detector, so the
# three directions below are what keeps this one honest: it still fires outside
# the excused path, the excused file is genuinely a file it would fire on, and
# the exemption still covers exactly one path.

# Assembled rather than written out. This file is judged by the very hook it
# drives here, and a literal marker in it would make it a file carrying a key:
# the first draft did exactly that and the hook caught it, which is the pair
# working one level up from where it was aimed.
_KEY_MARK = "PRIVATE KEY-----"
PROBE_KEY = f"-----BEGIN {_KEY_MARK}\nZmFrZQ==\n-----END {_KEY_MARK}\n"
EXEMPTED_PATH = "crates/apollia-runtime/src/api/tls_test_material.rs"
HOOK_ID = "detect-private-key"
HOOK_ENTRY = re.compile(
    rf"^(\s*)-\s*id:\s*{re.escape(HOOK_ID)}\s*$\n((?:\1\s+\S.*\n|\s*\n)*)",
    re.MULTILINE,
)
HOOK_EXCLUDE = re.compile(r"^\s*exclude:\s*['\"](.+?)['\"]\s*$", re.MULTILINE)


def _hook_exclude(config_text: str) -> str | None:
    """The `exclude` carried by the detect-private-key entry, if it carries one."""
    entry = HOOK_ENTRY.search(config_text)
    if entry is None:
        return None
    found = HOOK_EXCLUDE.search(entry.group(2))
    return found.group(1) if found else None


def _run_hook(paths: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["pre-commit", "run", HOOK_ID, "--files", *paths],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
    )


def check_private_key_detector() -> None:
    print("private-key detector: it still fires, and the exemption is one path wide")
    if shutil.which("pre-commit") is None:
        zero_coverage(
            "private-key detector",
            "pre-commit is not on this machine, so this pair examined nothing",
        )
        return
    config, where = subject(".pre-commit-config.yaml")
    material, _ = subject(EXEMPTED_PATH)
    if config is None or material is None:
        zero_coverage(
            "private-key detector",
            f"{'.pre-commit-config.yaml' if config is None else EXEMPTED_PATH} is "
            "absent, so this pair examined nothing",
        )
        return
    print(f"        configuration found in {where}")

    with tempfile.TemporaryDirectory() as tmp:
        planted = Path(tmp) / "planted.pem"
        planted.write_text(PROBE_KEY, encoding="utf-8")
        fired = _run_hook([str(planted)])
        case(
            "a file carrying a key, outside the excused path, is caught",
            fired.returncode != 0 and "planted.pem" in fired.stdout,
            f"expected a non-zero exit naming the file, got {fired.returncode}. "
            f"Output:\n{fired.stdout}{fired.stderr}",
        )
        # The excused file passing proves nothing unless the same detector,
        # given the same bytes at a path it does not excuse, fires on them.
        copied = Path(tmp) / "copied.rs"
        copied.write_text(material.read_text(encoding="utf-8"), encoding="utf-8")
        control = _run_hook([str(copied)])
        case(
            "control: the excused file's own content is content it fires on",
            control.returncode != 0 and "copied.rs" in control.stdout,
            f"the detector stayed silent on the bytes of {EXEMPTED_PATH} copied "
            f"to a path it does not excuse, so its silence on the real path says "
            f"nothing. Output:\n{control.stdout}{control.stderr}",
        )

    excused = _run_hook([EXEMPTED_PATH])
    case(
        "the excused path passes",
        excused.returncode == 0,
        f"expected exit 0 on {EXEMPTED_PATH}, got {excused.returncode}. Output:\n"
        f"{excused.stdout}{excused.stderr}",
    )

    pattern = _hook_exclude(config.read_text(encoding="utf-8"))
    if pattern is None:
        zero_coverage(
            "private-key detector, exemption width",
            f"the {HOOK_ID} entry of {config} declares no exclude, so there is "
            "no exemption left to measure",
        )
        return
    tracked = subprocess.run(
        ["git", "ls-files"],
        cwd=config.parent,
        capture_output=True,
        text=True,
    ).stdout.splitlines()
    subtracted = [path for path in tracked if re.search(pattern, path)]
    case(
        "the exemption subtracts exactly one path",
        subtracted == [EXEMPTED_PATH],
        f"the exclude {pattern!r} subtracts {len(subtracted)} tracked path(s) "
        f"from the detector: {subtracted}. An exemption that grew is an "
        "exemption nobody arbitrated",
    )


def main() -> int:
    check_corpus_validator()
    print()
    check_signature_verification()
    print()
    check_frontend_type_verdict()
    print()
    check_private_key_detector()

    if UNCOVERED:
        print(f"\n{len(UNCOVERED)} pair(s) examined nothing:\n", file=sys.stderr)
        for entry in UNCOVERED:
            print(f"  {entry}\n", file=sys.stderr)
    if FAILURES:
        print(f"\n{len(FAILURES)} guard verdict failure(s):\n", file=sys.stderr)
        for entry in FAILURES:
            print(f"  {entry}\n", file=sys.stderr)
    if UNCOVERED or FAILURES:
        return 1
    print(
        "\nfour guards hold a pair in both directions: the corpus validator "
        "rejects an absent anchor, the signature verification rejects a tampered "
        "bundle and no caller swallows it, the frontend verdict drops library "
        "noise without dropping repository errors, and the private-key detector "
        "still fires everywhere its one excused path is not"
    )
    return 0


if __name__ == "__main__":
    argparse.ArgumentParser(description=__doc__.splitlines()[0]).parse_args()
    sys.exit(main())
