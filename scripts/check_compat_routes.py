#!/usr/bin/env python3
"""Hold the compatible server's routes against the page that documents them.

The runtime serves two HTTP surfaces. The native API lives under
`crates/apollia-runtime/src/api/` and `scripts/check_openapi_routes.py` holds it
against `clients/openapi.json`. The compatible server lives under
`crates/apollia-runtime/src/compat_server/`, outside that tree on purpose: its
paths follow the OpenAI and Anthropic formats (`/v1/...`) and belong in no
OpenAPI document of Apollia. What documents them is the table of
`docs/site/docs/reference/compatible-endpoints.md`, and its French mirror; this
guard is what keeps the two from drifting apart.

The served paths are the string literals of every `.route("...")` call in the
non-test Rust files of `compat_server/`. The documented paths are the second
column of the table that follows the `<!-- compat-endpoints -->` marker of each
page. Two directions, both defects:

  undocumented  a path the router serves that a page does not list. An
                application finds an endpoint the documentation never named.
  unserved      a path a page lists that the router does not serve. The
                documentation promises an endpoint that answers 404.

An empty table is a valid state, and the one this guard starts from: the server
serves no endpoint yet, and the page says so.

Exit codes:
  0  every served path is documented in both languages, and nothing more.
  1  a path is undocumented or unserved.
  2  nothing measured: the module, a page, or its marked table is absent.

Usage:
    python3 scripts/check_compat_routes.py
    python3 scripts/check_compat_routes.py --selftest
"""

import argparse
import re
import shutil
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
MODULE = Path("crates/apollia-runtime/src/compat_server")
PAGES = (
    Path("docs/site/docs/reference/compatible-endpoints.md"),
    Path("docs/site/i18n/fr/docusaurus-plugin-content-docs/current/reference/compatible-endpoints.md"),
)
MARKER = "<!-- compat-endpoints -->"

ROUTE = re.compile(r'\.route\(\s*"([^"]+)"')
CELL_PATH = re.compile(r"`([^`]+)`")


class NotMeasured(Exception):
    """A source the guard reads is absent."""


def served_paths(root: Path) -> set[str]:
    """The paths the compatible server's router mounts, tests excluded."""
    module = root / MODULE
    if not module.is_dir():
        raise NotMeasured(f"{MODULE} is absent")
    paths: set[str] = set()
    for source in sorted(module.rglob("*.rs")):
        if source.name == "tests.rs" or "tests" in source.relative_to(module).parts[:-1]:
            continue
        text = source.read_text(encoding="utf-8")
        # A `#[cfg(test)] mod tests { ... }` block is test code: cut the file there.
        cut = text.find("#[cfg(test)]\nmod tests {")
        if cut != -1:
            text = text[:cut]
        paths.update(ROUTE.findall(text))
    return paths


def documented_paths(page: Path) -> set[str]:
    """The paths of the table that follows the marker of `page`."""
    if not page.is_file():
        raise NotMeasured(f"{page} is absent")
    lines = page.read_text(encoding="utf-8").splitlines()
    try:
        start = lines.index(MARKER)
    except ValueError as exc:
        raise NotMeasured(f"{page} carries no {MARKER} marker") from exc
    rows = []
    for line in lines[start + 1 :]:
        stripped = line.strip()
        if not stripped and not rows:
            continue
        if not stripped.startswith("|"):
            break
        rows.append(stripped)
    if len(rows) < 2:
        raise NotMeasured(f"{page} has no table after its {MARKER} marker")
    paths: set[str] = set()
    for row in rows[2:]:
        cells = [cell.strip() for cell in row.strip("|").split("|")]
        if len(cells) < 2:
            continue
        match = CELL_PATH.search(cells[1])
        paths.add(match.group(1) if match else cells[1])
    return paths


def check(root: Path) -> tuple[int, list[str]]:
    """Returns the exit code and the lines to print."""
    try:
        served = served_paths(root)
        pages = {page: documented_paths(root / page) for page in PAGES}
    except NotMeasured as exc:
        return 2, [f"check_compat_routes: nothing measured, {exc}"]
    defects = []
    for page, documented in pages.items():
        for path in sorted(served - documented):
            defects.append(f"  undocumented  {path} is served and absent from {page}")
        for path in sorted(documented - served):
            defects.append(f"  unserved      {path} is listed in {page} and not served")
    summary = (
        f"check_compat_routes: {len(served)} path(s) served by {MODULE}, "
        f"{', '.join(str(len(d)) for d in pages.values())} listed by the "
        f"{len(pages)} pages"
    )
    if defects:
        return 1, [summary, *defects, f"check_compat_routes: {len(defects)} defect(s)"]
    return 0, [summary, "check_compat_routes: the router and the pages agree, path by path"]


# ── selftest ─────────────────────────────────────────────────────────────────


def _copy(root: Path) -> None:
    """Copies the module and the pages of this repository under `root`."""
    shutil.copytree(REPO_ROOT / MODULE, root / MODULE)
    for page in PAGES:
        (root / page).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(REPO_ROOT / page, root / page)


def _add_route(root: Path, path: str) -> None:
    router = root / MODULE / "router.rs"
    text = router.read_text(encoding="utf-8")
    text = text.replace(
        "Router::new().fallback(",
        f'Router::new().route("{path}", axum::routing::post(unknown_endpoint)).fallback(',
        1,
    )
    router.write_text(text, encoding="utf-8")


def _add_row(root: Path, page: Path, path: str) -> None:
    target = root / page
    text = target.read_text(encoding="utf-8").rstrip("\n")
    target.write_text(f"{text}\n| `POST` | `{path}` | OpenAI |\n", encoding="utf-8")


def selftest() -> int:
    failures = []

    def expect(name: str, root: Path, code: int, needle: str = "") -> None:
        got, lines = check(root)
        text = "\n".join(lines)
        if got != code or needle not in text:
            failures.append(f"{name}: expected {code} with {needle!r}, got {got}\n{text}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        _copy(root)
        expect("the tree as it is", root, 0)

        _add_route(root, "/v1/selftest")
        expect("a route no page lists", root, 1, "undocumented  /v1/selftest")

        _add_row(root, PAGES[0], "/v1/selftest")
        expect("a route the French page omits", root, 1, f"absent from {PAGES[1]}")

        _add_row(root, PAGES[1], "/v1/selftest")
        expect("a route both pages list", root, 0)

        _add_row(root, PAGES[0], "/v1/ghost")
        expect("a row no route serves", root, 1, "unserved      /v1/ghost")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        _copy(root)
        test_file = root / MODULE / "tests.rs"
        test_file.write_text(test_file.read_text(encoding="utf-8") + '\n// .route("/v1/only-in-tests"\n')
        expect("a route written in tests only", root, 0)

        page = root / PAGES[1]
        page.write_text(page.read_text(encoding="utf-8").replace(MARKER, ""), encoding="utf-8")
        expect("a page without its marker", root, 2, "nothing measured")

    with tempfile.TemporaryDirectory() as tmp:
        expect("an absent module", Path(tmp), 2, "nothing measured")

    for failure in failures:
        print(f"selftest FAILED: {failure}")
    if failures:
        return 1
    print("check_compat_routes --selftest: 8 assertions, every one holds")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Hold the compatible server's routes against the page that documents them."
    )
    parser.add_argument("--selftest", action="store_true", help="check the guard against modified copies")
    args = parser.parse_args()
    if args.selftest:
        return selftest()
    code, lines = check(REPO_ROOT)
    for line in lines:
        print(line)
    return code


if __name__ == "__main__":
    sys.exit(main())
