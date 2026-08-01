#!/usr/bin/env python3
"""Every crate must actually get the workspace lints, not merely be expected to.

Cargo does not merge a crate's `[lints]` table with the workspace one: declaring
any local table replaces the inheritance wholesale. Five crates needed
`unsafe_code = "allow"` for FFI, wrote their own `[lints.rust]`, and silently
lost `unwrap_used = "deny"` along with it. Nothing failed. Clippy went on
passing, and an `unwrap()` sat in production code with nothing to report it,
against the first rule in FORBIDDEN.md.

That was written into RUST-PATTERNS.md, which is where the lesson of this whole
pass applies to itself: a rule in a document is not a gate. The next crate that
needs an FFI allow will reopen the hole exactly the same way, and only a check
that runs will say so.

Two things are verified per crate:

1. Either `[lints] workspace = true`, or a local table that restates
   `unwrap_used = "deny"`.
2. A local table that declares `unexpected_cfgs` must cover every `check-cfg`
   entry the workspace declares. A crate may add its own, `cfg(loom)` for
   instance, but dropping `cfg(fuzzing)` or `cfg(kani)` is how a future harness
   breaks the build in a crate nobody thought to look at.

Usage:
    python3 scripts/check_crate_lints.py
"""

import sys
import tomllib
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]


def check_cfg_set(lints: dict) -> set[str]:
    cfgs = lints.get("rust", {}).get("unexpected_cfgs", {})
    if isinstance(cfgs, dict):
        return set(cfgs.get("check-cfg", []))
    return set()


def main() -> int:
    root = tomllib.loads((REPO_ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    ws = root.get("workspace", {}).get("lints", {})
    ws_cfgs = check_cfg_set(ws)

    failures: list[str] = []
    inherited = local = 0

    for manifest in sorted((REPO_ROOT / "crates").glob("*/Cargo.toml")):
        name = manifest.parent.name
        lints = tomllib.loads(manifest.read_text(encoding="utf-8")).get("lints", {})

        if not lints:
            failures.append(
                f"{name}: no [lints] table at all, so it gets no workspace lint. "
                f"Add `[lints]` with `workspace = true`."
            )
            continue

        if lints.get("workspace") is True:
            inherited += 1
            # An inheriting crate must not also declare overrides: Cargo takes
            # `workspace = true` and ignores the rest, so a local rule here
            # would read as active while doing nothing.
            if lints.get("rust") or lints.get("clippy"):
                failures.append(
                    f"{name}: has `workspace = true` AND local lint tables. "
                    f"Cargo honours the inheritance and ignores the rest, so "
                    f"those local rules are inert. Pick one."
                )
            continue

        local += 1
        if lints.get("clippy", {}).get("unwrap_used") != "deny":
            failures.append(
                f"{name}: declares its own [lints], which REPLACES the workspace "
                f"table rather than extending it, and does not restate "
                f'`unwrap_used = "deny"`. Add it under [lints.clippy], or drop '
                f"the local table and inherit."
            )

        missing = ws_cfgs - check_cfg_set(lints)
        if missing and check_cfg_set(lints):
            failures.append(
                f"{name}: local `unexpected_cfgs` drops {sorted(missing)}, which "
                f"the workspace declares. A crate may add cfgs, never lose them."
            )

    print(f"{inherited} crates inherit the workspace lints, {local} restate them locally")

    if failures:
        print(f"\n{len(failures)} crate(s) would not get the lints they look like they get:\n",
              file=sys.stderr)
        for f in failures:
            print(f"  {f}\n", file=sys.stderr)
        return 1

    print("every crate is covered")
    return 0


if __name__ == "__main__":
    sys.exit(main())
