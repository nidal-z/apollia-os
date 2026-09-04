#!/usr/bin/env python3
"""Hold the generated API clients against the specification they come from.

`clients/openapi.json` is the contract the runtime publishes, and the clients
under `clients/` are generated from it. A generated artefact looks
authoritative and ages silently: the Python client was committed once, the
specification kept growing, and seven operations it declares have no module a
caller could reach. Nothing in the tree said so, and the tutorial that drives
Apollia from a host product sends an integrator straight into that client.

Two directions, and they are not the same defect:

  missing    the specification declares an operation and the client has no
             module for it. The client is behind, and a developer reading the
             API reference asks for something the package cannot do.
  orphan     the client carries a module for an operation the specification no
             longer declares. The client is ahead, and a caller written against
             it breaks on a runtime that dropped the route.

`missing` is on a descending ratchet rather than at zero. Regenerating the
whole client today produces thousands of lines of generator drift, which is a
change of a different nature from closing a gap, so the backlog is named here
and can only shrink: a count above the ceiling is a defect, a count below it
asks for the ceiling to be lowered in the same commit. `orphan` is held at
zero, because a client ahead of its contract is never intentional.

Three exit codes, because a missing measurement must never read as a pass:

  0  the client covers the specification, minus the named backlog.
  1  an operation left the client, or the backlog grew, or an orphan appeared.
  2  nothing measured: the specification or the client is absent.

Usage:
    python3 scripts/check_generated_clients.py
    python3 scripts/check_generated_clients.py --selftest
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SPEC = REPO / "clients" / "openapi.json"
PY_API = REPO / "clients" / "python" / "apollia_runtime_client" / "api"

# The operations the committed Python client does not carry, measured on
# 2026-09-04. This list is a ratchet: it may shrink, never grow. Closing an
# entry means regenerating that endpoint module and deleting its line here in
# the same commit.
MISSING_BACKLOG = frozenset(
    {
        "get_registry_model",
        "handle_webhook",
        "list_audit_journal",
        "reload_stt_engine",
        "search_registry",
        "transcribe_audio",
        "update_stt_config",
    }
)


def declared_operations(spec: dict) -> set[str]:
    """Every operationId the specification declares, whatever its verb."""
    found: set[str] = set()
    for item in spec.get("paths", {}).values():
        if not isinstance(item, dict):
            continue
        for operation in item.values():
            if isinstance(operation, dict) and operation.get("operationId"):
                found.add(operation["operationId"])
    return found


def client_modules(root: Path) -> set[str]:
    """Every endpoint module of the Python client, addressed by its stem."""
    return {p.stem for p in root.rglob("*.py") if p.stem != "__init__"}


def judge(declared: set[str], present: set[str]) -> list[str]:
    """Return one line per defect, empty when the two sides agree."""
    defects: list[str] = []
    missing = declared - present
    orphan = present - declared

    grown = sorted(missing - MISSING_BACKLOG)
    if grown:
        defects.append(
            "missing: the specification declares "
            + ", ".join(repr(name) for name in grown)
            + " and the Python client carries no module for it. The backlog of "
            "uncovered operations may only shrink, so either regenerate the "
            "module or state why it belongs in MISSING_BACKLOG"
        )

    closed = sorted(MISSING_BACKLOG - missing)
    if closed:
        defects.append(
            "missing: "
            + ", ".join(repr(name) for name in closed)
            + " is covered by the client and still named in MISSING_BACKLOG. "
            "Drop the line in the same commit, so the ratchet records the ground "
            "that was won"
        )

    if orphan:
        defects.append(
            "orphan: the Python client carries "
            + ", ".join(repr(name) for name in sorted(orphan))
            + " and the specification declares no such operation. A caller "
            "written against it breaks on a runtime that dropped the route"
        )
    return defects


def selftest() -> int:
    """Prove each rule fires, and that a clean set stays clean."""
    failures = 0
    backlog = sorted(MISSING_BACKLOG)

    # A clean tree: everything covered except exactly the named backlog.
    declared = {"alpha", "beta", *backlog}
    if judge(declared, {"alpha", "beta"}):
        print("selftest: the named backlog was reported as a defect")
        failures += 1

    # An operation that leaves the client is caught.
    if not judge(declared, {"alpha"}):
        print("selftest: a newly uncovered operation was not reported")
        failures += 1

    # An operation covered while still named in the backlog is caught.
    if not judge(declared, {"alpha", "beta", backlog[0]}):
        print("selftest: a closed backlog entry was not reported")
        failures += 1

    # A module with no operation behind it is caught.
    if not judge(declared, {"alpha", "beta", "gone_from_the_spec"}):
        print("selftest: an orphan module was not reported")
        failures += 1

    if failures:
        return 1
    print("check_generated_clients --selftest: 4 assertions, every one holds")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        prog="check_generated_clients.py",
        description=(
            "Hold the generated API clients against clients/openapi.json, with "
            "the uncovered operations on a descending ratchet."
        ),
    )
    parser.add_argument(
        "--selftest",
        action="store_true",
        help="prove each rule fires, and measure nothing else",
    )
    args = parser.parse_args()

    if args.selftest:
        return selftest()

    for path, what in ((SPEC, "the specification"), (PY_API, "the Python client")):
        if not path.exists():
            print(
                f"check_generated_clients: nothing measured. {what} is absent at\n"
                f"  {path}\n"
                "so the two sides cannot be crossed.",
                file=sys.stderr,
            )
            return 2

    declared = declared_operations(json.loads(SPEC.read_text(encoding="utf-8")))
    present = client_modules(PY_API)
    if not declared:
        print(
            "check_generated_clients: nothing measured. The specification "
            "declares no operationId, so there is nothing to cross.",
            file=sys.stderr,
        )
        return 2

    defects = judge(declared, present)
    if defects:
        print(
            f"{len(defects)} defect(s) between clients/openapi.json and the "
            "generated Python client.\n",
            file=sys.stderr,
        )
        for line in defects:
            print(f"  {line}", file=sys.stderr)
        return 1

    print(
        f"check_generated_clients: {len(declared)} operations declared, "
        f"{len(declared) - len(MISSING_BACKLOG)} carried by the Python client, "
        f"{len(MISSING_BACKLOG)} on the ratchet, 0 orphan"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
