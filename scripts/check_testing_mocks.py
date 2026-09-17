#!/usr/bin/env python3
"""Cross the published `ctx` protocols with the mocks the SDK hands to tests.

`apollia.testing` is what an agent author writes tests against. It is the third
side of the `ctx` contract, and it was the unguarded one: `check_ctx_contract.py`
crosses the protocol with the PyO3 bridge, nothing crossed either with the
mocks. A mock that drifts from the protocol does not break a build, it turns
tests green over behaviour the product does not have, which is worse than no
test at all.

The instance that forced this file, found on 2026-09-16 the day `schema=` landed
on `ctx.llm`:

  * `MockLlmProxy.chat` declared its parameters one by one and had no `schema`,
    so a call the runtime accepts raised `TypeError` under test;
  * `MockLlmProxy.complete` swallowed it through `**kwargs` and went on
    returning a `MockLlmResponse`, where the runtime returns the validated
    value. A test written on that mock passed while exercising a return type
    production never produces.

Both halves are the same defect seen twice, and both are mechanical.

What is crossed, and what is not

The pairing is read from the code rather than listed here: `Ctx` (in
`sdk/apollia/types.py`) names each service and its protocol, `MockContext` (in
`sdk/apollia/testing/mock_factory.py`) names each service and its mock. A
service present in both is crossed; one present in a single side is reported.
So a new service is crossed the day it is wired, without this file being
touched.

Member by member, for every method the protocol publishes:

  1. the mock has it;
  2. every parameter the protocol declares is declared by the mock too, by
     name. A `**kwargs` does NOT satisfy a declared parameter: that is the exact
     shape that let `schema=` through in silence;
  3. a parameter keyword-only on one side is keyword-only on the other, since
     that is what decides whether a call binds;
  4. a parameter with a default on the protocol has one on the mock, or a
     caller relying on the default gets a `TypeError` under test alone.

Return types are NOT compared. A mock returns `MockLlmResponse` where the
protocol returns `LlmResponse`, by construction, and a table mapping the two
would be a source of false positives. What is compared instead is the parameter
list, which is what made the return type wrong in the founding instance: a mock
forced to declare `schema` is a mock whose author had to decide what it returns.

Properties, and neither is a getter: a mock may add parameters and members the
protocol does not have (`responses`, `call_count`, the `assert_*` helpers are
the whole point of a mock), so the crossing runs in one direction only, from
the protocol outwards.

Verdict by exit code:

  0  every paired service agrees, member by member
  1  a divergence, or a service on one side alone
  2  nothing measured: a subtree is absent, or fewer than ten services paired

Run: python3 scripts/check_testing_mocks.py
     python3 scripts/check_testing_mocks.py --selftest
"""

from __future__ import annotations

import argparse
import ast
import sys
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SDK = REPO_ROOT / "sdk" / "apollia"
CTX_TYPES = SDK / "types.py"
CONTEXT_DIR = SDK / "context"
TESTING_DIR = SDK / "testing"
MOCK_FACTORY = TESTING_DIR / "mock_factory.py"

# Below this many paired services the run says nothing about the tree: one of
# the two sides stopped being read and a green would mean "nothing to compare".
MIN_PAIRED_SERVICES = 10

# `ctx.logger` is a stdlib `logging.Logger` on both sides, not an Apollia
# protocol, so there is nothing of ours to cross. Named rather than skipped by
# accident: an entry whose subject disappears is reported.
SERVICES_WITHOUT_AN_APOLLIA_PROTOCOL = {"logger"}

# ── the ratchet ──────────────────────────────────────────────────────────────
#
# The debt this guard found on the day it was written, one number per service.
# `ctx.llm` is absent because it was brought to zero in the same commit; the two
# below were not, and repairing them is a lot of its own rather than a widening
# of the one that produced this file.
#
# The ratchet only descends, and it is two-sided: a service ABOVE its number
# fails, and a service BELOW it fails too, until the number follows it down.
# Without the second half the list outlives the debt, and a guard carrying
# allowances nobody can retire is a silencer with extra steps.
#
# `ctx.mail` is one entry because the whole service is missing from
# `MockContext`: an agent that sends mail cannot be tested at all today.
DIVERGENCES_ON_THE_RATCHET: dict[str, int] = {
    "mail": 1,
    "memory": 13,
}


@dataclass(frozen=True)
class Param:
    """One parameter as Python binds it."""

    name: str
    keyword_only: bool
    has_default: bool


@dataclass
class Method:
    """One method's name and the parameters it declares, `self` excluded."""

    name: str
    params: list[Param]
    has_var_keyword: bool


def parse(path: Path) -> ast.Module | None:
    """The AST of `path`, or None when it cannot be read."""
    try:
        return ast.parse(path.read_text(encoding="utf-8"))
    except (OSError, SyntaxError):
        return None


def methods_of(node: ast.ClassDef) -> dict[str, Method]:
    """Every public method a class body declares, by name.

    Private helpers (a leading underscore) are a mock's own business and are not
    part of any contract, so they are left out of the crossing.
    """
    out: dict[str, Method] = {}
    for item in node.body:
        if not isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
            continue
        if item.name.startswith("_"):
            continue
        # A `@property` is an attribute on both sides; its parameter list is
        # `self` alone and comparing it says nothing.
        if any(isinstance(d, ast.Name) and d.id == "property" for d in item.decorator_list):
            continue
        args = item.args
        params: list[Param] = []
        positional = args.posonlyargs + args.args
        defaults_start = len(positional) - len(args.defaults)
        for index, arg in enumerate(positional):
            if arg.arg == "self":
                continue
            params.append(Param(arg.arg, keyword_only=False, has_default=index >= defaults_start))
        for arg, default in zip(args.kwonlyargs, args.kw_defaults):
            params.append(Param(arg.arg, keyword_only=True, has_default=default is not None))
        out[item.name] = Method(item.name, params, has_var_keyword=args.kwarg is not None)
    return out


def classes_in(tree: ast.Module) -> dict[str, ast.ClassDef]:
    """Top-level classes of a module, by name."""
    return {n.name: n for n in tree.body if isinstance(n, ast.ClassDef)}


def ctx_services() -> dict[str, str]:
    """Service name to protocol class name, read from the `Ctx` protocol."""
    tree = parse(CTX_TYPES)
    if tree is None:
        return {}
    ctx = classes_in(tree).get("Ctx")
    if ctx is None:
        return {}
    out: dict[str, str] = {}
    for item in ctx.body:
        if not isinstance(item, ast.AnnAssign) or not isinstance(item.target, ast.Name):
            continue
        annotation = item.annotation
        # `ProfileInterface | None` is the same protocol with a nullable reach.
        if isinstance(annotation, ast.BinOp):
            annotation = annotation.left
        if isinstance(annotation, ast.Name):
            out[item.target.id] = annotation.id
    return out


def mock_attributes() -> set[str]:
    """Every attribute `MockContext.__init__` assigns, whatever the value."""
    tree = parse(MOCK_FACTORY)
    if tree is None:
        return set()
    context = classes_in(tree).get("MockContext")
    if context is None:
        return set()
    names: set[str] = set()
    for stmt in ast.walk(context):
        targets: list[ast.expr] = []
        if isinstance(stmt, ast.Assign):
            targets = list(stmt.targets)
        elif isinstance(stmt, ast.AnnAssign):
            targets = [stmt.target]
        for target in targets:
            if (
                isinstance(target, ast.Attribute)
                and isinstance(target.value, ast.Name)
                and target.value.id == "self"
            ):
                names.add(target.attr)
    return names


def mock_services() -> dict[str, str]:
    """Service name to mock class name, read from `MockContext.__init__`."""
    tree = parse(MOCK_FACTORY)
    if tree is None:
        return {}
    context = classes_in(tree).get("MockContext")
    if context is None:
        return {}
    init = next(
        (n for n in context.body if isinstance(n, ast.FunctionDef) and n.name == "__init__"),
        None,
    )
    if init is None:
        return {}
    out: dict[str, str] = {}
    for stmt in ast.walk(init):
        if not isinstance(stmt, ast.Assign) or len(stmt.targets) != 1:
            continue
        target = stmt.targets[0]
        if not (isinstance(target, ast.Attribute) and isinstance(target.value, ast.Name)):
            continue
        if target.value.id != "self":
            continue
        if isinstance(stmt.value, ast.Call) and isinstance(stmt.value.func, ast.Name):
            out[target.attr] = stmt.value.func.id
    return out


def collect_classes(paths: list[Path]) -> dict[str, ast.ClassDef]:
    """Every class declared across `paths`, by name."""
    found: dict[str, ast.ClassDef] = {}
    for path in paths:
        tree = parse(path)
        if tree is not None:
            found.update(classes_in(tree))
    return found


def compare(service: str, protocol: Method, mock: Method | None) -> list[str]:
    """Every way `mock` fails to answer a call `protocol` accepts."""
    if mock is None:
        return [
            f"ctx.{service}.{protocol.name}: the protocol publishes it and the "
            "mock does not, so a test cannot call it at all"
        ]
    findings: list[str] = []
    by_name = {p.name: p for p in mock.params}
    for expected in protocol.params:
        actual = by_name.get(expected.name)
        if actual is None:
            if mock.has_var_keyword and expected.keyword_only:
                findings.append(
                    f"ctx.{service}.{protocol.name}: `{expected.name}` reaches the "
                    "mock through **kwargs and is dropped. A test passing it sees "
                    "the mock's unconditional behaviour, not the one the "
                    "parameter asks for"
                )
            else:
                findings.append(
                    f"ctx.{service}.{protocol.name}: the protocol declares "
                    f"`{expected.name}` and the mock does not, so the call raises "
                    "TypeError under test and works in production"
                )
            continue
        if actual.keyword_only != expected.keyword_only:
            side = "keyword-only" if expected.keyword_only else "positional"
            findings.append(
                f"ctx.{service}.{protocol.name}: `{expected.name}` is {side} on "
                "the protocol and not on the mock, and that is what decides "
                "whether a call binds"
            )
        if expected.has_default and not actual.has_default:
            findings.append(
                f"ctx.{service}.{protocol.name}: `{expected.name}` has a default "
                "on the protocol and none on the mock, so a caller relying on it "
                "meets a TypeError under test alone"
            )
    return findings


def cross(
    services: dict[str, str],
    mocks: dict[str, str],
    protocol_classes: dict[str, ast.ClassDef],
    mock_classes: dict[str, ast.ClassDef],
) -> tuple[list[str], int, int]:
    """Cross the two sides. Returns the findings, services paired, members compared."""
    findings: list[str] = []
    paired = 0
    members = 0

    for service, protocol_name in sorted(services.items()):
        if service in SERVICES_WITHOUT_AN_APOLLIA_PROTOCOL:
            continue
        mock_name = mocks.get(service)
        if mock_name is None:
            findings.append(
                f"ctx.{service}: published by the Ctx protocol and absent from "
                "MockContext, so an agent using it cannot be tested"
            )
            continue
        protocol_node = protocol_classes.get(protocol_name)
        mock_node = mock_classes.get(mock_name)
        if protocol_node is None or mock_node is None:
            findings.append(
                f"ctx.{service}: {protocol_name} or {mock_name} was not found in "
                "the subtrees this guard reads"
            )
            continue
        paired += 1
        protocol_methods = methods_of(protocol_node)
        mock_methods = methods_of(mock_node)
        for name, method in sorted(protocol_methods.items()):
            members += 1
            findings.extend(compare(service, method, mock_methods.get(name)))

    for service in sorted(set(mocks) - set(services)):
        findings.append(
            f"ctx.{service}: wired in MockContext and published by no Ctx "
            "attribute, so tests can reach a surface agents cannot"
        )

    return findings, paired, members


def stale_exemptions(services: dict[str, str]) -> list[str]:
    """Entries of the set-aside list whose subject the tree no longer carries.

    An exemption that outlives its object is a silencer, so it is driven from
    both sides. Kept out of `cross` on purpose: it is a fact about this tree,
    and the self-test crosses fabricated sources where it would fire on every
    run and say nothing.
    """
    return [
        f"ctx.{service}: set aside by a rule in this guard, and the Ctx "
        "protocol no longer publishes it. Remove the entry"
        for service in sorted(SERVICES_WITHOUT_AN_APOLLIA_PROTOCOL - set(services))
    ]


def service_of(finding: str) -> str:
    """The service a finding is about, read from its `ctx.<service>` prefix."""
    return finding.split(":", 1)[0].removeprefix("ctx.").split(".", 1)[0]


def apply_ratchet(raw: list[str], table: dict[str, int] | None = None) -> tuple[list[str], int]:
    """Hold the known debt at its number, in both directions.

    Returns the findings that must fail the run, and how many were held by the
    ratchet. A service over its number reports only the surplus, because the
    reader wants the new one, not the thirteen it joined.

    `table` defaults to the real one. It is a parameter so the self-test can
    drive one entry at a time: with the whole table, every entry the fabricated
    input does not mention reads as a debt that shrank to zero, which is the
    right answer on a full run and noise on a partial one.
    """
    table = DIVERGENCES_ON_THE_RATCHET if table is None else table
    counted: dict[str, list[str]] = {}
    failures: list[str] = []
    for finding in raw:
        service = service_of(finding)
        if service in table:
            counted.setdefault(service, []).append(finding)
        else:
            failures.append(finding)

    allowed = 0
    for service, entry in sorted(table.items()):
        found = counted.get(service, [])
        allowed += min(len(found), entry)
        if len(found) > entry:
            surplus = len(found) - entry
            failures.append(
                f"ctx.{service}: {len(found)} divergence(s) against a ratchet entry "
                f"of {entry}. Repair the {surplus} new one(s), or move the debt "
                "into the table in the same commit, knowingly"
            )
            failures.extend(found[entry:])
        elif len(found) < entry:
            failures.append(
                f"ctx.{service}: {len(found)} divergence(s) against a ratchet entry "
                f"of {entry}. The debt shrank; lower the entry to {len(found)} so "
                "the list cannot outlive what it excuses"
            )
    return failures, allowed


def run() -> int:
    if not CONTEXT_DIR.is_dir() or not TESTING_DIR.is_dir():
        print(
            "nothing measured: sdk/apollia/context or sdk/apollia/testing is absent",
            file=sys.stderr,
        )
        return 2

    services = ctx_services()
    mocks = mock_services()
    protocol_classes = collect_classes(sorted(CONTEXT_DIR.glob("*.py")))
    mock_classes = collect_classes(sorted(TESTING_DIR.glob("*.py")))

    # A `Ctx` attribute whose annotation names no protocol class is data (a
    # flag, a TypedDict), not a service: it has no methods to cross, but a mock
    # context without it still fails every test that reads it. It is required
    # to exist on MockContext and set aside from the method crossing.
    assigned = mock_attributes()
    data_members = {s: c for s, c in services.items() if c not in protocol_classes}
    services = {s: c for s, c in services.items() if c in protocol_classes}
    missing_data = [
        f"ctx.{service}: published by the Ctx protocol and never assigned by "
        "MockContext, so an agent reading it fails under test alone"
        for service in sorted(data_members)
        if service not in assigned and service not in SERVICES_WITHOUT_AN_APOLLIA_PROTOCOL
    ]
    raw, paired, members = cross(services, mocks, protocol_classes, mock_classes)
    raw.extend(missing_data)
    findings, allowed = apply_ratchet(raw)
    findings.extend(stale_exemptions({**services, **data_members}))

    print(f"services published by Ctx    : {len(services)}")
    print(f"services wired in MockContext: {len(mocks)}")
    print(f"services paired              : {paired}")
    print(f"members compared             : {members}")
    print(f"divergences on the ratchet   : {allowed}")

    if paired < MIN_PAIRED_SERVICES:
        print(
            f"\nnothing measured: {paired} service(s) paired, below the floor of "
            f"{MIN_PAIRED_SERVICES}. A crossing that reads one service and finds "
            "it in agreement has verified nothing.",
            file=sys.stderr,
        )
        return 2

    if not findings:
        print("\nOK: every mock answers the calls its protocol publishes, the ratcheted debt aside")
        return 0

    print(
        f"\n{len(findings)} divergence(s). A mock that drifts from its protocol "
        "turns tests green over behaviour the product does not have.\n",
        file=sys.stderr,
    )
    for finding in findings:
        print(f"  {finding}", file=sys.stderr)
    return 1


# ── self-test ────────────────────────────────────────────────────────────────
#
# A detector that only ever runs against a clean tree proves the scan ran, not
# that the detector works. The founding instance is replayed here, in both
# directions, on fabricated sources.

_PROTOCOL = """
class LlmProxy(Protocol):
    async def complete(self, messages, *, backend=None, schema=None): ...
    async def chat(self, system, user, *, schema=None): ...
"""

_MOCK_FAITHFUL = """
class MockLlmProxy:
    async def complete(self, messages, *, backend=None, schema=None): ...
    async def chat(self, system, user, *, schema=None): ...
"""

_MOCK_SWALLOWING = """
class MockLlmProxy:
    async def complete(self, messages, **kwargs): ...
    async def chat(self, system, user, *, schema=None): ...
"""

_MOCK_MISSING = """
class MockLlmProxy:
    async def complete(self, messages, *, backend=None, schema=None): ...
    async def chat(self, system, user): ...
"""


def _one(protocol_src: str, mock_src: str) -> list[str]:
    protocol = classes_in(ast.parse(protocol_src))
    mock = classes_in(ast.parse(mock_src))
    findings, _, _ = cross({"llm": "LlmProxy"}, {"llm": "MockLlmProxy"}, protocol, mock)
    return findings


def selftest() -> int:
    problems: list[str] = []

    if _one(_PROTOCOL, _MOCK_FAITHFUL):
        problems.append("the crossing refused a mock that matches its protocol")

    swallowed = _one(_PROTOCOL, _MOCK_SWALLOWING)
    if not any("**kwargs" in f for f in swallowed):
        problems.append(
            "a parameter reaching the mock through **kwargs was not reported, "
            "which is the defect this guard was written for"
        )

    missing = _one(_PROTOCOL, _MOCK_MISSING)
    if not any("TypeError under test" in f for f in missing):
        problems.append("a parameter absent from the mock was not reported")

    # A mock with extra parameters is not a finding: that is what a mock is for.
    extra = _one(_PROTOCOL, _MOCK_FAITHFUL.replace("schema=None)", "schema=None, spy=None)"))
    if extra:
        problems.append("the crossing reported a parameter the mock adds of its own")

    # A crossing that pairs almost nothing answers 2, not 0.
    _, paired, _ = cross({"llm": "LlmProxy"}, {"llm": "MockLlmProxy"}, {}, {})
    if paired >= MIN_PAIRED_SERVICES:
        problems.append("the floor on paired services does not hold")

    # The ratchet, driven in both directions on a fabricated entry. A service
    # over its number fails and names the surplus; a service UNDER it fails too,
    # which is what stops the list outliving the debt.
    ratcheted = "ctx.memory.record: a fabricated divergence"
    one_entry = {"memory": 3}
    at_number, held = apply_ratchet([ratcheted] * one_entry["memory"], one_entry)
    if at_number:
        problems.append("the ratchet failed a service sitting exactly on its number")
    if held != one_entry["memory"]:
        problems.append("the ratchet did not count what it held")

    over, _ = apply_ratchet([ratcheted] * (one_entry["memory"] + 1), one_entry)
    if not any("Repair the 1 new one(s)" in f for f in over):
        problems.append("the ratchet did not fail a service over its number")

    under, _ = apply_ratchet([ratcheted] * (one_entry["memory"] - 1), one_entry)
    if not any("The debt shrank" in f for f in under):
        problems.append(
            "the ratchet did not fail a service under its number, so the list "
            "can outlive the debt it excuses"
        )

    # A service absent from the table is never held.
    loose, _ = apply_ratchet(
        [ratcheted] * one_entry["memory"] + ["ctx.events.emit: a fabricated divergence"],
        one_entry,
    )
    if len(loose) != 1:
        problems.append("a divergence on a service outside the table was held")

    print("selftest: the crossing driven on a faithful mock, one swallowing")
    print("          through **kwargs, one missing the parameter, one adding its own,")
    print("          and the ratchet driven over, under and exactly on its number")
    if problems:
        print(f"\n{len(problems)} finding(s) in the guard itself:\n", file=sys.stderr)
        for problem in problems:
            print(f"  {problem}", file=sys.stderr)
        return 1
    print(
        "selftest: it accepts the first, refuses the next two, ignores the fourth, "
        "and the ratchet fails in both directions"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--selftest",
        action="store_true",
        help="drive the crossing on fabricated faithful and divergent mocks",
    )
    args = parser.parse_args()
    return selftest() if args.selftest else run()


if __name__ == "__main__":
    sys.exit(main())
