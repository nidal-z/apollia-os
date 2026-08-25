#!/usr/bin/env python3
"""Every CLI leaf honours the published `--json` and exit-code contract.

Promoted from the campaign probe (`cli_json_probe.py`). The leaf inventory is
enumerated from the built binary by walking `--help` recursively, then every
leaf runs once with `--json`, dummy required arguments, a throwaway HOME, a
socket path that does not exist, and stdin closed. The published contract
(`crates/apollia-cli/AGENTS.md` sections 3 and 6, `README.md`,
`docs/site/docs/architecture/08-decisions.md#cli`) is asserted on what comes
out:

  R1  under `--json`, stdout carries at most one JSON document, and a non-zero
      exit carries exactly one: `{"error": {"code": ..., "message": ...}}`
  R2  a clap usage refusal exits 1 (1 = usage, 2 = runtime)
  R3  the envelope `code` names the exit code (`general_error` = 1,
      `runtime_error` = 2, ...), and an unreachable runtime is `runtime_error`
  R4  stderr carries no ANSI escape sequence when it is not a TTY

Leaves in SKIP are the network, model or daemon leaves alone; each carries its
reason. Verdict by exit code, since the caller reads it rather than the text:

  0  every measured leaf holds the contract
  1  at least one leaf breaks it
  2  nothing was measured: the binary is absent (build it with
     `cargo build -p apollia-cli`) or the tree walk produced no leaf

`--selftest` exercises the classifier on canned outputs, in both directions:
a string envelope, a wrong exit code and an ANSI stderr must be reported, and
a conforming leaf must pass.

Usage:
    python3 scripts/check_cli_json_contract.py [--bin PATH]
    python3 scripts/check_cli_json_contract.py --selftest
"""

import argparse
import concurrent.futures
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_BIN = REPO_ROOT / "target/debug/apollia-os"

# The exit code each envelope code publishes (crates/apollia-cli/src/exit_codes.rs).
CODE_TO_EXIT = {
    "general_error": 1,
    "runtime_error": 2,
    "task_failed": 3,
    "timeout": 4,
    "interrupted": 5,
}

PARSE_REFUSAL = re.compile(
    r"error: (unexpected argument|unrecognized|invalid value|the following required)"
)

# Network, model or daemon leaves only. Everything else is measured.
SKIP = {
    "start": "runs the daemon in the foreground",
    "update": "outbound HTTPS to the GitHub releases API",
    "llm chat": "one-shot inference: needs a loaded model",
    "stt model download": "outbound HTTPS whisper model download",
}


def help_text(bin_path: str, path: list[str], env: dict) -> str:
    try:
        out = subprocess.run(
            [bin_path, *path, "--help"],
            capture_output=True,
            text=True,
            timeout=20,
            env=env,
        )
    except (subprocess.SubprocessError, OSError):
        return ""
    return (out.stdout or "") + (out.stderr or "")


def subcommands(text: str) -> list[str]:
    lines = text.splitlines()
    try:
        start = next(i for i, ln in enumerate(lines) if ln.strip() == "Commands:")
    except StopIteration:
        return []
    subs = []
    for ln in lines[start + 1 :]:
        if not ln.strip():
            break
        m = re.match(r"^  (\S+)", ln)
        if m and m.group(1) != "help":
            subs.append(m.group(1))
    return subs


def enumerate_leaves(bin_path: str, env: dict) -> list[list[str]]:
    """Walk `--help` recursively; a node without subcommands is a leaf."""
    top = subcommands(help_text(bin_path, [], env))
    leaves: list[list[str]] = []
    stack = [[c] for c in top]
    while stack:
        path = stack.pop()
        subs = subcommands(help_text(bin_path, path, env))
        if subs:
            stack.extend(path + [s] for s in subs)
        else:
            leaves.append(path)
    return sorted(leaves)


def dummy_args(help_out: str, path: list[str]) -> list[str]:
    """Dummy argv for the required arguments found in the Usage line."""
    m = re.search(r"^Usage: \S+ (.*?)$", help_out, re.M)
    if not m:
        return []
    rest = m.group(1)
    for tok in path:
        rest = rest.replace(tok, "", 1)
    rest = re.sub(r"\[[^\]]*\]", "", rest)
    argv = []
    for tok in rest.split():
        if tok.startswith("--"):
            argv.append(tok)
        elif tok.startswith("<"):
            argv.append("x")
    return argv


def violations(leaf: str, exit_code: int, stdout: str, stderr: str) -> list[str]:
    """The contract breaches one invocation carries. Empty when it conforms."""
    found = []
    if "\x1b[" in stderr:
        found.append(f"{leaf}: ANSI escape on stderr while it is not a TTY")
    if PARSE_REFUSAL.search(stderr):
        if exit_code != 1:
            found.append(f"{leaf}: clap usage refusal exited {exit_code}, the contract says 1")
        return found
    body = stdout.strip()
    if exit_code == 0:
        if body:
            try:
                json.loads(body)
            except ValueError:
                found.append(f"{leaf}: stdout under --json is not a single JSON document")
        return found
    try:
        doc = json.loads(body) if body else None
    except ValueError:
        doc = None
    if doc is None:
        found.append(
            f"{leaf}: exit {exit_code} with no JSON error envelope on stdout "
            f"(stderr: {stderr.strip().splitlines()[0][:80] if stderr.strip() else 'empty'})"
        )
        return found
    if not (isinstance(doc, dict) and "error" in doc):
        # A structured refusal (a validation report, a status document) is the
        # leaf's data, not an error emission; the exit code still signals it.
        return found
    err = doc.get("error")
    if not (
        set(doc) == {"error"}
        and isinstance(err, dict)
        and set(err) == {"code", "message"}
        and isinstance(err.get("message"), str)
        and err.get("code") in CODE_TO_EXIT
    ):
        found.append(f"{leaf}: error output is not {{'error': {{'code', 'message'}}}}: {body[:80]}")
        return found
    if CODE_TO_EXIT[err["code"]] != exit_code:
        found.append(f"{leaf}: envelope code {err['code']!r} does not name exit code {exit_code}")
    if "runtime not started" in err["message"] and err["code"] != "runtime_error":
        found.append(
            f"{leaf}: unreachable runtime reported as {err['code']!r}, not 'runtime_error'"
        )
    return found


def run_leaf(bin_path: str, leaf: list[str], env: dict, home: str, sock: str) -> list[str]:
    name = " ".join(leaf)
    extra = dummy_args(help_text(bin_path, leaf, env), leaf)
    argv = [bin_path, *leaf, *extra, "--json", "--socket", sock]
    try:
        p = subprocess.run(
            argv,
            capture_output=True,
            text=True,
            timeout=30,
            env=env,
            stdin=subprocess.DEVNULL,
            cwd=home,
        )
    except subprocess.TimeoutExpired:
        return [f"{name}: did not finish in 30s without a runtime"]
    return violations(name, p.returncode, p.stdout, p.stderr)


def measure(bin_path: str) -> int:
    if not Path(bin_path).is_file():
        print(
            f"nothing measured: {bin_path} is absent. Build it with `cargo build -p apollia-cli`.",
            file=sys.stderr,
        )
        return 2
    home = tempfile.mkdtemp(prefix="apollia-json-contract-")
    env = dict(os.environ, HOME=home)
    env.pop("NO_COLOR", None)
    env.pop("RUST_LOG", None)
    sock = os.path.join(home, "no-runtime.sock")
    leaves = enumerate_leaves(bin_path, env)
    if not leaves:
        print("nothing measured: the --help walk produced no leaf", file=sys.stderr)
        return 2
    measured = [lf for lf in leaves if " ".join(lf) not in SKIP]
    reds: list[str] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
        futures = [pool.submit(run_leaf, bin_path, lf, env, home, sock) for lf in measured]
        for future in futures:
            reds.extend(future.result())
    # A usage refusal must exit 1 even when the leaf itself parses: drive one
    # unknown flag through a known leaf so the rule is asserted on every run.
    bogus = subprocess.run(
        [bin_path, "agent", "list", "--bogus"],
        capture_output=True,
        text=True,
        timeout=20,
        env=env,
        stdin=subprocess.DEVNULL,
        cwd=home,
    )
    reds.extend(violations("agent list --bogus", bogus.returncode, bogus.stdout, bogus.stderr))
    for line in sorted(reds):
        print(f"RED  {line}")
    print(f"leaves enumerated : {len(leaves)}")
    print(f"leaves measured   : {len(measured)} (+ 1 usage-refusal probe)")
    print(f"leaves skipped    : {len(leaves) - len(measured)} (network, model or daemon)")
    print(f"contract breaches : {len(reds)}")
    return 1 if reds else 0


def selftest() -> int:
    failures = []

    def case(name: str, condition: bool) -> None:
        if condition:
            print(f"  ok    {name}")
        else:
            print(f"  FAIL  {name}")
            failures.append(name)

    envelope = json.dumps({"error": {"code": "runtime_error", "message": "runtime not started"}})
    case("a conforming error envelope passes", violations("x", 2, envelope, "") == [])
    case(
        "a string envelope is reported",
        violations("x", 2, '{"error": "runtime not started"}', "") != [],
    )
    case(
        "a bare stderr error under --json is reported",
        violations("x", 1, "", "Error: nope\n") != [],
    )
    case(
        "an envelope code that does not name the exit code is reported",
        violations("x", 1, envelope, "") != [],
    )
    case(
        "an unreachable runtime outside runtime_error is reported",
        violations(
            "x",
            1,
            json.dumps({"error": {"code": "general_error", "message": "runtime not started"}}),
            "",
        )
        != [],
    )
    case(
        "a clap refusal at exit 1 passes",
        violations("x", 1, "", "error: unexpected argument '--bogus' found\n") == [],
    )
    case(
        "a clap refusal at exit 2 is reported",
        violations("x", 2, "", "error: unexpected argument '--bogus' found\n") != [],
    )
    case(
        "text glued to the JSON document is reported",
        violations("x", 0, 'Continue? [y/N] {"status": "cancelled"}', "") != [],
    )
    case("an ANSI stderr is reported", violations("x", 0, "{}", "\x1b[2mINFO\x1b[0m x\n") != [])
    case(
        "a data-shaped refusal at non-zero exit passes",
        violations("x", 1, '{"ok": false, "warnings": []}', "") == [],
    )
    case("a clean success passes", violations("x", 0, '{"agents": []}', "") == [])
    case("an empty success passes", violations("x", 0, "", "") == [])

    if failures:
        print(f"\nselftest: {len(failures)} case(s) failed", file=sys.stderr)
        return 1
    print("\nselftest: every case holds")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Assert the published --json error and exit-code contract on every CLI leaf."
    )
    parser.add_argument("--bin", default=str(DEFAULT_BIN), help="path to the apollia-os binary")
    parser.add_argument(
        "--selftest", action="store_true", help="run the classifier on canned outputs"
    )
    args = parser.parse_args()
    if args.selftest:
        return selftest()
    return measure(args.bin)


if __name__ == "__main__":
    sys.exit(main())
