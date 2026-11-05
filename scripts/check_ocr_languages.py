#!/usr/bin/env python3
"""Download every pinned language of reading and compare it to its pin.

The language data of optical character recognition is pinned by the `[[ocr]]`
entries of `crates/apollia-llm/src/recommend/pinned_models.toml`: a repository,
a commit, a file, a size and a SHA-256. The application composes the address
as `OcrLanguage::download_url` does (`recommend/ocr.rs`):
`<base>/<repo>/<commit>/<file>`, under the `REGISTRY_BASE` of
`crates/apollia-llm/src/ocr_data.rs`. The first address the registry pinned
died upstream and nothing said so; this check downloads each file once a week
so the next one cannot die in silence.

The base is read from `ocr_data.rs` itself, never restated here. The layout is
held to the Rust side by `--selftest`, which reads the expected address out of
the test of `recommend/ocr.rs` that pins the French one, and refuses a
composition of this script that differs from it.

Options:
  --table PATH  the pinned table to read (default: the shipped one)
  --base URL    the base to compose under (default: REGISTRY_BASE)
  --retries N   attempts after the first on a transient failure (default 4)
  --selftest    drive the check against altered copies of the table and a
                local `http.server`, without the network, and fail unless
                every alteration is caught

Exit codes:
  0  every pinned file downloaded and matched its size and SHA-256
  1  at least one file is missing upstream or differs from its pin, each
     printed with language, address, expected and obtained
  2  nothing was measured: the table or the base is unreadable, the table
     pins no language, or the network is unreachable
"""

import argparse
import contextlib
import functools
import hashlib
import http.server
import re
import shutil
import socket
import sys
import tempfile
import threading
import time
import tomllib
import urllib.error
import urllib.request
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
TABLE = REPO_ROOT / "crates/apollia-llm/src/recommend/pinned_models.toml"
OCR_DATA_RS = REPO_ROOT / "crates/apollia-llm/src/ocr_data.rs"
OCR_RS = REPO_ROOT / "crates/apollia-llm/src/recommend/ocr.rs"
RUST_TEST = "test_french_downloads_from_the_upstream_set_at_its_pinned_commit"

BASE_CONST = re.compile(r'pub const REGISTRY_BASE: &str = "([^"]+)";')
REQUIRED = ("language", "repo", "commit", "file", "size_bytes", "sha256")
# Nothing pinned weighs this much; a server that streams without end is cut.
MAX_BYTES = 64 * 1024 * 1024
CHUNK = 1024 * 1024
TIMEOUT_S = 60


class Unmeasured(Exception):
    """Nothing could be measured: a subject is unreadable or the network is down."""


def read_base(path: Path = OCR_DATA_RS) -> str:
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as e:
        raise Unmeasured(f"{path}: {e}") from e
    found = BASE_CONST.findall(text)
    if len(found) != 1:
        raise Unmeasured(f"{path}: expected one REGISTRY_BASE, found {len(found)}")
    return found[0]


def read_table(path: Path) -> list[dict]:
    try:
        with path.open("rb") as f:
            data = tomllib.load(f)
    except (OSError, tomllib.TOMLDecodeError) as e:
        raise Unmeasured(f"{path}: {e}") from e
    entries = data.get("ocr", [])
    if not isinstance(entries, list) or not entries:
        raise Unmeasured(f"{path}: no [[ocr]] entry, nothing to measure")
    for entry in entries:
        missing = [k for k in REQUIRED if k not in entry]
        if missing:
            raise Unmeasured(f"{path}: an [[ocr]] entry lacks {', '.join(missing)}")
    return entries


def compose(base: str, entry: dict) -> str:
    """The address `OcrLanguage::download_url` composes for this entry."""
    return f"{base.rstrip('/')}/{entry['repo']}/{entry['commit']}/{entry['file']}"


def fetch(url: str, dest: Path, retries: int) -> tuple[int, str] | str:
    """Download `url` into `dest`; return (size, sha256), or an HTTP refusal.

    A refusal (an HTTP status) is a defect of the address and comes back as a
    string. A failure to reach the host at all, after the retries, is
    `Unmeasured`.
    """
    last: Exception | None = None
    for attempt in range(retries + 1):
        if attempt:
            time.sleep(min(2**attempt, 30))
        try:
            with urllib.request.urlopen(url, timeout=TIMEOUT_S) as resp, dest.open("wb") as out:
                digest = hashlib.sha256()
                size = 0
                while chunk := resp.read(CHUNK):
                    size += len(chunk)
                    if size > MAX_BYTES:
                        return f"more than {MAX_BYTES} bytes served"
                    digest.update(chunk)
                    out.write(chunk)
                return size, digest.hexdigest()
        except urllib.error.HTTPError as e:
            if e.code < 500:
                return f"HTTP {e.code}"
            last = e
        except (urllib.error.URLError, OSError) as e:
            last = e
    raise Unmeasured(f"{url}: unreachable after {retries + 1} attempt(s): {last}")


def check(table: Path, base: str | None, retries: int) -> int:
    try:
        entries = read_table(table)
        base = base if base is not None else read_base()
        defects = 0
        with tempfile.TemporaryDirectory(prefix="ocr-languages-") as tmp:
            for entry in entries:
                lang = entry["language"]
                url = compose(base, entry)
                got = fetch(url, Path(tmp) / f"{lang}.download", retries)
                if isinstance(got, str):
                    defects += 1
                    print(f"KO {lang}: {url}: expected a file, got {got}")
                    continue
                size, sha = got
                if size != entry["size_bytes"]:
                    defects += 1
                    print(f"KO {lang}: {url}: size expected {entry['size_bytes']}, got {size}")
                if sha != entry["sha256"]:
                    defects += 1
                    print(f"KO {lang}: {url}: sha256 expected {entry['sha256']}, got {sha}")
                if size == entry["size_bytes"] and sha == entry["sha256"]:
                    print(f"ok {lang}")
    except Unmeasured as e:
        print(f"check_ocr_languages: nothing measured: {e}", file=sys.stderr)
        return 2
    return 1 if defects else 0


# --- selftest ---------------------------------------------------------------


def rust_expected_address() -> str:
    """The address the Rust test of `recommend/ocr.rs` pins for French."""
    text = OCR_RS.read_text(encoding="utf-8")
    start = text.index(f"fn {RUST_TEST}")
    body = text[start:]
    literal = re.search(r'assert_eq!\(\s*url,\s*"((?:[^"\\]|\\.)*)"', body, re.DOTALL)
    if literal is None:
        raise Unmeasured(f"{OCR_RS}: no assert_eq! on url in {RUST_TEST}")
    # A Rust string continuation: backslash, newline, then the leading blanks.
    return re.sub(r"\\\n\s*", "", literal.group(1))


def toml_entry(entry: dict) -> str:
    return (
        "[[ocr]]\n"
        f'language = "{entry["language"]}"\n'
        f'repo = "{entry["repo"]}"\n'
        f'commit = "{entry["commit"]}"\n'
        f'file = "{entry["file"]}"\n'
        f"size_bytes = {entry['size_bytes']}\n"
        f'sha256 = "{entry["sha256"]}"\n'
    )


@contextlib.contextmanager
def serve(root: Path):
    handler = functools.partial(_QuietHandler, directory=str(root))
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_address[1]}"
    finally:
        server.shutdown()
        server.server_close()


class _QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, format, *args):  # noqa: A002 - the stdlib signature
        pass


def closed_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def selftest() -> int:
    failures = 0

    def case(name: str, ok: bool, detail: str) -> None:
        nonlocal failures
        print(f"  {'ok' if ok else 'KO'} {name}" + ("" if ok else f": {detail}"))
        failures += 0 if ok else 1

    print("selftest: composition, then the check on a local server and altered tables")
    try:
        shipped = read_table(TABLE)
        base = read_base()
        french = next(e for e in shipped if e["language"] == "fra")
        want = rust_expected_address()
    except (Unmeasured, StopIteration, ValueError, OSError) as e:
        print(f"selftest: nothing measured: {e}", file=sys.stderr)
        return 2
    got = compose(base, french)
    case("the French address is the one the Rust test pins", got == want, f"{got} != {want}")

    with tempfile.TemporaryDirectory(prefix="ocr-languages-selftest-") as tmp:
        root = Path(tmp) / "srv"
        tables = Path(tmp) / "tables"
        tables.mkdir()
        fakes = []
        for lang, payload in (
            ("fra", b"fake french data\n"),
            ("eng", b"fake english data, longer\n"),
        ):
            entry = {
                "language": lang,
                "repo": "owner/tessdata",
                "commit": "0" * 40,
                "file": f"{lang}.traineddata",
                "size_bytes": len(payload),
                "sha256": hashlib.sha256(payload).hexdigest(),
            }
            served = root / entry["repo"] / entry["commit"] / entry["file"]
            served.parent.mkdir(parents=True, exist_ok=True)
            served.write_bytes(payload)
            fakes.append(entry)

        def write(name: str, entries: list[dict]) -> Path:
            path = tables / f"{name}.toml"
            path.write_text("\n".join(toml_entry(e) for e in entries), encoding="utf-8")
            return path

        def altered(**change) -> list[dict]:
            return [{**fakes[0], **change}, fakes[1]]

        unreadable = tables / "unreadable.toml"
        unreadable.write_text("[[ocr]\nlanguage = \n", encoding="utf-8")
        with serve(root) as url:
            subjects = (
                ("the intact table matches", write("intact", fakes), url, 0),
                ("a changed sha256 is caught", write("sha", altered(sha256="f" * 64)), url, 1),
                (
                    "a changed size is caught",
                    write("size", altered(size_bytes=fakes[0]["size_bytes"] + 1)),
                    url,
                    1,
                ),
                (
                    "an address the composition does not reproduce is caught",
                    write("commit", altered(commit="1" * 40)),
                    url,
                    1,
                ),
                (
                    "a base serving another layout is caught",
                    write("base", fakes),
                    url + "/elsewhere",
                    1,
                ),
                ("an unreadable table measures nothing", unreadable, url, 2),
                ("a table pinning no language measures nothing", write("empty", []), url, 2),
            )
            for name, table, base_url, expected in subjects:
                code = check(table, base_url, retries=0)
                case(name, code == expected, f"exit {code}, expected {expected}")
        code = check(write("down", fakes), f"http://127.0.0.1:{closed_port()}", retries=0)
        case("an unreachable host measures nothing", code == 2, f"exit {code}, expected 2")
        shutil.rmtree(root, ignore_errors=True)

    if failures:
        print(f"selftest: {failures} alteration(s) not caught")
        return 1
    print("selftest: every alteration caught")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--table", type=Path, default=TABLE)
    ap.add_argument("--base", default=None)
    ap.add_argument("--retries", type=int, default=4)
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        return selftest()
    return check(args.table, args.base, max(args.retries, 0))


if __name__ == "__main__":
    sys.exit(main())
