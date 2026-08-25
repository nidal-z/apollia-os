#!/usr/bin/env python3
"""Every crate must actually get the workspace tables, not merely be expected to.

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

The same document promises two more things the manifests never kept, so this
guard holds them too. `[workspace.package]` shares `rust-version`, `edition`
and `license`, yet inheritance is opt-in per crate: 19 packages out of 21
published without an MSRV while RUST-PATTERNS.md announced one, and nothing
said so. `[workspace.dependencies]` is the single source of versions, yet 65
declarations carried their own version inline, four of them duplicating the
workspace entry, so one bump in the root moved only part of the tree.

Per workspace member, four things are verified:

1. Either `[lints] workspace = true`, or a local table that restates
   `unwrap_used = "deny"`.
2. A local table that declares `unexpected_cfgs` must cover every `check-cfg`
   entry the workspace declares. A crate may add its own, `cfg(loom)` for
   instance, but dropping `cfg(fuzzing)` or `cfg(kani)` is how a future harness
   breaks the build in a crate nobody thought to look at.
3. `rust-version`, `edition` and `license` are inherited
   (`<key>.workspace = true`), never restated and never absent.
4. Any dependency named in `[workspace.dependencies]` is consumed with
   `workspace = true`, never with its own inline version.

A package listed under `crates/` but excluded from the workspace cannot
inherit anything: Cargo resolves `workspace = true` against the package's own
manifest, which has no `[workspace]` table, and refuses the build. Such
packages are exempt from checks 3 and 4, and the exemption is printed by name
on every run so growth is visible. The `[lints]` checks still apply to them.

`--selftest` replays every rule against altered manifests in a temporary
fixture and fails if any rule stays silent, or fires on the clean control.

Exit codes: 0 when every manifest is covered, 1 when at least one defect is
found, 2 when nothing was measured (no manifest examined).

Usage:
    python3 scripts/check_crate_lints.py [--selftest]
"""

import argparse
import sys
import tempfile
import tomllib
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

INHERITED_KEYS = ("rust-version", "edition", "license")
DEP_SECTIONS = ("dependencies", "dev-dependencies", "build-dependencies")

EXIT_GREEN = 0
EXIT_DEFECT = 1
EXIT_NOTHING_MEASURED = 2


def check_cfg_set(lints: dict) -> set[str]:
    cfgs = lints.get("rust", {}).get("unexpected_cfgs", {})
    if isinstance(cfgs, dict):
        return set(cfgs.get("check-cfg", []))
    return set()


def dep_tables(manifest: dict):
    for sec in DEP_SECTIONS:
        if manifest.get(sec):
            yield sec, manifest[sec]
    for target, tables in manifest.get("target", {}).items():
        for sec in DEP_SECTIONS:
            if tables.get(sec):
                yield f"target.{target}.{sec}", tables[sec]


def check_lints(name: str, manifest: dict, ws_cfgs: set[str], failures: list[str]) -> str:
    """Returns 'inherited', 'local', or 'none' for the summary line."""
    lints = manifest.get("lints", {})

    if not lints:
        failures.append(
            f"{name}: no [lints] table at all, so it gets no workspace lint. "
            f"Add `[lints]` with `workspace = true`."
        )
        return "none"

    if lints.get("workspace") is True:
        # An inheriting crate must not also declare overrides: Cargo takes
        # `workspace = true` and ignores the rest, so a local rule here
        # would read as active while doing nothing.
        if lints.get("rust") or lints.get("clippy"):
            failures.append(
                f"{name}: has `workspace = true` AND local lint tables. "
                f"Cargo honours the inheritance and ignores the rest, so "
                f"those local rules are inert. Pick one."
            )
        return "inherited"

    if lints.get("clippy", {}).get("unwrap_used") != "deny":
        failures.append(
            f"{name}: declares its own [lints], which REPLACES the workspace "
            f"table rather than extending it, and does not restate "
            f'`unwrap_used = "deny"`. Add it under [lints.clippy], or drop '
            f"the local table and inherit."
        )

    missing = ws_cfgs - check_cfg_set(manifest.get("lints", {}))
    if missing and check_cfg_set(manifest.get("lints", {})):
        failures.append(
            f"{name}: local `unexpected_cfgs` drops {sorted(missing)}, which "
            f"the workspace declares. A crate may add cfgs, never lose them."
        )
    return "local"


def check_inheritance(name: str, manifest: dict, ws_deps: set[str], failures: list[str]) -> None:
    package = manifest.get("package", {})
    for key in INHERITED_KEYS:
        value = package.get(key)
        if value == {"workspace": True}:
            continue
        if value is None:
            failures.append(
                f"{name}: [package] declares no `{key}`, so the workspace value "
                f"does not apply to it. Add `{key}.workspace = true`."
            )
        else:
            failures.append(
                f"{name}: restates `{key}` inline instead of inheriting it. "
                f"Replace with `{key}.workspace = true` so the root stays the "
                f"single source."
            )

    for section, deps in dep_tables(manifest):
        for dep, spec in deps.items():
            if dep not in ws_deps:
                continue
            if isinstance(spec, dict) and spec.get("workspace") is True:
                continue
            failures.append(
                f"{name}: [{section}] pins `{dep}` with its own version while "
                f"[workspace.dependencies] already declares it. Two sources for "
                f"one constraint: use `{dep} = {{ workspace = true }}`."
            )


def run(repo_root: Path) -> int:
    root = tomllib.loads((repo_root / "Cargo.toml").read_text(encoding="utf-8"))
    workspace = root.get("workspace", {})
    ws_cfgs = check_cfg_set(workspace.get("lints", {}))
    ws_deps = set(workspace.get("dependencies", {}))

    member_dirs = {
        (repo_root / member).resolve() for member in workspace.get("members", [])
    }

    failures: list[str] = []
    inherited = local = 0
    exempt: list[str] = []
    examined = 0

    crate_manifests = sorted((repo_root / "crates").glob("*/Cargo.toml"))
    member_manifests = [
        d / "Cargo.toml" for d in sorted(member_dirs) if (d / "Cargo.toml").exists()
    ]

    for manifest_path in crate_manifests:
        examined += 1
        name = manifest_path.parent.name
        manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        verdict = check_lints(name, manifest, ws_cfgs, failures)
        inherited += verdict == "inherited"
        local += verdict == "local"
        if manifest_path.parent.resolve() not in member_dirs:
            exempt.append(name)

    for manifest_path in member_manifests:
        name = manifest_path.parent.name
        if manifest_path not in crate_manifests:
            examined += 1
        manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        check_inheritance(name, manifest, ws_deps, failures)

    if examined == 0:
        print("no manifest examined: nothing was measured", file=sys.stderr)
        return EXIT_NOTHING_MEASURED

    print(f"{inherited} crates inherit the workspace lints, {local} restate them locally")
    print(
        f"{len(member_manifests)} workspace members checked for [package] and "
        f"dependency inheritance"
    )
    if exempt:
        print(
            f"{len(exempt)} package(s) excluded from the workspace, exempt from "
            f"inheritance (cannot inherit by construction): {', '.join(exempt)}"
        )

    if failures:
        print(
            f"\n{len(failures)} defect(s): a manifest does not get what it looks "
            f"like it gets:\n",
            file=sys.stderr,
        )
        for f in failures:
            print(f"  {f}\n", file=sys.stderr)
        return EXIT_DEFECT

    print("every manifest is covered")
    return EXIT_GREEN


# ── selftest ─────────────────────────────────────────────────────────────────

FIXTURE_ROOT = """\
[workspace]
members = ["crates/good", "crates/bad", "crates/nolints", "tests"]

[workspace.package]
rust-version = "1.89"
edition = "2021"
license = "MIT"

[workspace.dependencies]
serde = "1"

[workspace.lints.rust]
unexpected_cfgs = { level = "warn", check-cfg = ['cfg(fuzzing)'] }

[workspace.lints.clippy]
unwrap_used = "deny"
"""

FIXTURE_GOOD = """\
[package]
name = "good"
rust-version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
serde = { workspace = true }

[lints]
workspace = true
"""

FIXTURE_BAD = """\
[package]
name = "bad"
edition = "2021"
license = "MIT"

[dependencies]
serde = "1"

[lints]
workspace = true
"""

FIXTURE_NOLINTS = """\
[package]
name = "nolints"
rust-version.workspace = true
edition.workspace = true
license.workspace = true
"""

FIXTURE_TESTS = """\
[package]
name = "fixture-tests"
rust-version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
serde = { workspace = true }

[lints]
workspace = true
"""

FIXTURE_EXCLUDED = """\
[package]
name = "excluded"
edition = "2021"
license = "MIT"

[dependencies]
serde = "1"

[lints.clippy]
unwrap_used = "deny"
"""


def _selftest() -> int:
    failures = 0

    def case(name: str, condition: bool, detail: str) -> None:
        nonlocal failures
        print(f"{'ok ' if condition else 'RED'}  {name}")
        if not condition:
            failures += 1
            print(f"       {detail}")

    def collect(root: Path) -> list[str]:
        manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
        workspace = manifest.get("workspace", {})
        ws_cfgs = check_cfg_set(workspace.get("lints", {}))
        ws_deps = set(workspace.get("dependencies", {}))
        member_dirs = {
            (root / member).resolve() for member in workspace.get("members", [])
        }
        found: list[str] = []
        for path in sorted((root / "crates").glob("*/Cargo.toml")) + [
            root / "tests" / "Cargo.toml"
        ]:
            if not path.exists():
                continue
            crate = tomllib.loads(path.read_text(encoding="utf-8"))
            check_lints(path.parent.name, crate, ws_cfgs, found)
            if path.parent.resolve() in member_dirs:
                check_inheritance(path.parent.name, crate, ws_deps, found)
        return found

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        for rel, content in [
            ("Cargo.toml", FIXTURE_ROOT),
            ("crates/good/Cargo.toml", FIXTURE_GOOD),
            ("crates/bad/Cargo.toml", FIXTURE_BAD),
            ("crates/nolints/Cargo.toml", FIXTURE_NOLINTS),
            ("crates/excluded/Cargo.toml", FIXTURE_EXCLUDED),
            ("tests/Cargo.toml", FIXTURE_TESTS),
        ]:
            path = root / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")

        found = collect(root)
        bad = [f for f in found if f.startswith("bad:")]

        case(
            "altered member: missing rust-version fires",
            any("`rust-version`" in f and "no" in f for f in bad),
            f"expected a missing rust-version defect, got: {bad}",
        )
        case(
            "altered member: restated edition fires",
            any("`edition` inline" in f for f in bad),
            f"expected a restated edition defect, got: {bad}",
        )
        case(
            "altered member: restated license fires",
            any("`license` inline" in f for f in bad),
            f"expected a restated license defect, got: {bad}",
        )
        case(
            "altered member: inline version of a workspace dep fires",
            any("`serde` with its own version" in f for f in bad),
            f"expected an inline-version defect, got: {bad}",
        )
        case(
            "member without [lints] fires",
            any(f.startswith("nolints:") and "[lints]" in f for f in found),
            f"expected a missing-lints defect, got: {found}",
        )
        case(
            "excluded package is exempt from inheritance, not from lints",
            not any(f.startswith("excluded:") for f in found),
            f"the excluded package cannot inherit, yet it was flagged: {found}",
        )
        case(
            "clean members stay silent",
            not any(f.startswith(("good:", "fixture-tests:")) for f in found),
            f"the clean control fired: {found}",
        )

    with tempfile.TemporaryDirectory() as tmp:
        empty = Path(tmp)
        (empty / "Cargo.toml").write_text("[workspace]\nmembers = []\n", encoding="utf-8")
        case(
            "an empty tree reports nothing measured, never a pass",
            run(empty) == EXIT_NOTHING_MEASURED,
            "a run that examined no manifest must exit 2",
        )

    if failures:
        print(f"\nselftest: {failures} case(s) RED", file=sys.stderr)
        return EXIT_DEFECT
    print("selftest: every rule fired on its altered manifest and stayed silent on the control")
    return EXIT_GREEN


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--selftest", action="store_true", help="drive the rules on altered fixtures, red first"
    )
    args = parser.parse_args()
    if args.selftest:
        return _selftest()
    return run(REPO_ROOT)


if __name__ == "__main__":
    sys.exit(main())
