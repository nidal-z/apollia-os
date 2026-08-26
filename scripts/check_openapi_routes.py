#!/usr/bin/env python3
"""Fail when the committed OpenAPI spec and the routes the runtime serves diverge.

`clients/openapi.json` is the driving contract. Three things are generated from
it and nothing else: the TypeScript types under `clients/ts`, the Python client
under `clients/python`, and the HTTP reference of the documentation site, which
`docs/site/regen.sh` builds through `docusaurus-plugin-openapi-docs`. A route
the runtime serves but the spec omits is therefore invisible to every consumer
the repository publishes, and a path the spec declares but no router registers
is a promise the daemon answers with 404.

The spec is assembled by `utoipa` from the `#[utoipa::path]` attributes on the
handlers, so it cannot drift from the *types* a handler serializes. It can and
did drift from the *set of routes*: `POST /api/v1/notifications/channels/:id/test`
was registered in the root router while its handler carried an annotation for
`/api/v1/notifications/test` only, and nothing measured the two sets against
each other. Sixty-eight lines of `.route(...)` and eighty-nine spec paths were
kept in step by review alone.

What is compared: the `(method, path)` pairs. Paths are normalised on both
sides, axum's `:id` and OpenAPI's `{id}` becoming the same token, so a rename of
a path parameter is not reported as a divergence while a rename of a segment is.
Comparing methods as well as paths is what makes the rule bite: a handler
annotated `get` on a path the router serves with `post` would otherwise pass.

Where the routes are read: `crates/apollia-runtime/src/api/`, `server/router.rs`
and the `routes_*` modules, their submodules included. That directory holds the
whole `/api/v1` router; the
other `.route(` call sites in the workspace (`apollia-auth` callback servers,
the MCP transports, the `apollia-runner` sidecar) serve their own local
protocols and are not part of the driving contract. Test modules are cut before
the scan, otherwise `middleware.rs` alone would contribute five invented routes.

Exit codes:
    0  every served route is declared, every declared path is served
    1  a route is served and not declared, or declared and not served
    2  nothing was measured: the spec is missing or unparseable, no route
       source was read, no route was extracted, a `.route(` call named no HTTP
       method, or the extractor failed its own fixture

Usage:
    python3 scripts/check_openapi_routes.py
    python3 scripts/check_openapi_routes.py --selftest
"""

import argparse
import json
import re
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

SPEC = Path("clients/openapi.json")
API_DIR = Path("crates/apollia-runtime/src/api")

# The file that must contribute routes for the scan to have measured anything:
# `server/router.rs` composes the root router, so a scan that read no route from
# it read nothing of the contract whatever the other modules returned. The path
# is relative to `API_DIR`, since the route modules are split into
# subdirectories and a bare file name no longer identifies one.
ROOT_ROUTER = "server/router.rs"

# The HTTP methods axum exposes as `MethodRouter` constructors, which are also
# the operation keys OpenAPI uses. `any` and `on` are absent on purpose: they
# name no single method, and a route built with one is reported as unmeasurable
# rather than guessed at.
METHODS = frozenset({"get", "post", "put", "delete", "patch", "head", "options", "trace"})

# The one served route the spec does not declare, and the reason it does not.
# `GET /api/v1/openapi.json` is the endpoint that serves the document itself:
# describing it inside the document it returns adds a path to every generated
# client for the sole purpose of fetching the description the client was
# generated from. A single named exemption, so a second one has to be written
# here to exist.
UNDECLARED_BY_DESIGN = frozenset({("get", "/api/v1/openapi.json")})

ROUTE_CALL = re.compile(r"\.route\(\s*\"([^\"]+)\"\s*,")
METHOD_CALL = re.compile(r"(?:^|[^\w:])(?:axum::routing::)?([a-z]+)\s*\(")
TEST_MODULE = re.compile(r"^#\[cfg\(test\)\]\s*\r?\n\s*(?:pub\s+)?mod\s", re.MULTILINE)
PATH_PARAM_BRACED = re.compile(r"\{(\w+)\}")
PATH_PARAM_COLON = re.compile(r":(\w+)")

# Driven through the extractor on every run. A guard whose detector has stopped
# firing reports a clean tree exactly as it reports a broken scanner, and this
# is the only thing that separates the two.
FIXTURE_SOURCE = """
pub fn build_router() -> Router {
    Router::new()
        .route("/api/v1/things", get(list_things).post(create_thing))
        .route("/api/v1/things/:id", axum::routing::delete(drop_thing))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_test_router_is_not_a_route() {
        let _ = Router::new().route("/api/v1/invented", get(nothing));
    }
}
"""

FIXTURE_EXPECTED = {
    ("get", "/api/v1/things"),
    ("post", "/api/v1/things"),
    ("delete", "/api/v1/things/{id}"),
}


class Unmeasurable(Exception):
    """Raised when the scan cannot decide, which is never the same as a pass."""


def normalise(path: str) -> str:
    """Return a path with its parameters written the OpenAPI way."""
    return PATH_PARAM_COLON.sub(r"{\1}", PATH_PARAM_BRACED.sub(r"{\1}", path))


def strip_test_modules(source: str) -> str:
    """Return `source` cut at its first top-level `#[cfg(test)] mod` block.

    Cutting rather than brace-matching: a test module is the last item of every
    file in this directory, and a brace matcher would have to lex string
    literals to stay correct. A `#[cfg(test)]` that is not followed by `mod` is
    deliberately not cut, so a route hidden behind one is reported rather than
    dropped in silence.
    """
    match = TEST_MODULE.search(source)
    return source[: match.start()] if match else source


def methods_of(call_body: str) -> set[str]:
    """HTTP methods named by the `MethodRouter` argument of a `.route(` call."""
    return {m.group(1) for m in METHOD_CALL.finditer(call_body)} & METHODS


def routes_in(source: str, origin: str) -> set[tuple[str, str]]:
    """`(method, path)` pairs registered by the production code of one module."""
    text = strip_test_modules(source)
    found: set[tuple[str, str]] = set()
    for call in ROUTE_CALL.finditer(text):
        path = call.group(1)
        depth, i = 1, call.end()
        while i < len(text) and depth:
            if text[i] == "(":
                depth += 1
            elif text[i] == ")":
                depth -= 1
            i += 1
        if depth:
            raise Unmeasurable(f"{origin}: unbalanced `.route(` call for {path}")
        verbs = methods_of(text[call.end() : i - 1])
        if not verbs:
            raise Unmeasurable(
                f"{origin}: `.route(\"{path}\", ...)` names no HTTP method, so "
                f"the route it registers cannot be compared with the spec"
            )
        for verb in verbs:
            found.add((verb, normalise(path)))
    return found


def served_routes(root: Path) -> tuple[set[tuple[str, str]], list[str]]:
    """Every route the API router registers, and the modules that were read."""
    api = root / API_DIR
    if not api.is_dir():
        raise Unmeasurable(f"{API_DIR} is not a directory")
    served: set[tuple[str, str]] = set()
    read: list[str] = []
    contributors: list[str] = []
    for path in sorted(api.rglob("*.rs")):
        rel = path.relative_to(api).as_posix()
        if rel != ROOT_ROUTER and not rel.startswith("routes_"):
            continue
        read.append(rel)
        found = routes_in(path.read_text(encoding="utf-8"), rel)
        if found:
            contributors.append(rel)
        served |= found
    if not read:
        raise Unmeasurable(f"no router module under {API_DIR}")
    if ROOT_ROUTER not in contributors:
        raise Unmeasurable(
            f"{ROOT_ROUTER} contributed no route; the root router composes the "
            f"whole contract, so a scan that missed it measured nothing"
        )
    return served, read


def declared_routes(root: Path) -> set[tuple[str, str]]:
    """Every `(method, path)` pair the committed spec declares."""
    spec_path = root / SPEC
    if not spec_path.is_file():
        raise Unmeasurable(f"{SPEC} not found")
    try:
        spec = json.loads(spec_path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, UnicodeDecodeError) as exc:
        raise Unmeasurable(f"{SPEC} is unreadable: {exc}") from exc
    paths = spec.get("paths")
    if not isinstance(paths, dict) or not paths:
        raise Unmeasurable(f"{SPEC} declares no path")
    declared = set()
    for path, operations in paths.items():
        if not isinstance(operations, dict):
            raise Unmeasurable(f"{SPEC}: {path} does not hold an operation map")
        for verb in operations:
            if verb in METHODS:
                declared.add((verb, normalise(path)))
    return declared


def extractor_broken() -> str | None:
    """Drive the fixture through the extractor, return what failed, if anything."""
    try:
        found = routes_in(FIXTURE_SOURCE, "fixture")
    except Unmeasurable as exc:
        return f"the fixture raised {exc}"
    if found != FIXTURE_EXPECTED:
        missing = sorted(FIXTURE_EXPECTED - found)
        extra = sorted(found - FIXTURE_EXPECTED)
        return f"missed {missing}, invented {extra}"
    return None


def render(label: str, pairs: list[tuple[str, str]]) -> None:
    print(f"\n{label}", file=sys.stderr)
    for verb, path in pairs:
        print(f"  {verb.upper():7} {path}", file=sys.stderr)


def run(root: Path) -> int:
    broken = extractor_broken()
    if broken:
        print(
            f"check_openapi_routes: NO COVERAGE, the route extractor failed its "
            f"own fixture: {broken}",
            file=sys.stderr,
        )
        return 2

    try:
        served, read = served_routes(root)
        declared = declared_routes(root)
    except (Unmeasurable, OSError, UnicodeDecodeError) as exc:
        print(f"check_openapi_routes: NO COVERAGE, {exc}", file=sys.stderr)
        return 2

    exempt = sorted(served & UNDECLARED_BY_DESIGN)
    registered = len(served)
    served -= UNDECLARED_BY_DESIGN
    undeclared = sorted(served - declared)
    unserved = sorted(declared - served)

    print(
        f"check_openapi_routes: {registered} routes registered by "
        f"{len(read)} modules of {API_DIR}, {len(declared)} declared by {SPEC}, "
        f"{len(exempt)} exempt, {len(undeclared)} served and undeclared, "
        f"{len(unserved)} declared and unserved"
    )
    sys.stdout.flush()

    if undeclared:
        render(
            f"{len(undeclared)} route(s) the runtime serves and the spec does not "
            f"declare. Every published consumer is generated from the spec, so "
            f"these exist for nobody:",
            undeclared,
        )
        print(
            "\nAnnotate the handler with `#[utoipa::path]` on this path, list it "
            "in the `paths(...)` of `crates/apollia-runtime/src/api/openapi.rs`, "
            "and refresh the spec with `bash clients/regen.sh --from-daemon`. "
            "If the route answers nothing a caller wants, remove the "
            "registration instead.",
            file=sys.stderr,
        )
        return 1

    if unserved:
        render(
            f"{len(unserved)} path(s) the spec declares and no router registers. "
            f"A generated client calling one gets a 404:",
            unserved,
        )
        print(
            "\nEither register the route, or drop the `#[utoipa::path]` "
            "annotation and refresh the spec.",
            file=sys.stderr,
        )
        return 1

    print("check_openapi_routes: the spec and the router agree, route by route")
    return 0


def _write_fixture(root: Path, source: str, spec: dict | None) -> None:
    api = root / API_DIR
    router = api / ROOT_ROUTER
    router.parent.mkdir(parents=True, exist_ok=True)
    router.write_text(source, encoding="utf-8")
    if spec is not None:
        (root / SPEC).parent.mkdir(parents=True, exist_ok=True)
        (root / SPEC).write_text(json.dumps(spec), encoding="utf-8")


def selftest() -> int:
    """Replay both directions on a temporary tree, never on the repository."""
    matching = {
        "paths": {
            "/api/v1/things": {"get": {}, "post": {}},
            "/api/v1/things/{id}": {"delete": {}},
        }
    }
    drifted = {"paths": {"/api/v1/things": {"get": {}, "post": {}}}}
    ghost = dict(matching["paths"])
    ghost["/api/v1/ghost"] = {"get": {}}

    cases = [
        ("a spec that matches the router is green", matching, 0),
        ("a route the spec omits is red", drifted, 1),
        ("a path no router serves is red", {"paths": ghost}, 1),
        ("a missing spec measures nothing", None, 2),
    ]
    failures = []
    with tempfile.TemporaryDirectory() as tmp:
        for label, spec, expected in cases:
            root = Path(tmp) / label.replace(" ", "_")
            _write_fixture(root, FIXTURE_SOURCE, spec)
            got = run(root)
            mark = "ok  " if got == expected else "FAIL"
            print(f"  {mark} {label} (expected {expected}, got {got})")
            if got != expected:
                failures.append(label)

        root = Path(tmp) / "no_method"
        _write_fixture(
            root,
            'Router::new().route("/api/v1/things", handler_router)\n',
            matching,
        )
        got = run(root)
        mark = "ok  " if got == 2 else "FAIL"
        print(f"  {mark} a route with no named method measures nothing "
              f"(expected 2, got {got})")
        if got != 2:
            failures.append("a route with no named method measures nothing")

    if failures:
        print(f"check_openapi_routes --selftest: {len(failures)} case(s) failed",
              file=sys.stderr)
        return 1
    print("check_openapi_routes --selftest: both directions hold")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--selftest",
        action="store_true",
        help="replay the rule on a temporary fixture instead of the repository",
    )
    args = parser.parse_args()
    return selftest() if args.selftest else run(REPO_ROOT)


if __name__ == "__main__":
    sys.exit(main())
