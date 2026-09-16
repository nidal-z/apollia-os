#!/usr/bin/env python3
"""The Windows package must carry python313.dll beside apollia-desktop.exe.

Why this is a guard and not a paragraph. PyO3 is taken with `auto-initialize`
(`Cargo.toml`), which links the binary against libpython instead of loading it on
demand. Measured on this tree, on the macOS build of the same configuration:

    $ otool -L target/debug/apollia-os | grep python
        @executable_path/../Resources/python/lib/libpython3.13.dylib

`LC_LOAD_DYLIB` is a load-time dependency. The dynamic loader resolves it before
`main`, so no code of Apollia runs until it is found. The Windows build of the
same configuration carries `python313.dll` in the import table of
`apollia-desktop.exe`, and the Windows loader searches, in order, the
executable's directory, the system directories, then `PATH`.

The bundled interpreter is declared as a Tauri resource under `python/**/*`, and
resources keep their relative path, so the DLL is installed at
`<install>\\python\\python313.dll`. The loader never looks there. macOS and Linux
escape the problem because the packaging rewrites the load path into the bundle
(`install_name_tool`, RPATH); Windows has no equivalent rewrite, and the `PATH`
prefix `setup_bundled_python` applies runs inside `main`, which is already too
late. It serves the interpreters spawned as subprocesses, not this one.

The effect, reported by a user on a managed machine with no administrator
rights: the installer runs, the application refuses to start on "python313.dll
not found", and the only machines where it starts are the ones that already have
a Python 3.13 on `PATH`, which contradicts the zero-dependency principle.

Two halves, and the guard checks both.

Static, always: `tauri.windows.conf.json` declares the DLL at the root of the
resources, and `bundle-cli.sh` stages it there. A rule without a guard is not
respected, and this one is invisible on a Mac: nothing in a macOS or Linux build
exercises it.

Dynamic, on demand: `--package <dir>` reads a built Windows package and refuses
one where the DLL does not sit beside the executable.

Run: python3 scripts/check_windows_python_dll.py
     python3 scripts/check_windows_python_dll.py --package <dir>
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

DESKTOP = REPO_ROOT / "crates" / "apollia-desktop"
WINDOWS_CONF = DESKTOP / "tauri.windows.conf.json"
BASE_CONF = DESKTOP / "tauri.conf.json"
STAGING_SCRIPT = DESKTOP / "scripts" / "bundle-cli.sh"

# The library the loader looks for, tied to the bundled interpreter's minor
# version like `bin/python3.13` and `lib/python3.13` elsewhere in the packaging.
DLL_NAME = "python313.dll"

# The executable the Windows installer places at the root of the install
# directory, and whose directory the loader searches first.
EXECUTABLE = "apollia-desktop.exe"


def read_resources(path: Path) -> list[str] | None:
    """The `bundle.resources` list of a Tauri config, or None when unreadable."""
    try:
        config = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    resources = config.get("bundle", {}).get("resources")
    if isinstance(resources, list):
        return [str(entry) for entry in resources]
    if isinstance(resources, dict):
        # The map form names the destination; a root destination is the DLL's
        # place, so the destinations are what this guard reads.
        return [str(dest) for dest in resources.values()]
    return None


def check_static() -> list[str]:
    """The wiring that puts the DLL beside the executable at package time."""
    failures: list[str] = []

    windows_resources = read_resources(WINDOWS_CONF)
    if windows_resources is None:
        failures.append(
            f"{WINDOWS_CONF.relative_to(REPO_ROOT)}: missing, unreadable, or "
            "declaring no bundle.resources list"
        )
    elif DLL_NAME not in windows_resources:
        failures.append(
            f"{WINDOWS_CONF.relative_to(REPO_ROOT)}: bundle.resources does not "
            f"carry `{DLL_NAME}` at the root, so the DLL is installed under "
            "python/ where the loader never looks"
        )
    else:
        # A platform config REPLACES the array it overrides (Tauri merges with a
        # JSON merge-patch, and a patch array replaces rather than appends), so
        # every base resource has to be restated here or it is dropped from the
        # Windows package.
        base_resources = read_resources(BASE_CONF) or []
        dropped = [entry for entry in base_resources if entry not in windows_resources]
        if dropped:
            failures.append(
                f"{WINDOWS_CONF.relative_to(REPO_ROOT)}: a platform config replaces "
                "the base array rather than extending it, and these entries of "
                f"tauri.conf.json are missing from it: {', '.join(dropped)}"
            )

    try:
        staging = STAGING_SCRIPT.read_text(encoding="utf-8")
    except OSError:
        staging = ""
        failures.append(f"{STAGING_SCRIPT.relative_to(REPO_ROOT)}: unreadable")
    if staging and DLL_NAME not in staging:
        failures.append(
            f"{STAGING_SCRIPT.relative_to(REPO_ROOT)}: never mentions {DLL_NAME}, "
            "so nothing copies it next to the executable before the bundle is built"
        )

    return failures


def check_package(root: Path) -> list[str]:
    """A built Windows package: the DLL beside every executable that needs it."""
    if not root.is_dir():
        return [f"{root}: not a directory"]

    executables = sorted(root.rglob(EXECUTABLE))
    if not executables:
        return [
            f"{root}: holds no {EXECUTABLE}, so this is not an unpacked Windows "
            "package and nothing was measured"
        ]

    failures: list[str] = []
    for executable in executables:
        beside = executable.parent / DLL_NAME
        if not beside.is_file():
            failures.append(
                f"{executable}: no {DLL_NAME} in its own directory. The loader "
                "resolves it before main, from this directory, the system "
                "directories, then PATH; a copy under python/ is not on that list."
            )
    return failures


def selftest() -> int:
    """Drive the package check in both directions on a fabricated package.

    A detector that only ever runs against a clean tree proves that the scan
    ran, not that the detector works. This guard has no compliant subject on a
    Mac at all: no Windows package is ever built here, so without this the
    dynamic half would be measured by nobody.
    """
    import tempfile

    findings: list[str] = []
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)

        # A faulty package: the DLL only where Tauri puts a resource.
        bad = root / "bad"
        (bad / "python").mkdir(parents=True)
        (bad / EXECUTABLE).touch()
        (bad / "python" / DLL_NAME).touch()
        if not check_package(bad):
            findings.append(
                "the check passed a package whose DLL sits only under python/, "
                "which is the exact layout that does not start"
            )

        # A correct package: the DLL beside the executable as well.
        good = root / "good"
        (good / "python").mkdir(parents=True)
        (good / EXECUTABLE).touch()
        (good / "python" / DLL_NAME).touch()
        (good / DLL_NAME).touch()
        if check_package(good):
            findings.append("the check refused a package that is correctly laid out")

        # A directory holding no executable measured nothing, and says so
        # rather than passing.
        empty = root / "empty"
        empty.mkdir()
        if not check_package(empty):
            findings.append(
                "the check passed a directory holding no executable, where it measured nothing"
            )

    print("selftest: the package check driven on a faulty, a correct and an empty subject")
    if findings:
        print(f"\n{len(findings)} finding(s) in the check itself:\n", file=sys.stderr)
        for finding in findings:
            print(f"  {finding}", file=sys.stderr)
        return 1
    print(
        "selftest: it refuses the faulty one, accepts the correct one, measures nothing on the empty one"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--package",
        type=Path,
        default=None,
        help="directory of an unpacked Windows package to inspect",
    )
    parser.add_argument(
        "--selftest",
        action="store_true",
        help="drive the package check on a fabricated faulty and correct package",
    )
    args = parser.parse_args()

    if args.selftest:
        return selftest()

    if args.package is not None:
        failures = check_package(args.package)
        print(f"package inspected: {args.package}")
    else:
        failures = check_static()
        print(
            f"packaging wiring inspected: {WINDOWS_CONF.relative_to(REPO_ROOT)} "
            f"and {STAGING_SCRIPT.relative_to(REPO_ROOT)}"
        )

    if not failures:
        print(f"{DLL_NAME} sits beside the executable, or is wired to")
        return 0

    print(
        f"\n{len(failures)} finding(s). Without {DLL_NAME} in the executable's own "
        "directory, apollia-desktop.exe does not start on a machine that has no "
        "Python 3.13 of its own on PATH.\n",
        file=sys.stderr,
    )
    for failure in failures:
        print(f"  {failure}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
