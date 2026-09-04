#!/usr/bin/env python3
"""Smoke sweep: one real invocation of every clap leaf of `apollia-os`.

The command tree is machine-readable and `--json` is a global flag, so the
whole surface can be played rather than sampled. For each leaf the sweep builds
a throwaway seeded HOME, synthesises an argument vector from the leaf's own
usage line, runs the binary there, and holds the result to three contracts:

  exit    the process exits with a code the CLI documents
          (crates/apollia-cli/src/exit_codes.rs: 0 success, 1 general, 2
          runtime absent, 3 task failed, 4 timeout, 5 interrupted). A code
          outside that set, or a death by signal, is a defect.
  json    stdout under `--json` that opens with `{` or `[` parses, whole or as
          JSON Lines. A truncated or interleaved document is a defect.
  panic   neither stream carries a Rust panic, a backtrace note or a fatal
          runtime error, and the process is not killed by a signal.

A leaf that does not return inside its deadline is a defect as well: a hung
command is the failure a user meets first.

What the sweep counts is what it PLAYED. Leaves it refuses are named one by
one in tests/cli/smoke/policy.py with a class and a reason, and printed in the
report; a policy entry naming a leaf the binary no longer has fails the run,
because a stale exclusion is a hole that outlives its reason.

Isolation: one HOME per leaf, built by the committed seed builder
(tests/cli/seed/build-seed.sh) under $TMPDIR, and the working directory is that
HOME too, so a leaf that writes into the current directory (`workspace init`)
cannot reach the repository. The real ~/.apollia is never opened.

Verdict by exit code, since the caller reads it rather than the text:

  0  every played leaf held all three contracts
  1  at least one leaf broke one, or the policy names a leaf that no longer exists
  2  nothing was measured: no binary, a binary this tree did not produce, no
     leaf enumerated, or no seeded HOME to run in

`--selftest` runs the whole engine against a stub binary the harness generates,
whose leaves panic, exit outside the taxonomy, emit truncated JSON and hang, and
asserts the sweep reports each one and returns 1; then that the same engine
returns 0 on the clean leaf alone, and 2 when nothing is measured.

Stdlib only.

Usage:
    python3 tests/cli/smoke/sweep.py [--bin PATH] [--out DIR] [--jobs N]
    python3 tests/cli/smoke/sweep.py --daemon        # also record API routes
    python3 tests/cli/smoke/sweep.py --selftest
"""

import argparse
import concurrent.futures
import importlib.util
import json
import os
import re
import shutil
import socket
import stat
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path
from typing import List, Optional, Tuple

REPO_ROOT = Path(__file__).resolve().parents[3]
GUARD = REPO_ROOT / "scripts/check_cli_e2e_coverage.py"
FRESHNESS = REPO_ROOT / "scripts/binary_freshness.py"
SEED_BUILDER = REPO_ROOT / "tests/cli/seed/build-seed.sh"
DEFAULT_BIN = REPO_ROOT / "target/debug/apollia-os"
DEFAULT_OUT = REPO_ROOT / "tests/cli/report"

sys.path.insert(0, str(Path(__file__).resolve().parent))
import policy  # noqa: E402

# Codes crates/apollia-cli/src/exit_codes.rs declares. Clap's own usage error
# also lands on 2, which the CLI documents as the runtime-absent code; both are
# inside the taxonomy, and telling them apart is a job for an assertion suite,
# not for a smoke sweep.
ALLOWED_EXIT_CODES = frozenset({0, 1, 2, 3, 4, 5})

PANIC_MARKERS = (
    "panicked at",
    "stack backtrace:",
    "note: run with `RUST_BACKTRACE=1`",
    "fatal runtime error",
    "Segmentation fault",
    "double free",
    "memory allocation of",
)

REQUEST_LINE = re.compile(rb"^(GET|POST|PUT|PATCH|DELETE|HEAD) (\S+) HTTP/1\.[01]\r?$")

# clap prints this line under every parse rejection, and only clap prints it.
# A rejected invocation never reaches the command's own body, so counting it as
# coverage would be the argument-shaped form of "a mention is not a coverage".
CLAP_REJECTION = "For more information, try '--help'"

# sockaddr_un.sun_path is 104 bytes on macOS and 108 on Linux; the CLI derives
# its socket from $HOME/.apollia/runtime.sock. A HOME under the default $TMPDIR
# overflows that on macOS, and the CLI then fails with `io error: path must be
# shorter than SUN_LEN` and exit 1 instead of reaching its real refusal, exit 2
# `runtime not started`. Measured here: 121 leaves reported 1 under a $TMPDIR
# HOME, 4 under a short one. The sweep therefore roots its workspace somewhere
# short and refuses to run when the path it would use is still too long, rather
# than reporting a number about path lengths as if it were about the product.
SUN_PATH_MAX = 104
WORKSPACE_ROOT = os.environ.get("APOLLIA_SMOKE_TMP", "/tmp")


# ── Leaf enumeration, borrowed from the guard that already owns it ─────────
def load_guard():
    spec = importlib.util.spec_from_file_location("check_cli_e2e_coverage", GUARD)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def usage_line(bin_path: str, leaf: list[str], env: dict) -> str:
    try:
        out = subprocess.run(
            [bin_path, *leaf, "--help"],
            capture_output=True,
            text=True,
            timeout=20,
            env=env,
        )
    except (subprocess.SubprocessError, OSError):
        return ""
    text = (out.stdout or "") + (out.stderr or "")
    for line in text.splitlines():
        if line.startswith("Usage:"):
            return line.strip()
    return ""


def synthesise_argv(usage: str, leaf: list[str]) -> list[str]:
    """Required options and required positionals of a usage line, valued.

    `Usage: apollia-os llm backends create [OPTIONS] --provider <PROVIDER>
    --model <MODEL> <NAME>` yields
    `--provider <value> --model <value> <value>`. Bracketed tokens are
    optional and left out: a smoke sweep plays the shortest accepted form.
    """
    if not usage:
        return []
    tokens = usage.split()[1:]  # drop "Usage:"
    tokens = tokens[1 + len(leaf) :]  # drop the program name and the leaf path
    argv: list[str] = []
    i = 0
    while i < len(tokens):
        tok = tokens[i]
        if tok == "[OPTIONS]":
            i += 1
        elif tok.startswith("--") and i + 1 < len(tokens) and tokens[i + 1].startswith("<"):
            name = tokens[i + 1].strip("<>.")
            argv += [tok, placeholder_for(name)]
            i += 2
        elif tok.startswith("<"):
            argv.append(placeholder_for(tok.strip("<>.")))
            i += 1
        else:  # [OPTIONAL], [OPTIONAL]..., trailing markers
            i += 1
    return argv


def placeholder_for(name: str) -> str:
    return policy.PLACEHOLDER_DEFAULTS.get(name, policy.GENERIC_PLACEHOLDER)


def expand(tokens: list[str], run_dir: Path, home: Path) -> list[str]:
    agent_dir = home / ".apollia/agents/seed-classifier"
    return [
        t.replace("@RUN_DIR@", str(run_dir))
        .replace("@SEED_HOME@", str(home))
        .replace("@SEED_AGENT_DIR@", str(agent_dir))
        for t in tokens
    ]


# ── The three contracts ────────────────────────────────────────────────────
def check_exit(code: int) -> Optional[str]:
    if code < 0:
        return f"killed by signal {-code}"
    if code not in ALLOWED_EXIT_CODES:
        return (
            f"exit code {code} is outside the documented taxonomy "
            f"{sorted(ALLOWED_EXIT_CODES)}"
        )
    return None


def check_panic(stdout: str, stderr: str) -> Optional[str]:
    for stream, text in (("stderr", stderr), ("stdout", stdout)):
        for marker in PANIC_MARKERS:
            if marker in text:
                idx = text.index(marker)
                excerpt = text[max(0, idx - 80) : idx + 160].replace("\n", " | ")
                return f"panic marker {marker!r} on {stream}: {excerpt}"
    return None


def check_json(stdout: str) -> Tuple[Optional[str], bool, int]:
    """(defect, promised, documents) for stdout produced under `--json`.

    A body that opens with `{` or `[` promises JSON. It is read as a sequence
    of JSON values, which accepts the single document, JSON Lines, and the
    concatenated pretty-printed documents a streaming command emits; anything
    truncated or interleaved with prose fails here.

    More than one document is not a defect, it is an observation: `onboard`
    legitimately prints the submission then the outcome, and the only thing
    that costs is that `jq .` over the whole stream needs `--seq` or a split.
    A body that is not JSON at all is an observation too, not a defect: a leaf
    whose `--json` output is a shell completion script or a markdown export is
    a contract question for the assertion suite, not a crash.
    """
    body = stdout.strip()
    if not body or body[0] not in "{[":
        return None, False, 0
    decoder = json.JSONDecoder()
    index = 0
    documents = 0
    while index < len(body):
        while index < len(body) and body[index] in " \t\r\n":
            index += 1
        if index >= len(body):
            break
        try:
            _, index = decoder.raw_decode(body, index)
        except ValueError as exc:
            return f"stdout opens as JSON but does not parse: {exc}", True, documents
        documents += 1
    return None, True, documents


# ── Running one leaf ───────────────────────────────────────────────────────
def prepare_run_dir(run_dir: Path) -> None:
    """Input fixtures the argument table points at, inside the throwaway dir."""
    run_dir.mkdir(parents=True, exist_ok=True)
    (run_dir / "profile-in.json").write_text("{}\n", encoding="utf-8")
    (run_dir / "memory-in.json").write_text("[]\n", encoding="utf-8")
    (run_dir / "eval.jsonl").write_text(
        json.dumps({"id": "smoke", "input": "ping", "expected": "pong"}) + "\n",
        encoding="utf-8",
    )
    (run_dir / "sample.diff").write_text(
        "--- a/smoke.txt\n+++ b/smoke.txt\n@@ -1 +1 @@\n-old\n+new\n", encoding="utf-8"
    )
    # 44-byte RIFF/WAVE header with a zero-length data chunk: a real container,
    # no samples, so a decoder reaches its own empty-input path.
    (run_dir / "sample.wav").write_bytes(
        b"RIFF\x24\x00\x00\x00WAVEfmt \x10\x00\x00\x00\x01\x00\x01\x00"
        b"\x80\x3e\x00\x00\x00\x7d\x00\x00\x02\x00\x10\x00data\x00\x00\x00\x00"
    )


def child_env(home: Path) -> dict:
    env = {
        k: v
        for k, v in os.environ.items()
        if not k.startswith("APOLLIA_") and k not in ("XDG_CONFIG_HOME",)
    }
    env.update(
        HOME=str(home),
        NO_COLOR="1",
        RUST_LOG="error",
        # Hermetic secret storage: the OS keychain is unreachable from a
        # sub-process here, and reaching for it would measure the keychain.
        APOLLIA_TOKEN_STORAGE="file",
        APOLLIA_TOKEN_PASSPHRASE="cli-smoke-sweep-passphrase",
        # An editor that returns immediately, so `config edit` measures the
        # command rather than a terminal waiting for a human.
        EDITOR="/usr/bin/true",
        VISUAL="/usr/bin/true",
        PAGER="/bin/cat",
    )
    return env


UUID_SEGMENT = re.compile(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-")


def normalise_route(route: str, argv: list) -> str:
    """`GET /api/v1/agents/seed-classifier` -> `GET /api/v1/agents/{id}`.

    A path segment is treated as a parameter when it is one of the arguments
    the sweep itself passed, or a UUID the runtime minted from them. Nothing
    else is collapsed: the OpenAPI document is not consulted, so this is what
    crossed the wire with its own inputs folded back in, not a claim about the
    router's declared paths.
    """
    method, _, path = route.partition(" ")
    passed = {token for token in argv if token and not token.startswith("-")}
    segments = [
        "{id}" if (seg in passed or UUID_SEGMENT.match(seg)) else seg
        for seg in path.split("/")
    ]
    return f"{method} {'/'.join(segments)}"


def check_reached(stderr: str) -> Optional[str]:
    """A clap rejection means the leaf's own body never ran."""
    if CLAP_REJECTION in stderr:
        first = next(
            (ln for ln in stderr.splitlines() if ln.startswith("error:")), "rejected"
        )
        return f"the invocation never reached the command: {first}"
    return None


def play_leaf(
    bin_path: str,
    leaf: list[str],
    seed_ref: Path,
    workspace: Path,
    socket_path: Optional[str],
    index: int = 0,
    probe: Optional["RouteProbe"] = None,
) -> dict:
    name = " ".join(leaf)
    # A short directory name, not the leaf slug: the per-leaf HOME carries the
    # runtime socket, and sun_path is 104 bytes.
    home = workspace / f"l{index:03d}"
    slug = f"l{index:03d}"
    result: dict = {"leaf": name, "played": True}

    if socket_path is None:
        try:
            shutil.copytree(seed_ref, home, symlinks=True)
        except OSError as exc:
            result.update(defect=f"could not stage a seeded HOME: {exc}")
            return result
    else:
        home = seed_ref  # daemon mode: one shared HOME, the daemon holds it

    run_dir = home / "smoke-run" / slug
    prepare_run_dir(run_dir)

    override = policy.ARGV.get(name)
    env = child_env(home)
    if override is None:
        argv = synthesise_argv(usage_line(bin_path, leaf, env), leaf)
    else:
        argv = list(override)
    argv = expand(argv, run_dir, home)

    cmd = [bin_path, "--json"]
    if socket_path:
        cmd += ["--socket", socket_path]
    cmd += leaf + argv
    result["command"] = " ".join(cmd[1:])

    timeout = policy.TIMEOUTS.get(name, policy.DEFAULT_TIMEOUT)
    before = probe.snapshot() if probe else {}
    started = time.monotonic()
    timed_out = False
    try:
        with open(os.devnull, "rb") as devnull:
            proc = subprocess.run(
                cmd,
                stdin=devnull,
                capture_output=True,
                timeout=timeout,
                env=env,
                cwd=str(run_dir),
            )
        code = proc.returncode
        stdout = proc.stdout.decode("utf-8", "replace")
        stderr = proc.stderr.decode("utf-8", "replace")
    except subprocess.TimeoutExpired as exc:
        timed_out = True
        code = None
        stdout = (exc.stdout or b"").decode("utf-8", "replace")
        stderr = (exc.stderr or b"").decode("utf-8", "replace")
    except OSError as exc:
        result.update(defect=f"could not run the binary: {exc}")
        return result

    result["duration_ms"] = int((time.monotonic() - started) * 1000)
    result["exit"] = code
    if probe:
        after = probe.snapshot()
        fired = sorted(k for k, v in after.items() if v > before.get(k, 0))
        result["routes"] = fired
        result["operations"] = sorted({normalise_route(r, argv) for r in fired})
    result["stdout_bytes"] = len(stdout)
    result["stderr_bytes"] = len(stderr)

    defects = []
    if timed_out:
        defects.append(f"did not return within {timeout}s")
    else:
        for defect in (
            check_exit(code),
            check_panic(stdout, stderr),
            check_reached(stderr),
        ):
            if defect:
                defects.append(defect)
        json_defect, promised, documents = check_json(stdout)
        result["json_promised"] = promised
        result["json_documents"] = documents
        if json_defect:
            defects.append(json_defect)
        if stdout.strip() and not promised:
            result["observation"] = "stdout under --json is not a JSON document"
        elif documents > 1:
            result["observation"] = (
                f"stdout under --json is a sequence of {documents} JSON documents, "
                f"so `jq .` over the whole stream fails"
            )
    if defects:
        result["defect"] = "; ".join(defects)
        result["stdout_head"] = stdout[:400]
        result["stderr_head"] = stderr[:400]

    if socket_path is None:
        shutil.rmtree(home, ignore_errors=True)
    else:
        shutil.rmtree(run_dir, ignore_errors=True)
    return result


# ── Daemon mode: a logging proxy in front of the runtime socket ────────────
class RouteProbe:
    """Unix-socket proxy that records the HTTP request line of every request.

    The runtime serves axum over a Unix socket and installs no request-tracing
    layer, so the only place the route is legible without touching `crates/` is
    the wire. The CLI is pointed at this socket, every byte is forwarded to the
    daemon's own, and the client-to-server direction is scanned for request
    lines. What it measures is what crossed the wire, not what the router
    declares.
    """

    def __init__(self, listen_path: str, target_path: str):
        self.listen_path = listen_path
        self.target_path = target_path
        self.routes: dict[str, int] = {}
        self._lock = threading.Lock()
        self._stop = threading.Event()
        self._server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self._server.bind(listen_path)
        self._server.listen(64)
        self._server.settimeout(0.5)
        self._thread = threading.Thread(target=self._accept_loop, daemon=True)

    def start(self) -> None:
        self._thread.start()

    def stop(self) -> None:
        self._stop.set()
        self._thread.join(timeout=3)
        try:
            self._server.close()
        except OSError:
            pass

    def _accept_loop(self) -> None:
        while not self._stop.is_set():
            try:
                conn, _ = self._server.accept()
            except socket.timeout:
                continue
            except OSError:
                return
            threading.Thread(target=self._handle, args=(conn,), daemon=True).start()

    def _handle(self, client: socket.socket) -> None:
        try:
            upstream = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            upstream.connect(self.target_path)
        except OSError:
            client.close()
            return
        up = threading.Thread(target=self._pump, args=(client, upstream, True))
        down = threading.Thread(target=self._pump, args=(upstream, client, False))
        up.start()
        down.start()
        up.join()
        down.join()
        for sock in (client, upstream):
            try:
                sock.close()
            except OSError:
                pass

    def _pump(self, src: socket.socket, dst: socket.socket, record: bool) -> None:
        buffer = b""
        while True:
            try:
                chunk = src.recv(65536)
            except OSError:
                break
            if not chunk:
                break
            if record:
                # Line by line, each consumed once: re-scanning a rolling
                # buffer would count the same request as many times as a chunk
                # arrives after it.
                buffer += chunk
                while True:
                    cut = buffer.find(b"\n")
                    if cut == -1:
                        break
                    line, buffer = buffer[:cut], buffer[cut + 1 :]
                    match = REQUEST_LINE.match(line)
                    if match:
                        route = match.group(2).decode("ascii", "replace").split("?")[0]
                        key = f"{match.group(1).decode()} {route}"
                        with self._lock:
                            self.routes[key] = self.routes.get(key, 0) + 1
                if len(buffer) > 65536:
                    buffer = buffer[-4096:]
            try:
                dst.sendall(chunk)
            except OSError:
                break
        try:
            dst.shutdown(socket.SHUT_WR)
        except OSError:
            pass

    def snapshot(self) -> dict[str, int]:
        with self._lock:
            return dict(self.routes)


# ── Orchestration ──────────────────────────────────────────────────────────
def build_seed(dest: Path) -> Optional[str]:
    if not SEED_BUILDER.is_file():
        return f"seed builder not found at {SEED_BUILDER}"
    if shutil.which("sqlite3") is None:
        return "sqlite3 is required to build the seed fixture"
    proc = subprocess.run(
        ["bash", str(SEED_BUILDER), str(dest)],
        capture_output=True,
        text=True,
        timeout=300,
    )
    if proc.returncode != 0:
        return f"seed build failed: {proc.stdout[-800:]}{proc.stderr[-800:]}"
    return None


def partition(leaves: list[str], daemon: bool) -> tuple[list[str], dict, list[str]]:
    """(played, excluded, stale) against the policy tables."""
    table = dict(policy.EXCLUSIONS)
    if daemon:
        table.update(policy.DAEMON_EXCLUSIONS)
    known = set(leaves)
    stale = sorted(set(table) - known)
    excluded = {leaf: table[leaf] for leaf in table if leaf in known}
    played = [leaf for leaf in leaves if leaf not in excluded]
    return played, excluded, stale


def verdict(results: list[dict], stale: list[str]) -> int:
    """0 measured and clean, 1 a defect, 2 nothing measured.

    A 2 must never read as a success, which is why an empty result list is not
    allowed to fall through to 0: a sweep that played nothing has proved
    nothing about the command tree.
    """
    if not results:
        return 2
    if stale or any(r.get("defect") for r in results):
        return 1
    return 0


def render_markdown(report: dict) -> str:
    out = ["# CLI smoke sweep\n"]
    out.append(
        f"- binary: `{report['binary']}`\n"
        f"- mode: {report['mode']}\n"
        f"- leaves enumerated: {report['leaves_total']}\n"
        f"- **played: {report['played']}**, refused: {report['excluded_count']}\n"
        f"- defects: {report['defects']}\n"
        f"- wall: {report['wall_s']}s\n"
    )
    if report["stale_exclusions"]:
        out.append("\n## Stale exclusions (the policy names leaves that no longer exist)\n")
        out.extend(f"- `{leaf}`" for leaf in report["stale_exclusions"])
        out.append("")
    out.append("\n## Refused leaves, one reason each\n")
    out.append("| leaf | class | reason |")
    out.append("|---|---|---|")
    for leaf, (klass, reason) in sorted(report["excluded"].items()):
        out.append(f"| `{leaf}` | {klass} | {reason} |")
    defective = [r for r in report["results"] if r.get("defect")]
    out.append("\n## Defects\n")
    if not defective:
        out.append("_None: every played leaf held the exit, json and panic contracts._")
    for r in defective:
        out.append(f"- `{r['leaf']}` (exit {r.get('exit')}): {r['defect']}")
        out.append(f"  - command: `{r.get('command', '')}`")
    observed = [r for r in report["results"] if r.get("observation")]
    out.append(f"\n## Observations, not defects ({len(observed)})\n")
    out.extend(f"- `{r['leaf']}`: {r['observation']}" for r in observed)
    if report.get("routes"):
        out.append(
            f"\n## API measured through the CLI\n\n"
            f"- {len(report['routes'])} distinct request lines crossed the socket\n"
            f"- {len(report['operations'])} distinct operations once the arguments "
            f"the sweep passed are folded back into their path segments\n"
        )
        out.append("\n| operation | leaves that reached it |")
        out.append("|---|---|")
        for operation in sorted(report["operations"]):
            leaves = ", ".join(f"`{l}`" for l in report["operations"][operation])
            out.append(f"| `{operation}` | {leaves} |")
    return "\n".join(out) + "\n"


def run_sweep(
    bin_path: str,
    leaves: list[str],
    seed_ref: Path,
    workspace: Path,
    jobs: int,
    socket_path: Optional[str],
    probe: Optional["RouteProbe"] = None,
) -> list:
    paths = list(enumerate(leaf.split() for leaf in leaves))
    if socket_path is not None or jobs <= 1:
        # Serial: the daemon holds one HOME, and per-leaf route attribution is
        # a before/after snapshot, which only reads true one leaf at a time.
        return [
            play_leaf(bin_path, leaf, seed_ref, workspace, socket_path, i, probe)
            for i, leaf in paths
        ]
    with concurrent.futures.ThreadPoolExecutor(max_workers=jobs) as pool:
        futures = [
            pool.submit(play_leaf, bin_path, leaf, seed_ref, workspace, socket_path, i)
            for i, leaf in paths
        ]
        return [f.result() for f in futures]


def main(argv: Optional[List[str]] = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--bin", default=str(DEFAULT_BIN))
    ap.add_argument("--out", default=str(DEFAULT_OUT))
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--only", default=None, help="play only leaves with this prefix")
    ap.add_argument(
        "--daemon",
        action="store_true",
        help="boot a daemon on the seeded HOME and record the API routes crossing "
        "the socket",
    )
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()

    bin_path = str(Path(args.bin).resolve())
    if not os.access(bin_path, os.X_OK):
        print(
            f"REFUSING: {bin_path} is not executable. Build it with "
            f"`cargo build -p apollia-cli --bin apollia-os`. Nothing was measured.",
            file=sys.stderr,
        )
        return 2
    fresh = subprocess.run(
        [sys.executable, str(FRESHNESS), "--bin", bin_path, "--tree", str(REPO_ROOT)],
        capture_output=True,
        text=True,
    )
    if fresh.returncode != 0:
        print(fresh.stdout + fresh.stderr, file=sys.stderr)
        print("          Nothing was measured.", file=sys.stderr)
        return 2

    guard = load_guard()
    leaves = [" ".join(p) for p in guard.enumerate_leaves(bin_path)]
    if args.only:
        leaves = [l for l in leaves if l.startswith(args.only)]
    if not leaves:
        print("REFUSING: the command tree walk produced no leaf.", file=sys.stderr)
        return 2

    played, excluded, stale = partition(leaves, args.daemon)
    if not played:
        print("REFUSING: no leaf left to play after the policy table.", file=sys.stderr)
        return 2

    started = time.monotonic()
    try:
        workspace = Path(tempfile.mkdtemp(prefix="aps.", dir=WORKSPACE_ROOT))
    except OSError as exc:
        print(f"REFUSING: no workspace under {WORKSPACE_ROOT}: {exc}", file=sys.stderr)
        return 2
    probe_socket = workspace / "l999" / ".apollia" / "runtime.sock"
    if len(str(probe_socket).encode()) > SUN_PATH_MAX:
        shutil.rmtree(workspace, ignore_errors=True)
        print(
            f"REFUSING: a per-leaf runtime socket under {workspace} would be "
            f"{len(str(probe_socket))} bytes, over the {SUN_PATH_MAX}-byte sun_path "
            f"limit. Every daemon-dependent leaf would report a path-length io "
            f"error instead of its real refusal, and the sweep would measure the "
            f"path rather than the product. Set APOLLIA_SMOKE_TMP to a shorter "
            f"directory. Nothing was measured.",
            file=sys.stderr,
        )
        return 2
    routes: dict[str, int] = {}
    daemon_note = None
    try:
        seed_ref = workspace / "seed-ref"
        err = build_seed(seed_ref)
        if err:
            print(f"REFUSING: {err}", file=sys.stderr)
            print("          Nothing was measured.", file=sys.stderr)
            return 2

        socket_path = None
        daemon = None
        probe = None
        if args.daemon:
            socket_path, daemon, probe, daemon_note = boot_daemon(bin_path, seed_ref)
            if socket_path is None:
                print(f"REFUSING: {daemon_note}", file=sys.stderr)
                print("          Nothing was measured.", file=sys.stderr)
                return 2

        results = run_sweep(
            bin_path, played, seed_ref, workspace, args.jobs, socket_path, probe
        )

        if args.daemon:
            if probe:
                routes = probe.snapshot()
                probe.stop()
            if daemon:
                subprocess.run(
                    [bin_path, "--socket", socket_path, "stop"],
                    capture_output=True,
                    timeout=30,
                    env=child_env(seed_ref),
                )
                try:
                    daemon.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    daemon.kill()
    finally:
        shutil.rmtree(workspace, ignore_errors=True)

    defects = [r for r in results if r.get("defect")]
    operations: dict = {}
    for r in results:
        for operation in r.get("operations", []):
            operations.setdefault(operation, []).append(r["leaf"])
    report = {
        "binary": bin_path,
        "mode": "daemon" if args.daemon else "offline",
        "daemon_note": daemon_note,
        "leaves_total": len(leaves),
        "played": len(results),
        "excluded_count": len(excluded),
        "excluded": excluded,
        "stale_exclusions": stale,
        "defects": len(defects),
        "wall_s": round(time.monotonic() - started, 1),
        "routes": routes,
        "operations": operations,
        "results": results,
    }
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "smoke.json").write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    (out_dir / "smoke.md").write_text(render_markdown(report), encoding="utf-8")

    print(
        f"smoke sweep: {len(results)} leaves played of {len(leaves)} enumerated "
        f"({len(excluded)} refused by policy), {len(defects)} defects, "
        f"{report['wall_s']}s"
    )
    if routes:
        print(
            f"             {len(routes)} request lines on the wire, "
            f"{len(operations)} distinct API operations reached through the CLI"
        )
    for r in defects:
        print(f"  DEFECT  {r['leaf']}: {r['defect']}")
    for leaf in stale:
        print(f"  DEFECT  policy excludes `{leaf}`, which the binary no longer has")
    print(f"             report: {out_dir / 'smoke.md'}")
    return verdict(results, stale)


def boot_daemon(bin_path: str, seed_home: Path):
    """Start the daemon on the seeded HOME behind a route-recording proxy."""
    real_sock = str(seed_home / "runtime.sock")
    probe_sock = str(seed_home / "probe.sock")
    env = child_env(seed_home)
    log = open(seed_home / "daemon.log", "wb")
    daemon = subprocess.Popen(
        [bin_path, "start", "--socket", real_sock, "--port", "0"],
        stdout=log,
        stderr=subprocess.STDOUT,
        stdin=subprocess.DEVNULL,
        env=env,
        cwd=str(seed_home),
    )
    for _ in range(60):
        probe = subprocess.run(
            [bin_path, "--socket", real_sock, "status"],
            capture_output=True,
            timeout=20,
            env=env,
        )
        if probe.returncode == 0:
            break
        time.sleep(1)
    else:
        daemon.kill()
        tail = (seed_home / "daemon.log").read_text(errors="replace")[-800:]
        return None, None, None, f"the daemon never answered on its socket: {tail}"
    route_probe = RouteProbe(probe_sock, real_sock)
    route_probe.start()
    return probe_sock, daemon, route_probe, "daemon booted, routes recorded on the wire"


# ── Self-test: the engine, played against a stub built to break ────────────
STUB = r'''#!/usr/bin/env python3
import sys, time
LEAVES = {
    "clean": ("json", 0),
    "boom": ("panic", 101),
    "weird": ("outside", 7),
    "truncated": ("badjson", 0),
    "hang": ("sleep", 0),
}
args = [a for a in sys.argv[1:] if not a.startswith("-")]
if "--help" in sys.argv:
    if not args:
        print("Usage: stub [OPTIONS] <COMMAND>\n\nCommands:")
        for name in LEAVES:
            print(f"  {name}  a stub leaf")
        print("  help  Print this message")
        print("")
        sys.exit(0)
    print(f"Usage: stub {args[0]} [OPTIONS]")
    sys.exit(0)
kind, code = LEAVES.get(args[0] if args else "", ("json", 0))
if kind == "json":
    print('{"ok": true}')
elif kind == "panic":
    sys.stderr.write(
        "thread 'main' panicked at src/main.rs:1:1:\nboom\n"
        "note: run with `RUST_BACKTRACE=1` to display a backtrace\n"
    )
elif kind == "badjson":
    sys.stdout.write('{"ok": tru')
elif kind == "sleep":
    time.sleep(30)
sys.exit(code)
'''


def selftest() -> int:
    failures: list[str] = []

    def case(name: str, ok: bool, detail: str = "") -> None:
        print(f"  {'ok   ' if ok else 'FAIL '} {name}{'  ' + detail if detail else ''}")
        if not ok:
            failures.append(name)

    workspace = Path(tempfile.mkdtemp(prefix="apollia-cli-smoke-selftest."))
    try:
        stub = workspace / "stub-apollia-os"
        stub.write_text(STUB, encoding="utf-8")
        stub.chmod(stub.stat().st_mode | stat.S_IEXEC | stat.S_IXGRP | stat.S_IXOTH)

        guard = load_guard()
        leaves = [" ".join(p) for p in guard.enumerate_leaves(str(stub))]
        case(
            "the walk finds the stub's five leaves",
            sorted(leaves) == ["boom", "clean", "hang", "truncated", "weird"],
            str(sorted(leaves)),
        )

        seed_ref = workspace / "fake-home"
        (seed_ref / ".apollia").mkdir(parents=True)
        saved = policy.TIMEOUTS.get("hang")
        policy.TIMEOUTS["hang"] = 2
        results = run_sweep(str(stub), leaves, seed_ref, workspace, 1, None)
        if saved is None:
            policy.TIMEOUTS.pop("hang")
        else:
            policy.TIMEOUTS["hang"] = saved
        by_leaf = {r["leaf"]: r for r in results}

        case("a clean leaf yields no defect", not by_leaf["clean"].get("defect"))
        case(
            "a panicking leaf is caught",
            "panic marker" in (by_leaf["boom"].get("defect") or ""),
            by_leaf["boom"].get("defect", "")[:60],
        )
        case(
            "an exit outside the taxonomy is caught",
            "outside the documented taxonomy" in (by_leaf["weird"].get("defect") or ""),
        )
        case(
            "truncated JSON is caught",
            "does not parse" in (by_leaf["truncated"].get("defect") or ""),
        )
        case(
            "a leaf that never returns is caught",
            "did not return within" in (by_leaf["hang"].get("defect") or ""),
        )
        case(
            "the run as a whole is red",
            sum(1 for r in results if r.get("defect")) == 4,
        )
        clean_only = run_sweep(str(stub), ["clean"], seed_ref, workspace, 1, None)
        case(
            "the same engine is green on the clean leaf alone",
            not any(r.get("defect") for r in clean_only),
        )
        played, excluded, stale = partition(leaves, False)
        case(
            "a policy entry naming an absent leaf is reported stale",
            sorted(stale) == sorted(policy.EXCLUSIONS),
            f"{len(stale)} stale against this stub",
        )
        case("nothing is excluded on a tree the policy does not know", excluded == {})
        case("every stub leaf is therefore played", len(played) == 5)

        case("verdict: a clean run is 0", verdict(clean_only, []) == 0)
        case("verdict: a defect is 1", verdict(results, []) == 1)
        case("verdict: a stale exclusion alone is 1", verdict(clean_only, ["gone"]) == 1)
        case(
            "verdict: an empty run is 2, never 0",
            verdict([], []) == 2 and verdict([], ["gone"]) == 2,
        )
        absent = workspace / "no-such-binary"
        case(
            "a binary that cannot be run measures nothing (2)",
            main(["--bin", str(absent)]) == 2,
        )
        case(
            "a binary this tree did not produce measures nothing (2)",
            main(["--bin", str(stub)]) == 2,
        )
    finally:
        shutil.rmtree(workspace, ignore_errors=True)

    print(f"\nselftest: {len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
