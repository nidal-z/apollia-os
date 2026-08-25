#!/usr/bin/env python3
"""List the desktop UI sources that no bundle entry reaches.

Builds the static import graph from the two Vite entries (`src/main.ts`,
`src/overlay.ts`) by following every `import ... from "..."`, `import "..."`,
`export ... from "..."` and dynamic `import("...")` whose specifier is relative
or starts with `$lib/`. A file the graph never reaches is shipped by nobody:
the bundle drops it, `svelte-check` still type-checks it, and a defect in it
is invisible until someone imports it again. Four components and a stale
`knip.json` sat in exactly that state when this guard was written.

Barrels are resolved by name: an `index.ts` only hands out the files whose
exports the importer actually names, so a component re-exported by a barrel
that nothing ever asks for stays unreached instead of being laundered into
the graph by the barrel's own existence.

One shape is tolerated: a `.ts` module whose only importer is a `*.test.ts`
file. `crates/apollia-desktop/ui/AGENTS.md` section 11 sanctions exporting
the logic under test, and `lib/i18n/catalogueDuplicateKeys.ts` lives that way
while its test guards the real catalogues. A `.svelte` file gets no such
tolerance: Vitest cannot mount a component here, so a component imported only
by a test is dead however green the test is.

Verdict by exit code, since the caller reads it rather than the text:

  0  every source is reached from an entry, or is a test-backed `.ts` module
  1  at least one source is unreached and not tolerated
  2  nothing was measured: the subtree or the entries are absent

`--entries <list>` restricts the roots (comma-separated, relative to the ui
directory); it exists so the guard can be watched reporting unreached files
on a healthy tree (positive control). `--selftest` checks the guard itself
against a built subject.
"""

import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
UI_SUBTREE = Path("crates/apollia-desktop/ui")
DEFAULT_ENTRIES = ["src/main.ts", "src/overlay.ts"]

NAMED_IMPORT = re.compile(
    r"""(?:^|[^\w$.])import\s+(?:type\s+)?\{([^}]*)\}\s*from\s*['"]([^'"]+)['"]""", re.M
)
BARREL_EXPORT = re.compile(
    r"""export\s*\{\s*([^}]*)\}\s*from\s*['"]([^'"]+)['"]|export\s*\*\s*from\s*['"]([^'"]+)['"]""",
    re.M,
)
ANY_SPECIFIER = re.compile(
    r"""(?:^|[^\w$.])(?:import|export)\s*(?:type\s+)?(?:[^'"]*?\sfrom\s*)?['"]([^'"]+)['"]"""
    r"""|import\(\s*['"]([^'"]+)['"]\s*\)""",
    re.M,
)


def _resolve(spec: str, from_file: Path, src_root: Path) -> Path | None:
    if spec.startswith("$lib/"):
        base = src_root / "lib" / spec[len("$lib/"):]
    elif spec.startswith("."):
        base = (from_file.parent / spec).resolve()
    else:
        return None
    candidates = [base]
    for ext in (".ts", ".svelte", ".js", ".json", ".css"):
        candidates.append(base.with_name(base.name + ext))
    for index in ("index.ts", "index.js"):
        candidates.append(base / index)
    for candidate in candidates:
        if candidate.is_file():
            # Canonical form, so `/tmp`-style symlinks cannot split one file
            # into two identities between the graph and the tracked listing.
            return candidate.resolve()
    return None


def _imports_of(path: Path):
    """Yield (specifier, named-imports-or-None) pairs for one source file."""
    text = path.read_text(encoding="utf-8", errors="replace")
    named: dict[str, set[str]] = {}
    for match in NAMED_IMPORT.finditer(text):
        names = {
            entry.strip().split(" as ")[0].replace("type ", "").strip()
            for entry in match.group(1).split(",")
            if entry.strip()
        }
        named.setdefault(match.group(2), set()).update(names)
    seen: set[str] = set()
    for match in ANY_SPECIFIER.finditer(text):
        spec = match.group(1) or match.group(2)
        if not spec or spec in seen:
            continue
        seen.add(spec)
        yield spec, named.get(spec)


def _barrel_targets(barrel: Path, names: set[str] | None, src_root: Path) -> list[Path]:
    """Files a barrel hands out for the given names, or all of them."""
    text = barrel.read_text(encoding="utf-8", errors="replace")
    out: list[Path] = []
    for match in BARREL_EXPORT.finditer(text):
        if match.group(3):
            target = _resolve(match.group(3), barrel, src_root)
            if target:
                out.append(target)
            continue
        exported = {
            entry.strip().split(" as ")[-1].strip()
            for entry in match.group(1).split(",")
            if entry.strip()
        }
        if names is None or exported & names:
            target = _resolve(match.group(2), barrel, src_root)
            if target:
                out.append(target)
    return out


def report(ui_root: Path, entries: list[str]) -> int:
    ui_root = ui_root.resolve()
    src_root = ui_root / "src"
    # A pre-commit hook inherits GIT_DIR / GIT_WORK_TREE / GIT_INDEX_FILE
    # aimed at the repository root; kept, they would make this listing read
    # `src` against the wrong tree and report nothing measured.
    environment = {
        key: value for key, value in os.environ.items() if not key.startswith("GIT_")
    }
    listing = subprocess.run(
        ["git", "ls-files", "src"],
        cwd=ui_root,
        capture_output=True,
        text=True,
        env=environment,
    )
    if listing.returncode != 0 or not src_root.is_dir():
        print(f"NOTHING MEASURED: {src_root} is absent or not tracked.")
        return 2
    tracked = [ui_root / line for line in listing.stdout.split()]
    sources = [
        path
        for path in tracked
        if path.suffix in (".ts", ".svelte")
        and not path.name.endswith(".test.ts")
        and not path.name.endswith(".d.ts")
        and path.is_file()
    ]
    roots = [ui_root / entry for entry in entries if (ui_root / entry).is_file()]
    if not sources or not roots:
        print("NOTHING MEASURED: no source file or no entry point found.")
        return 2

    reached: set[Path] = set(roots)
    stack = list(roots)
    unresolved = 0
    while stack:
        current = stack.pop()
        for spec, names in _imports_of(current):
            target = _resolve(spec, current, src_root)
            if target is None:
                if spec.startswith((".", "$lib/")):
                    unresolved += 1
                    print(f"unresolved: {current.relative_to(ui_root)} -> {spec}")
                continue
            if target.name in ("index.ts", "index.js") and names is not None:
                # A barrel only hands out what the importer names; its own
                # plain imports (side effects, css) still count in full.
                reached.add(target)
                for handed in _barrel_targets(target, names, src_root):
                    if handed not in reached:
                        reached.add(handed)
                        stack.append(handed)
                barrel_text = target.read_text(encoding="utf-8", errors="replace")
                reexported = {
                    match.group(2) or match.group(3)
                    for match in BARREL_EXPORT.finditer(barrel_text)
                }
                for spec2, _names2 in _imports_of(target):
                    if spec2 in reexported:
                        continue
                    plain = _resolve(spec2, target, src_root)
                    if plain is not None and plain not in reached:
                        reached.add(plain)
                        stack.append(plain)
                continue
            if target not in reached:
                reached.add(target)
                stack.append(target)

    # Importers of each unreached source, to tell test-backed modules apart.
    importers: dict[Path, set[Path]] = {}
    for path in tracked:
        if path.suffix not in (".ts", ".svelte") or not path.is_file():
            continue
        for spec, _names in _imports_of(path):
            target = _resolve(spec, path, src_root)
            if target is not None:
                importers.setdefault(target, set()).add(path)

    tolerated: list[Path] = []
    defects: list[Path] = []
    for source in sorted(path for path in sources if path not in reached):
        who = importers.get(source, set())
        test_backed = source.suffix == ".ts" and any(
            importer.name.endswith(".test.ts") for importer in who
        )
        (tolerated if test_backed else defects).append(source)
        tag = "test-only" if test_backed else "unreached"
        lines = sum(1 for _ in source.open(encoding="utf-8", errors="replace"))
        print(f"{tag:10} {lines:5d}  {source.relative_to(ui_root)}")

    print(
        f"sources: {len(sources)}, reached from entries: "
        f"{len([s for s in sources if s in reached])}, unreached: {len(defects)}, "
        f"test-backed .ts tolerated: {len(tolerated)}, "
        f"unresolved specifiers: {unresolved}"
    )
    if defects:
        print(f"FAIL: {len(defects)} source(s) reached by no bundle entry.")
        return 1
    print("OK: every source is reached from a bundle entry, or is a test-backed module")
    return 0


# ── self-test ────────────────────────────────────────────────────────────────


def _write(root: Path, relative: str, body: str) -> None:
    target = root / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(body, encoding="utf-8")


def _case(name: str, condition: bool) -> bool:
    print(f"  {'ok  ' if condition else 'FAIL'}  {name}")
    return condition


def _build_subject(root: Path, with_orphan: bool) -> Path:
    """A miniature ui subtree under its own git index."""
    ui = root / "ui"
    _write(ui, "src/main.ts", 'import { Used } from "$lib/components";\nUsed;\n')
    _write(
        ui,
        "src/lib/components/index.ts",
        'export { default as Used } from "./Used.svelte";\n'
        'export { default as Orphan } from "./Orphan.svelte";\n',
    )
    _write(ui, "src/lib/components/Used.svelte", "<div>used</div>\n")
    if with_orphan:
        _write(ui, "src/lib/components/Orphan.svelte", "<div>orphan</div>\n")
    _write(ui, "src/lib/logic.ts", "export function pure(): number { return 1; }\n")
    _write(ui, "src/lib/logic.test.ts", 'import { pure } from "./logic";\npure();\n')
    environment = {
        key: value for key, value in os.environ.items() if not key.startswith("GIT_")
    }
    subprocess.run(["git", "init", "-q"], cwd=ui, check=True, env=environment)
    subprocess.run(["git", "add", "-A"], cwd=ui, check=True, env=environment)
    return ui


def selftest() -> int:
    print("unimported files: both directions on a built subject")
    results: list[bool] = []
    with tempfile.TemporaryDirectory(prefix="check-unimported-") as tmp:
        ui = _build_subject(Path(tmp), with_orphan=True)
        import contextlib
        import io

        buffer = io.StringIO()
        with contextlib.redirect_stdout(buffer):
            code = report(ui, ["src/main.ts"])
        text = buffer.getvalue()
        results.append(
            _case(
                "a component only a barrel re-exports is a red, and it is named",
                code == 1 and "src/lib/components/Orphan.svelte" in text,
            )
        )
        results.append(
            _case(
                "a .ts module whose only importer is a test is tolerated",
                "test-only" in text and "src/lib/logic.ts" in text,
            )
        )
    with tempfile.TemporaryDirectory(prefix="check-unimported-") as tmp:
        ui = _build_subject(Path(tmp), with_orphan=False)
        import contextlib
        import io

        buffer = io.StringIO()
        with contextlib.redirect_stdout(buffer):
            code = report(ui, ["src/main.ts"])
        results.append(
            _case("positive control: the same tree without the orphan is green", code == 0)
        )
        buffer = io.StringIO()
        with contextlib.redirect_stdout(buffer):
            code = report(ui, ["src/absent.ts"])
        results.append(
            _case("an absent entry measures nothing", code == 2)
        )
    print()
    if all(results):
        print(f"self-test: all {len(results)} cases pass")
        return 0
    print(f"self-test: {results.count(False)} of {len(results)} cases fail")
    return 1


def main() -> None:
    argv = sys.argv[1:]
    if "--selftest" in argv:
        sys.exit(selftest())
    entries = DEFAULT_ENTRIES
    if "--entries" in argv:
        index = argv.index("--entries")
        if index + 1 >= len(argv):
            print("usage: check_unimported_files.py [--entries src/a.ts,src/b.ts]")
            sys.exit(2)
        entries = argv[index + 1].split(",")
    ui_root = REPO_ROOT / UI_SUBTREE
    if not ui_root.is_dir():
        print(f"NOTHING MEASURED: {ui_root} is absent from this tree.")
        sys.exit(2)
    sys.exit(report(ui_root, entries))


if __name__ == "__main__":
    main()
