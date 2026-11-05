#!/usr/bin/env python3
"""Prove on a shipped bundle that a scanned page and a picture are read and found.

The smoke of the release asks `doctor --json` whether the recognition engine is
compiled in. That says the bundle carries Tesseract, not that it reads a page:
a build can announce recognition and never deliver it, through a language file
it cannot fetch, a folder it cannot name, or a page it stores unread. This
script drives the bundle the way a user does, through its launcher, under a
throwaway HOME, and judges what the runtime wrote.

One run, per case below, in its own folder under `--home`:

  1. a copy of `scanned-fr.pdf` (three pages: one picture each for the first
     two, two pictures for the third) and `recu.png` goes in a folder;
  2. the daemon starts from the launcher on a port the kernel assigned;
  3. the folder is added as a connection, then as a source;
  4. the index of the source is polled until the pass is over, within a
     named delay;
  5. the index must list no page `ocr_unavailable` and exactly one `not_read`
     (the page with two pictures);
  6. the full-text index of the source, opened with the `sqlite3` module of
     Python (the `sqlite3` binary is absent from the Windows image), must find
     "Besançon" in both documents;
  7. the language file on disk must have the digest the registry pins.

Cases:

  online             (default) the language is not on disk: the runtime
                     fetches it from the registry, then reads the pages again.
  offline, altered   (`--offline --language-file F`) the network is made
                     unreachable to the daemon through `HTTPS_PROXY` aimed at a
                     closed local port, which the binary honours without any
                     test code in it. `offline` copies F into the models folder
                     before the start: the pages are read. `altered` copies F
                     with one byte changed: the engine refuses it, the fetch
                     the runtime then attempts fails on the proxy and traces
                     `sources.indexer.ocr_data_unavailable`, and the pages stay
                     `ocr_unavailable`.

Exit codes, the contract of the guards:

  0  every assertion holds
  1  a defect: the bundle did not start, did not read, or read wrong
  2  nothing was measured: the launcher, a fixture or the registry is absent,
     the language file given is not the pinned one, or this Python has no FTS5

Usage:
    python3 scripts/release_ocr_e2e.py --launcher <bundle>/apollia-os.sh --home <tmp>
    python3 scripts/release_ocr_e2e.py --launcher <bundle>/apollia-os.sh --home <tmp> \\
        --offline --language-file <tmp>/fra.traineddata
    python3 scripts/release_ocr_e2e.py --selftest
"""

import argparse
import hashlib
import json
import os
import shutil
import socket
import sqlite3
import subprocess
import sys
import tempfile
import time
import tomllib
import urllib.error
import urllib.request
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
FIXTURES = REPO_ROOT / "crates" / "apollia-sources" / "tests" / "fixtures" / "ocr"
FIXTURE_NAMES = ("scanned-fr.pdf", "recu.png")
REGISTRY = REPO_ROOT / "crates" / "apollia-llm" / "src" / "recommend" / "pinned_models.toml"
LANGUAGE = "fra"
# Escaped so the file stays one language for check_file_language.py.
WORD = "Besan\u00e7on"

# How long the daemon may take to answer its health route after the launch.
START_TIMEOUT_S = 120.0
# How long one pass may take, from the creation of the source to its end. The
# indexer sweeps every 60 s, so a source created between two sweeps waits up
# to one; the online case then fetches the language and reads the pages again.
PASS_TIMEOUT_S = 420.0
# How long the altered case waits for the trace of the failed fetch.
TRACE_TIMEOUT_S = 120.0
# How long a stopped daemon may take to exit before it is killed.
STOP_TIMEOUT_S = 60.0
POLL_INTERVAL_S = 2.0

DONE_STATES = {"ready", "keywords_only", "error"}
FETCH_FAILED_TRACE = "sources.indexer.ocr_data_unavailable"


class Defect(Exception):
    """The bundle did something wrong: exit 1."""


class NotMeasured(Exception):
    """The run could not judge the bundle: exit 2."""


# ── Pure judgements, shared by the run and the self-test ──────────────────────


def sha256_of(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def pinned_sha256(registry: Path = REGISTRY, language: str = LANGUAGE) -> str:
    try:
        with registry.open("rb") as f:
            entries = tomllib.load(f).get("ocr", [])
    except (OSError, tomllib.TOMLDecodeError) as e:
        raise NotMeasured(f"the registry {registry} is unreadable: {e}") from e
    for entry in entries:
        if entry.get("language") == language:
            return str(entry["sha256"]).lower()
    raise NotMeasured(f"the registry {registry} pins no `{language}` language")


def unread_reasons(index: dict) -> list[str]:
    return [str(p.get("reason")) for p in index.get("unreadPages") or []]


def pass_is_over(index: dict, documents: int, expect_read: bool) -> bool:
    """Whether the pass that `index` reports is the one to judge.

    A pass is over once the state is a final one and every document is
    counted. When the pages are expected read, a page still waiting for
    recognition means the language is on its way and a second pass follows.
    """
    if index.get("indexState") not in DONE_STATES:
        return False
    if int(index.get("documents") or 0) < documents:
        return False
    if expect_read and "ocr_unavailable" in unread_reasons(index):
        return False
    return True


def wait_for_pass(fetch, documents: int, expect_read: bool, timeout: float,
                  clock=time.monotonic, pause=time.sleep) -> dict:
    """Polls `fetch()` until the pass is over, within `timeout` seconds."""
    deadline = clock() + timeout
    last: dict = {}
    while True:
        last = fetch()
        if pass_is_over(last, documents, expect_read):
            return last
        if clock() >= deadline:
            raise Defect(
                f"the pass was not over after {timeout:.0f} s (PASS_TIMEOUT_S); "
                f"last index: {json.dumps(last, ensure_ascii=False)}"
            )
        pause(POLL_INTERVAL_S)


def judge_read(index: dict) -> None:
    """The pages were read: none waits for recognition, one is not read."""
    if index.get("indexState") == "error":
        raise Defect(f"the index ended in error: {json.dumps(index, ensure_ascii=False)}")
    reasons = unread_reasons(index)
    waiting = reasons.count("ocr_unavailable")
    not_read = reasons.count("not_read")
    if waiting:
        raise Defect(f"{waiting} page(s) still `ocr_unavailable`: {index.get('unreadPages')}")
    if not_read != 1:
        raise Defect(
            f"expected one page `not_read` (the page with two pictures), got {not_read}: "
            f"{index.get('unreadPages')}"
        )


def judge_refused(index: dict) -> None:
    """The altered language was refused: the pictures wait for recognition."""
    waiting = unread_reasons(index).count("ocr_unavailable")
    if waiting < 3:
        raise Defect(
            "an altered language file must leave the two pictured pages and the "
            f"picture `ocr_unavailable`, got {waiting}: {index.get('unreadPages')}"
        )


def documents_with_word(db: Path, word: str) -> set[str]:
    """The paths of the documents whose chunks the FTS5 index matches `word` in."""
    if not db.is_file():
        raise Defect(f"no index at {db}")
    # Not `mode=ro`: the index is in WAL mode, and a read-only connection
    # cannot open its shared-memory file. The HOME is a throwaway.
    try:
        conn = sqlite3.connect(str(db))
    except sqlite3.Error as e:
        raise Defect(f"the index {db} cannot be opened: {e}") from e
    try:
        rows = conn.execute(
            "SELECT DISTINCT d.path FROM chunks_fts f "
            "JOIN chunks c ON c.id = f.rowid JOIN documents d ON d.id = c.document_id "
            "WHERE chunks_fts MATCH ?",
            ('"' + word.replace('"', '""') + '"',),
        ).fetchall()
    except sqlite3.OperationalError as e:
        if "fts5" in str(e):
            raise NotMeasured(f"this Python's sqlite3 has no FTS5: {e}") from e
        raise Defect(f"the index {db} cannot be searched: {e}") from e
    finally:
        conn.close()
    return {str(r[0]) for r in rows}


def judge_word(found: set[str], names: tuple[str, ...], word: str) -> None:
    missing = [n for n in names if not any(Path(p).name == n for p in found)]
    if missing:
        raise Defect(f"the full-text index does not find {word!r} in {', '.join(missing)}")


def judge_language(path: Path, pinned: str) -> None:
    if not path.is_file():
        raise Defect(f"no language file at {path}")
    digest = sha256_of(path)
    if digest != pinned:
        raise Defect(f"{path} has sha256 {digest}, the registry pins {pinned}")


def log_has(log: Path, needle: str) -> bool:
    try:
        return needle in log.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return False


# ── The run on the bundle ──────────────────────────────────────────────────────


def free_port() -> int:
    """A port the kernel assigns, released for the daemon to bind."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


class Daemon:
    """One daemon started from the launcher under `home`, on a free port."""

    def __init__(self, launcher: Path, home: Path, extra_env: dict[str, str]):
        self.launcher = launcher
        self.home = home
        self.data_dir = home / ".apollia"
        self.socket = home / "s.sock"
        self.port = free_port()
        self.log = home / "daemon.log"
        self.env = dict(os.environ)
        self.env.update({"HOME": str(home), "USERPROFILE": str(home)})
        self.env.update(extra_env)
        self.proc: subprocess.Popen | None = None
        self.token = ""
        # The script's own calls go to the loopback, never through a proxy.
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    def command(self, *args: str) -> list[str]:
        return [str(self.launcher), *args]

    def start(self) -> None:
        with self.log.open("wb") as out:
            self.proc = subprocess.Popen(
                self.command("start", "--socket", str(self.socket), "--port", str(self.port)),
                stdout=out, stderr=subprocess.STDOUT, env=self.env, cwd=str(self.home),
            )
        deadline = time.monotonic() + START_TIMEOUT_S
        while time.monotonic() < deadline:
            if self.proc.poll() is not None:
                raise Defect(f"the daemon exited with {self.proc.returncode}:\n{self.tail()}")
            try:
                with self.opener.open(f"{self.base}/api/v1/health", timeout=5) as r:
                    if r.status == 200:
                        break
            except (urllib.error.URLError, OSError):
                pass
            time.sleep(POLL_INTERVAL_S)
        else:
            raise Defect(f"no health answer after {START_TIMEOUT_S:.0f} s:\n{self.tail()}")
        token = self.data_dir / "api-token"
        try:
            self.token = token.read_text(encoding="utf-8").strip()
        except OSError as e:
            raise Defect(f"the daemon wrote no token at {token}: {e}") from e

    @property
    def base(self) -> str:
        return f"http://127.0.0.1:{self.port}"

    def call(self, method: str, route: str, body: dict | None = None) -> dict:
        data = None if body is None else json.dumps(body).encode("utf-8")
        req = urllib.request.Request(f"{self.base}{route}", data=data, method=method)
        req.add_header("Authorization", f"Bearer {self.token}")
        if data is not None:
            req.add_header("Content-Type", "application/json")
        try:
            with self.opener.open(req, timeout=30) as r:
                return json.loads(r.read().decode("utf-8"))
        except urllib.error.HTTPError as e:
            detail = e.read().decode("utf-8", errors="replace")
            raise Defect(f"{method} {route} answered {e.code}: {detail}") from e
        except (urllib.error.URLError, OSError, json.JSONDecodeError) as e:
            raise Defect(f"{method} {route} failed: {e}") from e

    def tail(self, lines: int = 40) -> str:
        try:
            text = self.log.read_text(encoding="utf-8", errors="replace")
        except OSError:
            return "(no daemon log)"
        return "\n".join(text.splitlines()[-lines:])

    def stop(self) -> None:
        if self.proc is None or self.proc.poll() is not None:
            return
        try:
            subprocess.run(
                self.command("stop", "--socket", str(self.socket)),
                env=self.env, cwd=str(self.home), stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL, timeout=STOP_TIMEOUT_S, check=False,
            )
        except subprocess.TimeoutExpired:
            pass
        try:
            self.proc.wait(timeout=STOP_TIMEOUT_S)
        except subprocess.TimeoutExpired:
            if os.name == "nt":
                # The launcher is a batch file: killing its cmd.exe would
                # leave the daemon holding the pipe the next case needs.
                subprocess.run(["taskkill", "/T", "/F", "/PID", str(self.proc.pid)],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                               check=False)
            else:
                self.proc.kill()
            self.proc.wait(timeout=STOP_TIMEOUT_S)


def closed_port_proxy() -> dict[str, str]:
    """Proxy variables aimed at a loopback port nothing listens on."""
    proxy = f"http://127.0.0.1:{free_port()}"
    return {
        "HTTPS_PROXY": proxy, "https_proxy": proxy,
        "HTTP_PROXY": proxy, "http_proxy": proxy,
        "ALL_PROXY": proxy, "all_proxy": proxy,
        "NO_PROXY": "127.0.0.1,localhost", "no_proxy": "127.0.0.1,localhost",
    }


def run_case(name: str, launcher: Path, root: Path, language: bytes | None,
             offline: bool, pinned: str) -> None:
    home = root / name
    home.mkdir(parents=True)
    docs = home / "docs"
    docs.mkdir()
    for fixture in FIXTURE_NAMES:
        shutil.copy2(FIXTURES / fixture, docs / fixture)
    tessdata = home / ".apollia" / "models" / "tessdata"
    lang_file = tessdata / f"{LANGUAGE}.traineddata"
    if language is not None:
        tessdata.mkdir(parents=True)
        lang_file.write_bytes(language)
    expect_read = name != "altered"
    daemon = Daemon(launcher, home, closed_port_proxy() if offline else {})
    print(f"== {name}: daemon on 127.0.0.1:{daemon.port}, HOME {home}")
    try:
        daemon.start()
        connection = daemon.call(
            "POST", "/api/v1/connections/folders", {"path": str(docs), "label": "Scans"}
        )
        source = daemon.call("POST", "/api/v1/sources", {
            "kind": "folder", "label": "Scans", "reference": connection["id"],
        })
        sid = str(source["id"])
        print(f"   source {sid} on connection {connection['id']}")
        index = wait_for_pass(
            lambda: daemon.call("GET", f"/api/v1/sources/{sid}/index"),
            len(FIXTURE_NAMES), expect_read, PASS_TIMEOUT_S,
        )
        print(f"   index: {index.get('indexState')}, {index.get('documents')} document(s), "
              f"unread {unread_reasons(index)}")
        if expect_read:
            judge_read(index)
            found = documents_with_word(daemon.data_dir / "sources" / f"{sid}.db", WORD)
            judge_word(found, FIXTURE_NAMES, WORD)
            print(f"   FTS5 finds {WORD!r} in {sorted(Path(p).name for p in found)}")
            judge_language(lang_file, pinned)
            print(f"   {lang_file.name} has the pinned sha256")
        else:
            deadline = time.monotonic() + TRACE_TIMEOUT_S
            while not log_has(daemon.log, FETCH_FAILED_TRACE):
                if time.monotonic() >= deadline:
                    raise Defect(
                        f"no `{FETCH_FAILED_TRACE}` trace after {TRACE_TIMEOUT_S:.0f} s: "
                        f"the fetch was not attempted, or not refused:\n{daemon.tail()}"
                    )
                time.sleep(POLL_INTERVAL_S)
            index = daemon.call("GET", f"/api/v1/sources/{sid}/index")
            judge_refused(index)
            if lang_file.is_file() and sha256_of(lang_file) == pinned:
                raise Defect(f"{lang_file} has the pinned digest: the network was reached")
            print(f"   `{FETCH_FAILED_TRACE}` traced; pages still waiting: "
                  f"{unread_reasons(index).count('ocr_unavailable')}")
        if offline and log_has(daemon.log, "ocr.data.fetched"):
            raise Defect("the language was fetched although the network was unreachable")
        print(f"== {name}: ok")
    except Defect:
        print(f"== {name}: daemon log (tail)\n{daemon.tail()}", file=sys.stderr)
        raise
    finally:
        daemon.stop()


def run(args: argparse.Namespace) -> int:
    # Each daemon starts with its own HOME as working directory: a launcher
    # named relative to the checkout would not be found from there.
    launcher = Path(args.launcher).resolve()
    if not launcher.is_file():
        raise NotMeasured(f"no launcher at {launcher}")
    for fixture in FIXTURE_NAMES:
        if not (FIXTURES / fixture).is_file():
            raise NotMeasured(f"no fixture at {FIXTURES / fixture}")
    pinned = pinned_sha256()
    root = Path(args.home).resolve()
    root.mkdir(parents=True, exist_ok=True)
    if not args.offline:
        run_case("online", launcher, root, None, False, pinned)
        return 0
    if not args.language_file:
        raise NotMeasured("--offline needs --language-file")
    source = Path(args.language_file).resolve()
    if not source.is_file() or sha256_of(source) != pinned:
        raise NotMeasured(f"{source} is not the `{LANGUAGE}` language file the registry pins")
    good = source.read_bytes()
    altered = bytes([good[0] ^ 0x01]) + good[1:]
    run_case("offline", launcher, root, good, True, pinned)
    run_case("altered", launcher, root, altered, True, pinned)
    return 0


# ── Self-test: every judgement catches its case, without a binary ─────────────


def _fake_fts(path: Path, texts: dict[str, str]) -> None:
    conn = sqlite3.connect(path)
    conn.executescript(
        "CREATE TABLE documents (id INTEGER PRIMARY KEY, path TEXT);"
        "CREATE TABLE chunks (id INTEGER PRIMARY KEY, document_id INTEGER, text TEXT);"
        "CREATE VIRTUAL TABLE chunks_fts USING fts5 (text, content = 'chunks', "
        "content_rowid = 'id', tokenize = 'unicode61 remove_diacritics 2');"
    )
    for i, (doc, text) in enumerate(texts.items(), start=1):
        conn.execute("INSERT INTO documents VALUES (?, ?)", (i, f"/x/{doc}"))
        conn.execute("INSERT INTO chunks VALUES (?, ?, ?)", (i, i, text))
        conn.execute("INSERT INTO chunks_fts (rowid, text) VALUES (?, ?)", (i, text))
    conn.commit()
    conn.close()


def _index(state: str, reasons: list[str], documents: int = 2) -> dict:
    pages = [{"path": "/x/scanned-fr.pdf", "page": i + 1, "reason": r}
             for i, r in enumerate(reasons)]
    return {"indexState": state, "documents": documents, "unreadPages": pages}


def _replay(responses: list[dict]):
    calls = {"n": 0}

    def fetch() -> dict:
        i = min(calls["n"], len(responses) - 1)
        calls["n"] += 1
        return responses[i]
    return fetch, calls


def _clock():
    now = {"t": 0.0}
    return (lambda: now["t"]), (lambda s: now.__setitem__("t", now["t"] + s))


def _raises(kind, fn) -> bool:
    try:
        fn()
    except kind:
        return True
    return False


def selftest() -> int:
    cases: list[tuple[str, bool]] = []
    read = _index("keywords_only", ["not_read"])

    fetch, calls = _replay([read])
    clock, pause = _clock()
    got = wait_for_pass(fetch, 2, True, 10, clock, pause)
    cases.append(("index without waiting: judged at the first answer",
                  got is read and calls["n"] == 1))

    waiting = [_index("indexing", [], 0), _index("keywords_only", ["ocr_unavailable"] * 3
                                                + ["not_read"]), read]
    fetch, calls = _replay(waiting)
    clock, pause = _clock()
    got = wait_for_pass(fetch, 2, True, 60, clock, pause)
    cases.append(("index with waiting: judged once the pages are read",
                  got is read and calls["n"] == 3))

    fetch, _ = _replay([_index("indexing", [], 0)])
    clock, pause = _clock()
    cases.append(("a pass that never ends is a defect, within the delay",
                  _raises(Defect, lambda: wait_for_pass(fetch, 2, True, 10, clock, pause))))

    fetch, calls = _replay([_index("keywords_only", ["ocr_unavailable"] * 3 + ["not_read"])])
    clock, pause = _clock()
    got = wait_for_pass(fetch, 2, False, 10, clock, pause)
    cases.append(("a refused language is judged without waiting for a read",
                  calls["n"] == 1 and not _raises(Defect, lambda: judge_refused(got))))

    cases.append(("a page left `ocr_unavailable` is a defect",
                  _raises(Defect, lambda: judge_read(_index("ready", ["ocr_unavailable",
                                                                       "not_read"])))))
    cases.append(("no page `not_read` is a defect",
                  _raises(Defect, lambda: judge_read(_index("ready", [])))))
    cases.append(("two pages `not_read` are a defect",
                  _raises(Defect, lambda: judge_read(_index("ready", ["not_read"] * 2)))))
    cases.append(("an index in error is a defect",
                  _raises(Defect, lambda: judge_read(_index("error", ["not_read"])))))
    cases.append(("the expected index passes", not _raises(Defect, lambda: judge_read(read))))
    cases.append(("a refused language that left pages read is a defect",
                  _raises(Defect, lambda: judge_refused(read))))

    with tempfile.TemporaryDirectory(prefix="ocr-e2e-selftest-") as tmp:
        tmpdir = Path(tmp)
        present = tmpdir / "present.db"
        absent = tmpdir / "absent.db"
        try:
            _fake_fts(present, {"scanned-fr.pdf": f"Receipt from {WORD}, first page",
                                "recu.png": "BESANCON, May 3"})
            _fake_fts(absent, {"scanned-fr.pdf": "Receipt from Dijon", "recu.png": WORD})
        except sqlite3.OperationalError as e:
            print(f"release_ocr_e2e: this Python's sqlite3 has no FTS5 ({e}); "
                  "nothing measured", file=sys.stderr)
            return 2
        found = documents_with_word(present, WORD)
        cases.append(("the word found in both documents passes",
                      not _raises(Defect, lambda: judge_word(found, FIXTURE_NAMES, WORD))))
        found = documents_with_word(absent, WORD)
        cases.append(("the word absent from one document is a defect",
                      _raises(Defect, lambda: judge_word(found, FIXTURE_NAMES, WORD))))

        lang = tmpdir / "fra.traineddata"
        lang.write_bytes(b"language data")
        good = sha256_of(lang)
        cases.append(("the pinned digest passes",
                      not _raises(Defect, lambda: judge_language(lang, good))))
        lang.write_bytes(b"language datb")
        cases.append(("a file altered by one byte is a defect",
                      _raises(Defect, lambda: judge_language(lang, good))))
        cases.append(("a missing language file is a defect",
                      _raises(Defect, lambda: judge_language(tmpdir / "none", good))))

        log = tmpdir / "daemon.log"
        log.write_text(f"WARN error=proxy {FETCH_FAILED_TRACE}\n", encoding="utf-8")
        cases.append(("the trace of the failed fetch is seen in the log",
                      log_has(log, FETCH_FAILED_TRACE)
                      and not log_has(log, "ocr.data.fetched")))

        registry = tmpdir / "pinned_models.toml"
        registry.write_text('[[ocr]]\nlanguage = "eng"\nsha256 = "00"\n', encoding="utf-8")
        cases.append(("a registry without the language measures nothing",
                      _raises(NotMeasured, lambda: pinned_sha256(registry))))
    cases.append(("the registry of the tree pins the language",
                  len(pinned_sha256()) == 64))

    failed = [label for label, ok in cases if not ok]
    for label, ok in cases:
        print(f"  {'ok  ' if ok else 'FAIL'} {label}")
    if failed:
        print(f"release_ocr_e2e --selftest: {len(failed)} case(s) not caught", file=sys.stderr)
        return 1
    print(f"release_ocr_e2e --selftest: {len(cases)} case(s) caught")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--launcher", help="the launcher of the assembled bundle")
    ap.add_argument("--home", help="a throwaway folder, one HOME per case under it")
    ap.add_argument("--offline", action="store_true",
                    help="the network unreachable; the language copied before the start")
    ap.add_argument("--language-file", help="the pinned language file, for --offline")
    ap.add_argument("--selftest", action="store_true", help="judge simulated answers")
    args = ap.parse_args()
    if args.selftest:
        return selftest()
    if not args.launcher or not args.home:
        ap.error("--launcher and --home are required outside --selftest")
    try:
        return run(args)
    except NotMeasured as e:
        print(f"release_ocr_e2e: nothing measured: {e}", file=sys.stderr)
        return 2
    except Defect as e:
        print(f"release_ocr_e2e: FAIL {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
