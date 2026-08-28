#!/usr/bin/env python3
"""Cross the sources the documentation generators declare with the tree.

Four generators under `docs/site/scripts/` derive a published page from the
code: the configuration reference from the Rust config structs, the evaluation
schema from the suite parser, the EventBus catalogue from the runtime enum and
the desktop bridge, and the seventeen SDK pages from the `Ctx` protocol. Each
one points at a file and at a symbol inside it, and until this guard nothing
crossed those pointers with the tree.

That is not a hypothetical. `gen_config_ref.py` looked for `LlmConfig` in
`crates/apollia-llm/src/router.rs`; a module split moved the struct to
`router/config.rs`; the generator printed a warning, exited 0, and every run of
`docs/site/regen.sh` deleted the whole `### [llm]` table from the published
reference. The break lived through nine waves because the only thing that could
have caught it was a human reading a regeneration diff.

Two halves close it, and this is the outer one. The inner half is
`docs/site/scripts/declared_sources.py`: a generator declares its sources and
refuses to write when one of them no longer resolves. That half only fires when
someone regenerates. This half fires at the commit that moves the file, which is
where the person who can repair it is standing.

Four rules, all mechanical:

  * every `gen_*.py` declares a `SOURCES` list. A generator that declares
    nothing cannot be crossed with anything;
  * every `gen_*.py` calls `declared_sources.require`. A declaration no code
    reads is a comment, and a comment does not stop a regeneration;
  * every declared source resolves: the path is in the tree, and the symbol the
    generator looks for is in that file;
  * every `gen_*.py` is launched by `docs/site/regen.sh`. A generator nobody
    runs publishes nothing, and its page is stale by definition.

Verdict by exit code, since the caller reads it rather than the text:

  0  every generator declares its sources, enforces them, is launched, and
     every declared source resolves
  1  at least one declaration does not hold
  2  nothing was measured: the generator directory is absent, or it holds no
     `gen_*.py`

Usage:
    python3 scripts/check_doc_generators.py
    python3 scripts/check_doc_generators.py --selftest
"""

import argparse
import importlib.util
import shutil
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
GENERATORS = REPO_ROOT / "docs" / "site" / "scripts"
REGEN = REPO_ROOT / "docs" / "site" / "regen.sh"

ENFORCER = "declared_sources.require"


def load(path: Path):
    """Import a generator by path, under a name of its own.

    The module is imported rather than parsed, because `SOURCES` is built from
    a comprehension in one generator and a literal list in the others. Reading
    it as text would mean re-implementing that comprehension here, which is the
    second source this guard exists to refuse.
    """
    name = f"_docgen_{path.parent.name}_{path.stem}"
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise ImportError(f"no loader for {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def cross(generators_dir: Path, root: Path, launcher: Path | None) -> tuple[int, int, list[str]]:
    """Return (generators read, sources declared, findings).

    `root` is the tree the declared paths are resolved against, and `launcher`
    the script expected to run each generator. Both are parameters so the
    selftest exercises the two directions on a fixture rather than on the tree.
    """
    findings: list[str] = []
    declared_total = 0
    scripts = sorted(generators_dir.glob("gen_*.py"))
    launched = launcher.read_text(encoding="utf-8") if launcher and launcher.exists() else None

    original_path = list(sys.path)
    sys.path.insert(0, str(generators_dir))
    try:
        for script in scripts:
            rel = script.name
            try:
                module = load(script)
            except Exception as exc:  # a generator that cannot import runs nothing
                findings.append(f"{rel}: cannot be imported ({exc.__class__.__name__}: {exc})")
                continue

            text = script.read_text(encoding="utf-8")
            sources = getattr(module, "SOURCES", None)
            if sources is None:
                findings.append(
                    f"{rel}: declares no SOURCES, so no pointer of it can be crossed"
                )
            elif not sources:
                findings.append(f"{rel}: declares an empty SOURCES")
            else:
                declared_total += len(sources)
                for source in sources:
                    reason = source.unresolved(root)
                    if reason is not None:
                        findings.append(f"{rel}: {reason}")

            if ENFORCER not in text:
                findings.append(
                    f"{rel}: never calls {ENFORCER}, so its declaration stops nothing"
                )

            if launched is not None and rel not in launched:
                findings.append(
                    f"{rel}: is not launched by {launcher.name}, so its page is never rewritten"
                )
    finally:
        sys.path[:] = original_path

    return len(scripts), declared_total, findings


def report(findings: list[str]) -> None:
    print()
    for finding in findings:
        print(f"  BROKEN  {finding}")
    print()
    print("A generator whose pointer no longer resolves does not stop: it publishes")
    print("the page with the section missing, which reads like a page that never had")
    print("one. Repair the pointer in the generator, or move the symbol back.")


def selftest() -> int:
    """Both directions, on a fixture, so a green tree proves nothing on its own."""
    print("selftest: a fixture generator directory, one compliant and four faulty")
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        gens = root / "docs" / "site" / "scripts"
        gens.mkdir(parents=True)
        shutil.copy(GENERATORS / "declared_sources.py", gens / "declared_sources.py")

        (root / "crates").mkdir()
        (root / "crates" / "present.rs").write_text("pub struct Kept {}\n", encoding="utf-8")

        head = (
            "from declared_sources import Source\n"
            "import declared_sources\n"
        )
        (gens / "gen_ok.py").write_text(
            head
            + 'SOURCES = [Source("crates/present.rs", "pub struct Kept")]\n'
            + "def main():\n    return declared_sources.require('gen_ok', SOURCES)\n",
            encoding="utf-8",
        )
        (gens / "gen_dead_path.py").write_text(
            head
            + 'SOURCES = [Source("crates/gone.rs", "pub struct Kept")]\n'
            + "def main():\n    return declared_sources.require('gen_dead_path', SOURCES)\n",
            encoding="utf-8",
        )
        (gens / "gen_dead_symbol.py").write_text(
            head
            + 'SOURCES = [Source("crates/present.rs", "pub struct Moved")]\n'
            + "def main():\n    return declared_sources.require('gen_dead_symbol', SOURCES)\n",
            encoding="utf-8",
        )
        (gens / "gen_undeclared.py").write_text("VALUE = 1\n", encoding="utf-8")
        (gens / "gen_unenforced.py").write_text(
            head + 'SOURCES = [Source("crates/present.rs", "pub struct Kept")]\n',
            encoding="utf-8",
        )
        launcher = root / "regen.sh"
        launcher.write_text(
            "python3 gen_ok.py\npython3 gen_dead_path.py\npython3 gen_dead_symbol.py\n"
            "python3 gen_undeclared.py\npython3 gen_unenforced.py\n",
            encoding="utf-8",
        )

        read, declared, findings = cross(gens, root, launcher)
        failures = []

        if read != 5:
            failures.append(f"5 fixture generators were laid down, {read} were read")
        if declared != 4:
            failures.append(f"4 sources were declared across the fixture, {declared} were counted")

        expected = {
            "gen_dead_path.py": "does not exist",
            "gen_dead_symbol.py": "is not in",
            "gen_undeclared.py": "declares no SOURCES",
            "gen_unenforced.py": "never calls",
        }
        for name, fragment in expected.items():
            hit = [f for f in findings if f.startswith(name) and fragment in f]
            if hit:
                print(f"  ok    positive control: {name} is reported")
            else:
                print(f"  FAIL  positive control: {name} is not reported")
                failures.append(f"{name} was not reported for {fragment!r}: {findings!r}")

        if any(f.startswith("gen_ok.py") for f in findings):
            print("  FAIL  negative control: the compliant generator is reported")
            failures.append(
                f"gen_ok.py resolves, is enforced and is launched, yet it was "
                f"reported: {findings!r}"
            )
        else:
            print("  ok    negative control: the compliant generator is not reported")

        # A launcher that names nothing must accuse every generator, and the
        # absence of a launcher must accuse none: without this the launch rule
        # is satisfied by any file that happens to contain the word.
        empty = root / "empty.sh"
        empty.write_text("echo nothing\n", encoding="utf-8")
        _read, _declared, silent_launcher = cross(gens, root, empty)
        unlaunched = [f for f in silent_launcher if "is not launched" in f]
        if len(unlaunched) == 5:
            print("  ok    positive control: a launcher that runs nothing accuses all five")
        else:
            print("  FAIL  positive control: a launcher that runs nothing accuses all five")
            failures.append(f"{len(unlaunched)} of 5 were accused: {silent_launcher!r}")

        _read, _declared, no_launcher = cross(gens, root, None)
        if any("is not launched" in f for f in no_launcher):
            print("  FAIL  negative control: no launcher accuses nobody of not being launched")
            failures.append(f"the launch rule fired without a launcher: {no_launcher!r}")
        else:
            print("  ok    negative control: no launcher accuses nobody of not being launched")

        # Nothing measured is not a failure, and it is not a success either.
        bare = root / "bare"
        bare.mkdir()
        if cross(bare, root, launcher)[0] == 0:
            print("  ok    an empty directory yields zero generators read")
        else:
            print("  FAIL  an empty directory yields zero generators read")
            failures.append("a directory with no gen_*.py was not read as empty")

    if failures:
        print()
        for failure in failures:
            print(f"  FAIL  {failure}")
        return 1
    print("selftest: both directions hold")
    return 0


def main() -> int:
    if not GENERATORS.is_dir():
        print(
            f"NOTHING MEASURED: {GENERATORS.relative_to(REPO_ROOT)} is absent, so no "
            "generator was crossed with anything",
            file=sys.stderr,
        )
        return 2

    read, declared, findings = cross(GENERATORS, REPO_ROOT, REGEN)
    if read == 0:
        print(
            f"NOTHING MEASURED: no gen_*.py under {GENERATORS.relative_to(REPO_ROOT)}",
            file=sys.stderr,
        )
        return 2

    print(f"check_doc_generators: {read} generator(s) read, {declared} declared source(s) crossed")
    if findings:
        print(f"check_doc_generators: {len(findings)} declaration(s) do not hold")
        report(findings)
        return 1
    print("check_doc_generators: every declared source is where its generator looks for it")
    return 0


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--selftest",
        action="store_true",
        help="replay both directions on a temporary fixture, never on the tree",
    )
    args = parser.parse_args()
    sys.exit(selftest() if args.selftest else main())
