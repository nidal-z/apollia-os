#!/usr/bin/env python3
"""Hold the connector replay fixtures against the operations they must cover.

The two native connectors declare 51 operations, and until the replay harness
landed not one of them was ever handed an upstream answer to read. The tests
around them assert the shape of the request; the reading of the response was
measured nowhere. `crates/apollia-connectors/src/replay/` closes that half, and
this guard says how much of it is actually closed.

Four crossings, and they are not the same defect:

  uncovered   an operation the connectors declare with no fixture on disk. Its
              replay arm compiles, and has never run. On a descending ratchet:
              recording an answer needs a throwaway Google or Microsoft account,
              which arrives one operation at a time, so the backlog is named
              here and may only shrink.
  orphan      a fixture naming an operation no connector declares. The fixture
              is ahead of the tree and covers nothing. Held at zero.
  unarmed     an operation with a fixture but no arm in the dispatch table, or
              an arm naming an operation that does not exist. Held at zero; the
              Rust suite crosses the same two sides and would fail first.
  hardwired   a client whose base-URL constant is still read somewhere other
              than the one place that seeds the field. Such a call site cannot
              be pointed at the mock server, so the operations behind it are
              unreplayable however many fixtures they get. Held at zero.

Provenance is counted separately and never folded into coverage. A fixture
written by hand from a vendor's reference page proves the harness runs; it
proves nothing about what the API returns. `origin` tells the two apart, and
this guard prints both numbers rather than one.

Three exit codes, because a missing measurement must never read as a pass:

  0  the fixtures cover the operations, minus the named backlog.
  1  a defect: the backlog grew, a fixture went orphan, or a base URL is
     hardwired again.
  2  nothing measured: the operation catalogue or the fixture directory is
     absent, or no operation could be parsed.

Usage:
    python3 scripts/check_connector_fixtures.py
    python3 scripts/check_connector_fixtures.py --selftest
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CONNECTORS = REPO / "crates" / "apollia-connectors"
FIXTURES = CONNECTORS / "fixtures"
CATALOGUES = (
    CONNECTORS / "src" / "google" / "mod.rs",
    CONNECTORS / "src" / "microsoft" / "mod.rs",
)
DISPATCH = CONNECTORS / "src" / "replay" / "dispatch.rs"

# Every client module, with the base-URL constants it declares. Each constant
# may be read exactly twice: once where it is declared, once where it seeds the
# client's `base` field in `new()`. A third reading is a call site the replay
# harness cannot redirect.
BASE_CONSTANTS = {
    "src/google/gmail.rs": ("BASE",),
    "src/google/calendar.rs": ("BASE",),
    "src/google/docs.rs": ("BASE",),
    "src/google/sheets.rs": ("BASE",),
    "src/google/slides.rs": ("BASE",),
    "src/google/forms.rs": ("BASE",),
    "src/google/tasks.rs": ("BASE",),
    "src/google/youtube.rs": ("BASE",),
    "src/google/drive_workspace.rs": ("DRIVE_BASE", "UPLOAD_BASE"),
    "src/microsoft/mail.rs": ("GRAPH",),
    "src/microsoft/calendar.rs": ("GRAPH",),
    "src/microsoft/onedrive.rs": ("GRAPH",),
}

# Both calendars build their list URL in a free function that takes the base as
# an argument, and each has two tests that pass the constant in. Those are the
# only readings beyond the declaration and the seeding site anywhere in the
# crate, and the allowance is set to exactly that count rather than rounded up:
# a spare slot is a hardwired call site nobody would notice.
EXTRA_READINGS = {
    "src/google/calendar.rs": {"BASE": 2},
    "src/microsoft/calendar.rs": {"GRAPH": 2},
}

# Operations declared by a connector that reach no HTTP endpoint at all, so no
# recording can exist for them. `gdrive.list_picked_folders` answers from
# `apollia_auth::drive_prefs`, the local record of folders granted through the
# Drive picker. Kept out of the denominator rather than parked on the ratchet.
NO_UPSTREAM_CALL = frozenset({"gdrive.list_picked_folders"})

# Operations whose client reads nothing back, so a fixture can measure nothing
# about a reading. Six discard the response and answer `()`; four hand the body
# straight through as `serde_json::Value`, where an expectation would compare a
# document to itself. Both were measured from the return type of the client
# method each dispatch arm calls, and two were then read by hand.
#
# They sat on the ratchet, which described them as work a recording session
# could close. It cannot: recording an answer nobody parses proves nothing. Out
# of the denominator, like the operation that makes no HTTP call at all, so the
# number left says what is actually left.
#
# `outlook.send` is the sixth of the six and it was missed when this list was
# first written, which is the reason the list names its own measurement: the
# client answers `()` exactly like `outlook.reply` declared beside it, both
# routing through `HttpClient::send` and dropping the response, and it stayed on
# the ratchet as a recording nobody could ever make count.
#
# A client that starts reading one of these stops being in this list, and the
# guard says so rather than letting the exemption outlive its reason.
READS_NOTHING = frozenset(
    {
        # Discard the response entirely.
        "gcal.delete_event",
        "gdrive.workspace_delete",
        "gtasks.delete",
        "outlook.reply",
        "outlook.send",
        "outlook_cal.delete_event",
        # Return the body unparsed.
        "gdocs.append_text",
        "gsheets.append_values",
        "gsheets.update_values",
        "gslides.append_slide",
    }
)

# The operations with no fixture. This list is a ratchet: it may shrink, never
# grow. Closing an entry means recording an answer from a throwaway account,
# dropping the fixture in, and deleting the line here in the same commit.
#
# Every one of the sixteen left reads a shape another fixture already exercises:
# two an `Event`, two a `Vec<DriveFile>`, one a `DriveFile`, two a `Task`, two a
# `DriveItem`, two an `OutlookMessage`, three an `OutlookEvent`, two raw bytes.
# So closing one measures the same decode again on a real body rather than a new
# one, which is why they are the ones left rather than the ones done first.
#
# `gdrive.read_file` is the extreme case: its dispatch arm calls
# `workspace_read`, the very method the `gdrive.workspace_read` fixture already
# drives, because the runtime bridge maps those two operations onto that one
# method as well. A fixture for it would replay a byte-identical call.
UNCOVERED_BACKLOG = frozenset(
    {
        "gcal.create_event",
        "gcal.update_event",
        "gdrive.find_by_name",
        "gdrive.list_files_in",
        "gdrive.read_file",
        "gdrive.workspace_write",
        "gtasks.complete",
        "gtasks.create",
        "onedrive.download",
        "onedrive.get_metadata",
        "onedrive.list_recent",
        "outlook.move",
        "outlook.search",
        "outlook_cal.create_event",
        "outlook_cal.get_event",
        "outlook_cal.update_event",
    }
)

OPERATION_ID = re.compile(r'^\s*id:\s*"([a-z_]+\.[a-z_]+)",\s*$', re.M)
DISPATCH_ENTRY = re.compile(r'^\s*"([a-z_]+\.[a-z_]+)",\s*$', re.M)


def declared_operations() -> set[str]:
    """Every operation id the two connectors' catalogues declare."""
    found: set[str] = set()
    for path in CATALOGUES:
        found |= set(OPERATION_ID.findall(path.read_text(encoding="utf-8")))
    return found


def dispatch_arms() -> set[str]:
    """Every operation the replay dispatch table names."""
    text = DISPATCH.read_text(encoding="utf-8")
    return set(DISPATCH_ENTRY.findall(text))


def fixture_files() -> list[tuple[str, dict]]:
    """Every fixture on disk, as (file name, parsed object)."""
    out: list[tuple[str, dict]] = []
    for path in sorted(FIXTURES.glob("*.json")):
        try:
            out.append((path.name, json.loads(path.read_text(encoding="utf-8"))))
        except (OSError, json.JSONDecodeError) as e:
            out.append((path.name, {"__unreadable__": str(e)}))
    return out


def hardwired_bases() -> list[str]:
    """Base-URL constants read somewhere other than their seeding site."""
    defects: list[str] = []
    for rel, constants in BASE_CONSTANTS.items():
        path = CONNECTORS / rel
        if not path.exists():
            defects.append(f"{rel} is absent, so its base URL cannot be checked")
            continue
        text = path.read_text(encoding="utf-8")
        for const in constants:
            readings = len(re.findall(rf"\b{const}\b", text))
            allowed = 2 + EXTRA_READINGS.get(rel, {}).get(const, 0)
            if readings > allowed:
                defects.append(
                    f"{rel}: {const} is read {readings} times, {allowed} expected. "
                    "A call site that interpolates the constant instead of the "
                    "client's `base` field cannot be pointed at the replay mock "
                    "server, so the operations behind it are unreplayable"
                )
    return defects


def judge(
    declared: set[str], armed: set[str], fixtures: list[tuple[str, dict]]
) -> tuple[list[str], dict[str, int]]:
    """Return one line per defect, plus the counts to report on a clean run."""
    defects: list[str] = []

    unreadable = [name for name, body in fixtures if "__unreadable__" in body]
    if unreadable:
        defects.append(
            "unreadable: " + ", ".join(repr(n) for n in sorted(unreadable))
            + " could not be parsed as JSON, so it measures nothing"
        )

    covered: set[str] = set()
    captures = 0
    examples = 0
    for name, body in fixtures:
        if "__unreadable__" in body:
            continue
        op = body.get("operation")
        if not op:
            defects.append(f"malformed: {name!r} declares no `operation`")
            continue
        covered.add(op)
        origin = body.get("origin")
        if origin == "capture":
            captures += 1
        elif origin == "example":
            examples += 1
        else:
            defects.append(
                f"malformed: {name!r} declares origin {origin!r}, which is neither "
                "'capture' nor 'example'"
            )

    replayable = declared - NO_UPSTREAM_CALL - READS_NOTHING
    uncovered = replayable - covered

    # An exemption that outlives its reason is worse than no exemption. A client
    # that starts reading one of these would leave it silently out of the count.
    for name in sorted(READS_NOTHING & covered):
        defects.append(
            f"reads-nothing: {name!r} is exempt as an operation whose client reads "
            f"nothing back, and a fixture now claims a reading for it. Either the "
            f"client changed and the line belongs in the ratchet, or the fixture "
            f"asserts something the client never parses"
        )

    grown = sorted(uncovered - UNCOVERED_BACKLOG)
    if grown:
        defects.append(
            "uncovered: "
            + ", ".join(repr(name) for name in grown)
            + " has no replay fixture and is not on the ratchet. The backlog of "
            "uncovered operations may only shrink, so either record an answer for "
            "it or state why it belongs in UNCOVERED_BACKLOG"
        )

    closed = sorted(UNCOVERED_BACKLOG - uncovered)
    if closed:
        defects.append(
            "uncovered: "
            + ", ".join(repr(name) for name in closed)
            + " now carries a fixture and is still named in UNCOVERED_BACKLOG. Drop "
            "the line in the same commit, so the ratchet records the ground won"
        )

    orphan = sorted(covered - declared)
    if orphan:
        defects.append(
            "orphan: "
            + ", ".join(repr(name) for name in orphan)
            + " carries a fixture and no connector declares it. The fixture covers "
            "nothing"
        )

    unarmed = sorted(replayable - armed)
    if unarmed:
        defects.append(
            "unarmed: "
            + ", ".join(repr(name) for name in unarmed)
            + " is declared and has no arm in the replay dispatch table, so no "
            "fixture for it could ever run"
        )

    stale_arm = sorted(armed - declared)
    if stale_arm:
        defects.append(
            "unarmed: the replay dispatch table names "
            + ", ".join(repr(name) for name in stale_arm)
            + ", which no connector declares"
        )

    defects.extend(hardwired_bases())

    counts = {
        "declared": len(declared),
        "replayable": len(replayable),
        "covered": len(covered & replayable),
        "backlog": len(UNCOVERED_BACKLOG),
        "captures": captures,
        "examples": examples,
        "files": len(fixtures),
    }
    return defects, counts


def selftest() -> int:
    """Prove each rule fires, and that a clean set stays clean."""
    failures = 0
    backlog = sorted(UNCOVERED_BACKLOG)
    declared = {"alpha.one", "beta.two", *backlog, *NO_UPSTREAM_CALL}
    armed = declared - NO_UPSTREAM_CALL

    def fixture(op: str, origin: str = "capture") -> tuple[str, dict]:
        return (f"{op}.json", {"operation": op, "origin": origin})

    clean = [fixture("alpha.one"), fixture("beta.two")]

    def check(label: str, defects: list[str], want_defect: bool) -> int:
        if bool(defects) != want_defect:
            print(f"selftest: {label} (got {defects})")
            return 1
        return 0

    # A clean tree: everything covered except exactly the named backlog. The
    # hardwired check runs against the real tree, which must also be clean.
    failures += check("the named backlog was reported as a defect",
                      judge(declared, armed, clean)[0], False)

    # An operation that loses its fixture is caught.
    failures += check("a newly uncovered operation was not reported",
                      judge(declared, armed, [fixture("alpha.one")])[0], True)

    # An operation covered while still named in the backlog is caught.
    failures += check("a closed backlog entry was not reported",
                      judge(declared, armed, clean + [fixture(backlog[0])])[0], True)

    # A fixture for an operation nothing declares is caught.
    failures += check("an orphan fixture was not reported",
                      judge(declared, armed, clean + [fixture("gone.away")])[0], True)

    # A declared operation with no dispatch arm is caught.
    failures += check("an unarmed operation was not reported",
                      judge(declared, armed - {"alpha.one"}, clean)[0], True)

    # A fixture with no `origin` is caught.
    failures += check("a fixture with an unknown origin was not reported",
                      judge(declared, armed, clean + [("x.json", {"operation": "alpha.one",
                                                                 "origin": "guess"})])[0], True)

    # A fixture that is not JSON is caught.
    failures += check("an unreadable fixture was not reported",
                      judge(declared, armed, clean + [("x.json", {"__unreadable__": "boom"})])[0],
                      True)

    # The real tree's base URLs are injectable.
    failures += check("a base URL is hardwired on the current tree",
                      hardwired_bases(), False)

    if failures:
        return 1
    print("check_connector_fixtures --selftest: 8 assertions, every one holds")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        prog="check_connector_fixtures.py",
        description=(
            "Hold the connector replay fixtures against the operations the two "
            "native connectors declare, with the uncovered ones on a descending "
            "ratchet."
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

    for path, what in (
        (CATALOGUES[0], "the Google operation catalogue"),
        (CATALOGUES[1], "the Microsoft operation catalogue"),
        (DISPATCH, "the replay dispatch table"),
        (FIXTURES, "the fixture directory"),
    ):
        if not path.exists():
            print(
                f"check_connector_fixtures: nothing measured. {what} is absent at\n"
                f"  {path}\n"
                "so the two sides cannot be crossed.",
                file=sys.stderr,
            )
            return 2

    declared = declared_operations()
    if not declared:
        print(
            "check_connector_fixtures: nothing measured. No operation id was "
            "parsed out of the connector catalogues, so there is nothing to "
            "cross. The catalogue shape this guard reads has probably changed.",
            file=sys.stderr,
        )
        return 2

    armed = dispatch_arms()
    if not armed:
        print(
            "check_connector_fixtures: nothing measured. The replay dispatch "
            "table names no operation, so no fixture could run.",
            file=sys.stderr,
        )
        return 2

    defects, counts = judge(declared, armed, fixture_files())
    if defects:
        print(
            f"{len(defects)} defect(s) between the connector operations and their "
            "replay fixtures.\n",
            file=sys.stderr,
        )
        for line in defects:
            print(f"  {line}", file=sys.stderr)
        return 1

    print(
        "check_connector_fixtures: "
        f"{counts['declared']} operations declared, "
        f"{counts['replayable']} whose client reads a response, "
        f"{counts['covered']} carrying a fixture, "
        f"{counts['backlog']} on the ratchet, "
        f"{len(NO_UPSTREAM_CALL)} making no HTTP call, "
        f"{len(READS_NOTHING)} reading nothing back, 0 orphan, 0 unarmed, 0 hardwired"
    )
    print(
        f"  provenance: {counts['files']} fixture file(s), "
        f"{counts['captures']} recorded from a real API, "
        f"{counts['examples']} hand-written from public reference pages"
    )
    if counts["captures"] == 0:
        print(
            "  no fixture is a recording yet, so the harness is proven to run and "
            "nothing is proven about the real upstream shapes"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
