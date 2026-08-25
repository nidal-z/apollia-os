#!/usr/bin/env python3
"""Every HTTP client and every response body goes through `apollia_core::net`.

Two rules used to live in three copies each, and both were missing wherever the
copy had not been made.

  - The SSRF policy. `crates/apollia-tools/src/ssrf.rs` and
    `crates/apollia-desktop/src/commands/ssrf.rs` carried the same 80 lines,
    and `crates/apollia-runtime/src/hooks/executor.rs` carried neither: it
    built a bare `reqwest::Client` and POSTed an agent's tool arguments to an
    operator-configured URL, which `docs/agents/SECURITY.md` states cannot
    happen.
  - The body cap. Nineteen call sites across seven files read a whole remote
    answer into memory with `.text()`, `.bytes()` or `.json()`, while six other
    files each carried their own copy of the same capped-read loop.

A rule stated in a document and applied by hand had produced both. This check
applies them mechanically instead.

The two rules:

  1. A `reqwest::Client::new()` or `reqwest::Client::builder()` outside
     `crates/apollia-core/src/net.rs` is refused. The two ways to build one are
     `apollia_core::net::safe_client{,_builder}()`, which carries the SSRF
     redirect policy, and `configured_endpoint_client_builder()`, the named
     exception for an endpoint the operator declared and which may legitimately
     be internal (a local MCP server, the runner, a self-hosted LLM).
  2. An awaited `.text()`, `.bytes()`, `.json()`, `.chunk()` or a
     `.bytes_stream()` on a response, in a file that uses `reqwest`, is refused
     outside that module. The bodies are read through
     `apollia_core::net::read_capped_{bytes,text,json}`.

Both rules are waived by a `// SAFETY:` comment in the six lines above the
site, which is how a genuine exception (an open-ended event stream, a
policy-equivalent fallback) stays visible and countable.

Test code is out of scope: a `#[cfg(test)]` module, a `tests/` directory, and
any file under `crates/*/tests/`. The scope is what ships.

Options:
  --selftest  run both rules against fixtures, in both directions, and fail
              unless every planted defect is caught and every compliant
              fixture passes.

Exit 0 when every site is compliant, 1 when a site is not, 2 when nothing
could be measured (which is not a pass).
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

# The module that implements both rules, and is therefore the one place allowed
# to build a client and to consume a body.
HELPER_FILE = "crates/apollia-core/src/net.rs"

# Building a reqwest client directly.
CLIENT = re.compile(r"\breqwest::Client::(?:new|builder)\s*\(")

# Consuming a response body: the call takes no argument and is awaited. The
# empty parentheses matter: `.json(&body)` on a request builder sets a payload
# and is not a body read. The `.await` matters too: `String::bytes()` and
# `scraper`'s `.text()` are synchronous iterators over local data.
CONSUMER = re.compile(
    r"\.\s*(?:text|bytes|json|chunk|text_with_charset)\s*(?:::<[^;{}]*?>)?\s*\(\s*\)\s*\.\s*await"
    r"|\.\s*bytes_stream\s*\(\s*\)"
)

# A named exemption, searched in the lines above the site.
SAFETY = re.compile(r"//\s*SAFETY:")
SAFETY_WINDOW = 6


def tracked_rust_files() -> list[str]:
    """Tracked production Rust files under `crates/`, tests excluded.

    Reading the index rather than walking the disk keeps the set identical in a
    developer's tree and in a fresh extraction of the same commit.
    """
    out = subprocess.run(
        ["git", "ls-files", "--", "crates"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if out.returncode != 0:
        return []
    files = []
    for line in out.stdout.split():
        if not line.endswith(".rs"):
            continue
        parts = line.split("/")
        if "tests" in parts or "/ui/" in line:
            continue
        files.append(line)
    return sorted(files)


def strip_test_modules(text: str) -> str:
    """Blank out every `#[cfg(test)]` item, matching braces rather than proximity.

    The naive form (truncate at the first `#[cfg(test)]`) drops production code
    from the sweep whenever the first occurrence is an inline attribute early in
    a long file, and that is exactly how the instrument this check replaces
    reported a clean file that carried an unbounded read 600 lines further down.
    """
    out = list(text)
    for match in re.finditer(r"#\[cfg\(test\)\]", text):
        brace = text.find("{", match.end())
        if brace < 0:
            continue
        # A `#[cfg(test)]` on a `use` or a `const` has no block of its own:
        # only follow the brace when it opens before the statement ends.
        semicolon = text.find(";", match.end())
        if 0 <= semicolon < brace:
            for i in range(match.start(), semicolon + 1):
                out[i] = "\n" if text[i] == "\n" else " "
            continue
        depth = 0
        i = brace
        while i < len(text):
            if text[i] == "{":
                depth += 1
            elif text[i] == "}":
                depth -= 1
                if depth == 0:
                    break
            i += 1
        for j in range(match.start(), min(i + 1, len(text))):
            out[j] = "\n" if text[j] == "\n" else " "
    return "".join(out)


def exempted(lines: list[str], index: int) -> bool:
    """True when a `// SAFETY:` sits within the window above line `index`."""
    start = max(0, index - SAFETY_WINDOW)
    return any(SAFETY.search(lines[i]) for i in range(start, index + 1))


def scan_text(rel: str, text: str) -> list[str]:
    """Both rules over one file's production text."""
    findings: list[str] = []
    if rel == HELPER_FILE:
        return findings
    production = strip_test_modules(text)
    lines = production.splitlines()
    uses_reqwest = "reqwest" in production

    for index, line in enumerate(lines):
        if CLIENT.search(line) and not exempted(lines, index):
            findings.append(
                f"{rel}:{index + 1}: builds a reqwest client outside apollia_core::net"
                f" - {line.strip()}"
            )

    if not uses_reqwest:
        return findings

    for index, line in enumerate(lines):
        if CONSUMER.search(line) and not exempted(lines, index):
            findings.append(
                f"{rel}:{index + 1}: reads a response body outside read_capped_*"
                f" - {line.strip()}"
            )
    return findings


def scan() -> tuple[list[str], int, int]:
    findings: list[str] = []
    files = tracked_rust_files()
    scanned = 0
    for rel in files:
        path = REPO_ROOT / rel
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        scanned += 1
        findings.extend(scan_text(rel, text))
    return findings, scanned, len(files)


# ─── Selftest ────────────────────────────────────────────────────────────────

DIRTY_CLIENT = """
fn build() -> reqwest::Client {
    reqwest::Client::builder().build().unwrap()
}
"""

CLEAN_CLIENT = """
fn build() -> reqwest::Client {
    apollia_core::net::safe_client_builder().build().unwrap()
}
"""

DIRTY_BODY = """
async fn read(resp: reqwest::Response) -> String {
    resp.text().await.unwrap_or_default()
}
"""

CLEAN_BODY = """
async fn read(resp: reqwest::Response) -> String {
    apollia_core::net::read_capped_text(resp, 1024).await.unwrap_or_default()
}
"""

WAIVED_BODY = """
async fn read(resp: reqwest::Response) -> String {
    // SAFETY: the caller needs the raw stream, the cap is applied downstream.
    resp.text().await.unwrap_or_default()
}
"""

TEST_ONLY_BODY = """
#[cfg(test)]
mod tests {
    async fn read(resp: reqwest::Response) -> String {
        resp.text().await.unwrap_or_default()
    }
}
"""

PRODUCTION_AFTER_TEST_MODULE = """
#[cfg(test)]
mod early {
    fn nothing() {}
}

async fn read(resp: reqwest::Response) -> String {
    resp.bytes().await.map(|b| b.len().to_string()).unwrap_or_default()
}
"""

REQUEST_BUILDER_JSON = """
async fn post(client: &reqwest::Client, body: &serde_json::Value) {
    let _ = client.post("https://example.com").json(body).send().await;
}
"""

SYNCHRONOUS_TEXT = """
fn count(node: &scraper::ElementRef, s: &str) -> usize {
    // reqwest is named in this file, so the rule is active on it.
    node.text().collect::<String>().len() + s.bytes().len()
}
"""

SELFTEST_CASES = [
    ("a bare client builder is refused", DIRTY_CLIENT, 1),
    ("the shared builder passes", CLEAN_CLIENT, 0),
    ("an unbounded body read is refused", DIRTY_BODY, 1),
    ("a capped body read passes", CLEAN_BODY, 0),
    ("a named SAFETY waives the body read", WAIVED_BODY, 0),
    ("a read inside a test module is out of scope", TEST_ONLY_BODY, 0),
    (
        "production after an early test module is still scanned",
        PRODUCTION_AFTER_TEST_MODULE,
        1,
    ),
    ("a request-builder .json(body) is not a body read", REQUEST_BUILDER_JSON, 0),
    ("synchronous .text()/.bytes() are not body reads", SYNCHRONOUS_TEXT, 0),
]


def selftest() -> int:
    failures = 0
    for label, fixture, expected in SELFTEST_CASES:
        found = scan_text("crates/apollia-fixture/src/lib.rs", fixture)
        ok = len(found) == expected
        print(f"  {'ok  ' if ok else 'FAIL'} {label}")
        if not ok:
            failures += 1
            print(
                f"       expected {expected} finding(s), got {len(found)}: {found!r}",
                file=sys.stderr,
            )

    # The helper module itself is exempt, and that exemption must be the file
    # path rather than anything in its content.
    helper = scan_text(HELPER_FILE, DIRTY_CLIENT + DIRTY_BODY)
    ok = helper == []
    print(f"  {'ok  ' if ok else 'FAIL'} the helper module is exempt from both rules")
    if not ok:
        failures += 1

    elsewhere = scan_text("crates/apollia-other/src/net.rs", DIRTY_CLIENT + DIRTY_BODY)
    ok = len(elsewhere) == 2
    print(f"  {'ok  ' if ok else 'FAIL'} another file named net.rs is not exempt")
    if not ok:
        failures += 1
        print(f"       got {elsewhere!r}", file=sys.stderr)

    if failures:
        print(f"\n{failures} selftest case(s) failed", file=sys.stderr)
        return 1
    print(f"{len(SELFTEST_CASES) + 2} selftest case(s) passed")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--selftest",
        action="store_true",
        help="run both rules against fixtures instead of against the tree",
    )
    args = parser.parse_args()

    if args.selftest:
        print("check_http_clients selftest")
        return selftest()

    findings, scanned, listed = scan()
    if listed == 0:
        print(
            "no tracked Rust file under crates/ could be listed; nothing was "
            "measured, which is not the same as no defect",
            file=sys.stderr,
        )
        return 2

    print(f"{scanned} production Rust file(s) scanned under crates/")
    if not findings:
        print(
            "every HTTP client and every response body goes through "
            "apollia_core::net"
        )
        return 0

    print(
        f"\n{len(findings)} site(s) bypass apollia_core::net.\n"
        "Build clients with apollia_core::net::safe_client{,_builder}(), or with\n"
        "configured_endpoint_client_builder() for an operator-declared endpoint\n"
        "that may legitimately be internal. Read bodies with\n"
        "apollia_core::net::read_capped_{bytes,text,json}. A genuine exception\n"
        "carries a `// SAFETY:` comment above the site saying why.\n",
        file=sys.stderr,
    )
    for finding in findings:
        print(f"  {finding}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
