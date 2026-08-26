#!/usr/bin/env python3
"""The guard table of the method describes the guards the tree really runs.

`docs/internal/method/reference/gardes.md` is the table a framing reads to know
which commands protect the surface a lot is about to touch. It is not tracked by
git, so no prose guard, no hook and no crossing ever looked at it, and it drifted
the way an untracked table drifts: on the tree this file was written for, the
`guards` recipe of the justfile launched fifty-nine commands and the table
carried a row for eighteen of them. Forty-one guards, several of them the only
thing standing between a rule and its violation, were invisible to the reader who
consults the table to decide what to run.

Drift in the other direction costs more. A row that declares `pré-commit` for a
guard no hook launches promises a protection nobody has, and a row that names a
`scripts/check_*.py` the tree no longer carries sends a reader after a file that
does not exist. Both were present.

Three rules, each falsifiable on its own:

  coverage    Every subject the `guards` recipe of the justfile launches has a
              row. The recipe is the local boundary of the corpus, so a guard it
              launches and the table ignores is a guard the method cannot name.

  subject     Every repository path a row's command names exists, and every
              `just <recipe>` it names is a recipe of the justfile. A row whose
              subject is gone is a row nobody can replay.

  location    The location cell of a row declares its boundaries as a bracketed
              token list, and the declared set equals the measured set. The four
              mechanical tokens are `just guards`, `pré-commit`, `CI` and
              `verdicts`, read from the justfile, from `.pre-commit-config.yaml`,
              from `.github/workflows/*.yml` and from the `GUARDS` table of
              `scripts/worktree_verdicts.py`. `local` and `local préparé` are
              declarative: no boundary file carries them, they are established by
              running the guard in a fresh worktree, and this file accepts them
              without checking them rather than pretending otherwise.

A subject that starts with `scripts/` is matched against a launching line by
exact equality once `python3 ` and surrounding backticks are stripped, so
`scripts/check_cli_json_contract.py` and
`scripts/check_cli_json_contract.py taxonomy` stay two subjects with two rows.
Any other subject, an external gate whose invocation the boundaries wrap
differently, is matched by containment.

The location cell is read only as far as the closing bracket of its token list.
What follows is prose for the reader, and prose that named a boundary would be
the drift this file exists to close, so the token list is the only thing a
boundary is read from.

Exit codes:
    0  every rule holds
    1  at least one rule is broken
    2  nothing measured: `docs/internal/method/reference/gardes.md` is absent,
       which is the normal state of a fresh clone, of a worktree and of CI, the
       method reference being untracked. A tree that does not carry the table
       cannot be said to carry a wrong one.

Usage:
    python3 scripts/check_method_references.py
    python3 scripts/check_method_references.py --selftest
"""

import argparse
import contextlib
import io
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
TABLE = Path("docs/internal/method/reference/gardes.md")
JUSTFILE = Path("justfile")
PRECOMMIT = Path(".pre-commit-config.yaml")
WORKFLOWS = Path(".github/workflows")
VERDICTS = Path("scripts/worktree_verdicts.py")

MECHANICAL = ("just guards", "pré-commit", "CI", "verdicts")
DECLARATIVE = ("local", "local préparé")

# Two boundaries name a guard by something other than the command it runs, so
# the crossing cannot read them from a launching line.
#
# A pre-commit hook that comes from a third-party repository carries an `id:`
# and no `entry:`, so nothing in the config spells out what it runs. Each id is
# mapped here to the row it protects, and the id itself is checked against the
# config: a mapping that outlived its hook is a row promising a hook nobody
# installed.
THIRD_PARTY_HOOKS: dict[str, tuple[str, ...]] = {
    "ruff-check": ("lints Python", "lints des scripts"),
    "ruff-format": ("format Python",),
}

# A workflow sometimes launches the same tool under a different invocation than
# the one an operator types: `working-directory:` replaces a `cd`, and the
# hosted run widens the scope (`ruff check sdk agents scripts` against
# `ruff check scripts`). Reading the run line by containment would then answer
# "no CI" for a guard the CI does run, so each such row names the run line the
# workflow really carries, and that line is looked for in the workflows.
CI_VARIANTS: dict[str, str] = {
    "format Rust": "cargo fmt --all -- --check",
    "build frontend": "npm run build",
    "build documentation": "npm run build",
    "chaînes non routées": "npm run audit:i18n",
    "accessibilité": "npm run audit:a11y",
    "types Python": "mypy apollia",
    "tests Python": "pytest --cov=apollia",
    "format Python": "ruff format --check sdk agents",
    "lints Python": "ruff check sdk agents scripts",
    "lints des scripts": "ruff check sdk agents scripts",
}

# The heavy-guard table of `worktree_verdicts.py` names its lines by a key, not
# by a command: `ui-build` and `docs-build` both run `npm run build`. Each key
# is mapped to the row it measures, and both directions are checked, a key with
# no row and a row claiming `verdicts` without a key.
VERDICTS_ROWS: dict[str, str] = {
    "cargo-check": "compilation",
    "cargo-clippy": "lints Rust",
    "cargo-test": "tests Rust",
    "cli-e2e": "suite CLI, Track 1",
    "ui-build": "build frontend",
    "svelte-check": "types frontend",
    "vitest": "tests frontend",
    "docs-build": "build documentation",
    "desktop-automation": "corpus gestuel, verdict d'exécution",
    "linux-test": "compilation et tests Linux",
}

_YAML_LAUNCH = re.compile(r"^(\s*)(?:-\s+)?(?:run|entry):\s*(.*?)\s*$")


def normalise(line: str) -> str:
    """A launching line reduced to the subject it launches."""
    text = line.strip().strip("`").strip()
    text = re.sub(r"\s+", " ", text)
    for prefix in ("python3 ", "python ", "bash "):
        if text.startswith(prefix):
            text = text[len(prefix) :]
    return text.strip()


def yaml_launch_lines(text: str) -> list[str]:
    """Values of `run:` and `entry:` keys, block scalars included."""
    out: list[str] = []
    lines = text.splitlines()
    i = 0
    while i < len(lines):
        m = _YAML_LAUNCH.match(lines[i])
        if m is None:
            i += 1
            continue
        indent, value = len(m.group(1)), m.group(2)
        i += 1
        if value and not value.startswith(("|", ">")):
            out.append(value)
            continue
        while i < len(lines):
            body = lines[i]
            if body.strip() and (len(body) - len(body.lstrip())) <= indent:
                break
            out.append(body.strip())
            i += 1
    return out


def justfile_guards(root: Path) -> list[str]:
    """The subjects the `guards` recipe launches, in the order it names them."""
    text = (root / JUSTFILE).read_text(encoding="utf-8")
    subjects: list[str] = []
    for array in ("guards", "externals"):
        block = re.search(rf"{array}=\((.*?)\n    \)", text, re.S)
        if block is None:
            continue
        subjects += [normalise(v) for v in re.findall(r'"([^"]+)"', block.group(1))]
    return subjects


def justfile_recipes(root: Path) -> set[str]:
    text = (root / JUSTFILE).read_text(encoding="utf-8")
    return set(re.findall(r"^([a-z0-9][a-z0-9-]*)[^:\n]*:", text, re.M))


def launch_lines(root: Path) -> dict[str, list[str]]:
    """The launching lines of each mechanical boundary, already normalised."""
    lines: dict[str, list[str]] = {name: [] for name in MECHANICAL}
    lines["just guards"] = justfile_guards(root)

    config = root / PRECOMMIT
    if config.exists():
        raw = yaml_launch_lines(config.read_text(encoding="utf-8"))
        lines["pré-commit"] = [normalise(v) for v in raw]

    workflows = root / WORKFLOWS
    if workflows.is_dir():
        collected: list[str] = []
        for path in sorted(workflows.glob("*.yml")):
            collected += yaml_launch_lines(path.read_text(encoding="utf-8"))
        lines["CI"] = [normalise(v) for v in collected]

    return lines


def verdicts_keys(root: Path) -> list[str]:
    """The keys of the heavy-guard table of `worktree_verdicts.py`."""
    path = root / VERDICTS
    if not path.exists():
        return []
    body = path.read_text(encoding="utf-8")
    table = re.search(r"^GUARDS = \[(.*?)^\]", body, re.S | re.M)
    if table is None:
        return []
    return re.findall(r'Guard\(\s*"([^"]+)"', table.group(1))


def precommit_ids(root: Path) -> set[str]:
    path = root / PRECOMMIT
    if not path.exists():
        return set()
    return set(re.findall(r"^\s*-\s*id:\s*(\S+)", path.read_text(encoding="utf-8"), re.M))


def launched_by(subject: str, lines: list[str]) -> bool:
    """Whether a boundary carrying these lines launches this subject."""
    if subject.startswith("scripts/"):
        return subject in lines
    return any(subject in line for line in lines)


def parse_table(text: str) -> list[dict[str, str]]:
    """The rows of the guard table, one dict per row."""
    rows: list[dict[str, str]] = []
    header: list[str] | None = None
    for raw in text.splitlines():
        line = raw.strip()
        if not line.startswith("|"):
            header = None
            continue
        cells = [c.strip() for c in line.strip("|").split("|")]
        if header is None:
            if len(cells) >= 3 and cells[0] == "Garde" and cells[1] == "Commande":
                header = cells
            continue
        if set("".join(cells)) <= set("-: "):
            continue
        rows.append(dict(zip(header, cells)))
    return rows


def declared_tokens(cell: str) -> tuple[list[str], str | None]:
    """The bracketed token list opening a location cell, or a reason it is not."""
    m = re.match(r"^\[([^\]]+)\]", cell)
    if m is None:
        return [], "does not open with a bracketed token list, `[local + CI]`"
    return [t.strip().strip("`").strip() for t in m.group(1).split("+")], None


PATH_TOKEN = re.compile(r"`([^`]+)`")


def subject_defects(row: dict[str, str], root: Path, recipes: set[str]) -> list[str]:
    defects = []
    command = row.get("Commande", "")
    for token in PATH_TOKEN.findall(command):
        text = normalise(token)
        recipe = re.match(r"^just ([a-z0-9-]+)", text)
        if recipe is not None:
            if recipe.group(1) not in recipes:
                defects.append(f"names `just {recipe.group(1)}`, no such recipe")
            continue
        for word in text.split():
            if "/" in word and not word.startswith("-"):
                candidate = word.split(":")[0]
                if not (root / candidate).exists():
                    defects.append(f"names `{candidate}`, which the tree does not carry")
    return defects


def measure(
    root: Path,
    hooks: dict[str, tuple[str, ...]] | None = None,
    heavy_rows: dict[str, str] | None = None,
    ci_variants: dict[str, str] | None = None,
) -> tuple[list[str], int, int]:
    """Returns the defects, the number of rows read, the number of subjects.

    The two mappings are arguments so the self-test can drive them over its own
    fixture: a mapping pinned to this repository would make every fixture case
    fail for a reason foreign to the case.
    """
    hooks = THIRD_PARTY_HOOKS if hooks is None else hooks
    heavy_rows = VERDICTS_ROWS if heavy_rows is None else heavy_rows
    ci_variants = CI_VARIANTS if ci_variants is None else ci_variants
    table = root / TABLE
    if not table.exists():
        return ["NOTHING MEASURED"], 0, 0

    text = table.read_text(encoding="utf-8")
    rows = parse_table(text)
    lines = launch_lines(root)
    recipes = justfile_recipes(root)
    subjects = lines["just guards"]
    names = {row.get("Garde", "?") for row in rows}

    defects: list[str] = []

    hook_ids = precommit_ids(root)
    hooked_rows: set[str] = set()
    for hook, protected in hooks.items():
        if hook not in hook_ids:
            defects.append(
                f"boundary: `{hook}` is mapped to {list(protected)!r} and "
                f"{PRECOMMIT} carries no hook of that id"
            )
            continue
        for name in protected:
            if name not in names:
                defects.append(
                    f"boundary: hook `{hook}` is mapped to row `{name}`, which "
                    f"the table does not carry"
                )
        hooked_rows |= set(protected)

    ci_rows: set[str] = set()
    for name, line in ci_variants.items():
        if name not in names:
            defects.append(
                f"boundary: the workflow line `{line}` is mapped to row "
                f"`{name}`, which the table does not carry"
            )
            continue
        if not any(line in candidate for candidate in lines["CI"]):
            defects.append(
                f"boundary: row `{name}` is mapped to the workflow line "
                f"`{line}`, which no workflow carries any more"
            )
            continue
        ci_rows.add(name)

    heavy = verdicts_keys(root)
    verdict_rows: set[str] = set()
    for key in heavy:
        name = heavy_rows.get(key)
        if name is None:
            defects.append(
                f"boundary: `{VERDICTS}` carries the heavy guard `{key}` and no "
                f"row of the table is mapped to it"
            )
            continue
        if name not in names:
            defects.append(
                f"boundary: heavy guard `{key}` is mapped to row `{name}`, "
                f"which the table does not carry"
            )
            continue
        verdict_rows.add(name)
    for key in sorted(set(heavy_rows) - set(heavy)):
        defects.append(
            f"boundary: row `{heavy_rows[key]}` is mapped to the heavy guard "
            f"`{key}`, which `{VERDICTS}` no longer carries"
        )

    by_subject: dict[str, dict[str, str]] = {}
    for row in rows:
        command = normalise(re.sub(r"`", "", row.get("Commande", "")))
        by_subject[command] = row

    for subject in subjects:
        if subject.startswith("scripts/"):
            found = subject in by_subject
        else:
            found = any(subject in key for key in by_subject)
        if not found:
            defects.append(
                f"coverage: `{subject}` is launched by the `guards` recipe and has "
                f"no row in the table"
            )

    for row in rows:
        name = row.get("Garde", "?")
        for defect in subject_defects(row, root, recipes):
            defects.append(f"subject: row `{name}` {defect}")

        cell = row.get("Où, vérifié", row.get("Où", ""))
        tokens, why = declared_tokens(cell)
        if why is not None:
            defects.append(f"location: row `{name}` {why}")
            continue
        unknown = [t for t in tokens if t not in MECHANICAL + DECLARATIVE]
        if unknown:
            defects.append(
                f"location: row `{name}` declares unknown token(s) {unknown!r}; "
                f"the vocabulary is {list(MECHANICAL + DECLARATIVE)!r}"
            )
            continue
        subject = normalise(re.sub(r"`", "", row.get("Commande", "")))
        for token in MECHANICAL:
            claimed = token in tokens
            if token == "verdicts":
                real = name in verdict_rows
            elif token == "pré-commit":
                real = name in hooked_rows or launched_by(subject, lines[token])
            elif token == "CI":
                real = name in ci_rows or launched_by(subject, lines[token])
            else:
                real = launched_by(subject, lines[token])
            if claimed and not real:
                defects.append(
                    f"location: row `{name}` declares `{token}`, which launches "
                    f"nothing that answers to `{subject}`"
                )
            if real and not claimed:
                defects.append(
                    f"location: `{token}` launches `{subject}` and row `{name}` does not declare it"
                )

    return defects, len(rows), len(subjects)


def report(defects: list[str], rows: int, subjects: int) -> int:
    if defects == ["NOTHING MEASURED"]:
        print(
            f"NOTHING MEASURED: {TABLE} is absent from this tree, so no row was "
            f"read.\n                 The method reference is untracked by "
            f"design; a clone, a worktree\n                 and a hosted runner "
            f"all answer this way.",
            file=sys.stderr,
        )
        return 2
    print(f"method references: {rows} row(s) read, {subjects} subject(s) launched by `just guards`")
    if defects:
        for defect in defects:
            print(f"  {defect}")
        print(f"\n{len(defects)} defect(s): the table does not describe this tree", file=sys.stderr)
        return 1
    print("every launched guard has a row, every row names a subject that exists,")
    print("and every declared boundary is the boundary that launches it")
    return 0


# ── Self-test ────────────────────────────────────────────────────────────────

FIXTURE_JUSTFILE = """\
guards:
    #!/usr/bin/env bash
    guards=(
      "scripts/check_alpha.py"
      "scripts/check_beta.py --strict"
    )
    externals=(
      "cargo machete"
    )

linux-check target:
    echo {{target}}
"""

FIXTURE_PRECOMMIT = """\
repos:
  - repo: https://github.com/astral-sh/ruff-pre-commit
    hooks:
      - id: ruff-format
  - repo: local
    hooks:
      - id: alpha
        entry: python3 scripts/check_alpha.py
      - id: machete
        entry: bash -c 'cargo machete'
"""

FIXTURE_WORKFLOW = """\
jobs:
  guards:
    steps:
      - run: python3 scripts/check_beta.py --strict
"""

FIXTURE_VERDICTS = """\
GUARDS = [
    Guard(
        "cargo-machete",
        "cargo machete",
        ["cargo", "machete"],
    ),
]
"""

FIXTURE_TABLE = """\
# Les gardes

## La table

| Garde | Commande | Où, vérifié | Famille | Sert | Verdict | Temps |
|---|---|---|---|---|---|---|
| alpha | `python3 scripts/check_alpha.py` | [local + `just guards` + pré-commit] runs anywhere | base | `rust` | vert | 1 s |
| beta | `python3 scripts/check_beta.py --strict` | [local + `just guards` + CI + pré-commit] | lentille | `rust` | vert | 1 s |
| machete | `cargo machete` | [local + `just guards` + pré-commit + verdicts] | lentille | `rust` | vert | 1 s |
| recette | `just linux-check arm` | [local] | base | `rust` | vert, sous Docker | 2 s |
"""


def write_fixture(root: Path, table: str) -> None:
    (root / "scripts").mkdir(parents=True, exist_ok=True)
    (root / ".github/workflows").mkdir(parents=True, exist_ok=True)
    (root / "docs/internal/method/reference").mkdir(parents=True, exist_ok=True)
    (root / "scripts/check_alpha.py").write_text("", encoding="utf-8")
    (root / "scripts/check_beta.py").write_text("", encoding="utf-8")
    (root / "justfile").write_text(FIXTURE_JUSTFILE, encoding="utf-8")
    (root / PRECOMMIT).write_text(FIXTURE_PRECOMMIT, encoding="utf-8")
    (root / ".github/workflows/ci.yml").write_text(FIXTURE_WORKFLOW, encoding="utf-8")
    (root / VERDICTS).write_text(FIXTURE_VERDICTS, encoding="utf-8")
    (root / TABLE).write_text(table, encoding="utf-8")


def selftest() -> int:
    import tempfile

    failures: list[str] = []

    def case(name: str, condition: bool, detail: str) -> None:
        if condition:
            print(f"  ok    {name}")
        else:
            print(f"  FAIL  {name}")
            failures.append(f"{name}: {detail}")

    hooks = {"ruff-format": ("beta",)}
    heavy_rows = {"cargo-machete": "machete"}

    ci_variants: dict[str, str] = {}

    def verdict(
        table: str | None,
        hook_map: dict[str, tuple[str, ...]] | None = None,
        heavy_map: dict[str, str] | None = None,
        ci_map: dict[str, str] | None = None,
    ) -> tuple[int, list[str]]:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_fixture(root, table if table is not None else FIXTURE_TABLE)
            if table is None:
                (root / TABLE).unlink()
            defects, rows, subjects = measure(
                root,
                hooks if hook_map is None else hook_map,
                heavy_rows if heavy_map is None else heavy_map,
                ci_variants if ci_map is None else ci_map,
            )
            # The verdict comes from `report`, never from a second reading of
            # the defect list: a self-test that recomputed the exit code would
            # leave the mapping from defects to codes untested.
            sink = io.StringIO()
            with contextlib.redirect_stdout(sink), contextlib.redirect_stderr(sink):
                code = report(defects, rows, subjects)
            return code, defects

    code, _ = verdict(FIXTURE_TABLE)
    case(
        "negative control: a table that describes its fixture passes",
        code == 0,
        "the complete fixture was reported as defective, so every red below "
        "would be the checker matching nothing rather than a defect",
    )

    dropped = "\n".join(
        line for line in FIXTURE_TABLE.splitlines() if not line.startswith("| beta ")
    )
    code, defects = verdict(dropped)
    case(
        "a guard the recipe launches and the table ignores is reported",
        code == 1 and any("coverage:" in d for d in defects),
        f"removing the row of a launched guard answered {code} with {defects!r}, "
        f"which is the drift this file exists to close",
    )

    lying = FIXTURE_TABLE.replace(
        "| recette | `just linux-check arm` | [local]",
        "| recette | `just linux-check arm` | [local + CI]",
    )
    code, defects = verdict(lying)
    case(
        "a row that declares a boundary no file carries is reported",
        code == 1 and any("declares `CI`" in d for d in defects),
        f"a declared workflow that launches nothing answered {code} with "
        f"{defects!r}: a promised protection nobody has",
    )

    silent = FIXTURE_TABLE.replace(
        "| alpha | `python3 scripts/check_alpha.py` | [local + `just guards` + pré-commit]",
        "| alpha | `python3 scripts/check_alpha.py` | [local + `just guards`]",
    )
    code, defects = verdict(silent)
    case(
        "a boundary that launches a guard the row omits is reported",
        code == 1 and any("does not declare it" in d for d in defects),
        f"an undeclared hook answered {code} with {defects!r}; the location "
        f"column has to be falsifiable in both directions or it is a wish",
    )

    gone = FIXTURE_TABLE.replace("scripts/check_alpha.py", "scripts/check_gone.py")
    code, defects = verdict(gone)
    case(
        "a row naming a file the tree does not carry is reported",
        code == 1 and any("subject:" in d for d in defects),
        f"a row pointing at an absent script answered {code} with {defects!r}",
    )

    norecipe = FIXTURE_TABLE.replace("just linux-check arm", "just nowhere-check arm")
    code, defects = verdict(norecipe)
    case(
        "a row naming a recipe the justfile does not carry is reported",
        code == 1 and any("no such recipe" in d for d in defects),
        f"a row pointing at an absent recipe answered {code} with {defects!r}",
    )

    bare = FIXTURE_TABLE.replace(
        "[local + `just guards` + CI + pré-commit]", "local et parfois la CI"
    )
    code, defects = verdict(bare)
    case(
        "a location cell without its token list is reported",
        code == 1 and any("bracketed token list" in d for d in defects),
        f"prose in place of the token list answered {code} with {defects!r}; a "
        f"cell nothing can parse is a cell nothing can check",
    )

    typo = FIXTURE_TABLE.replace("[local + `just guards` + CI + pré-commit]", "[local + precommit]")
    code, defects = verdict(typo)
    case(
        "a location token outside the vocabulary is reported",
        code == 1 and any("unknown token" in d for d in defects),
        f"a misspelt token answered {code} with {defects!r}, and a token no "
        f"boundary is read from excuses the row from every check",
    )

    code, defects = verdict(FIXTURE_TABLE, hook_map={"ruff-gone": ("beta",)})
    case(
        "a third-party hook the config no longer carries is reported",
        code == 1 and any("carries no hook of that id" in d for d in defects),
        f"a mapping onto a removed hook answered {code} with {defects!r}; a row "
        f"that promises a hook nobody installed is the drift one step further",
    )

    code, defects = verdict(FIXTURE_TABLE, heavy_map={})
    case(
        "a heavy guard mapped to no row is reported",
        code == 1 and any("no row of the table is mapped to it" in d for d in defects),
        f"a heavy guard absent from the mapping answered {code} with {defects!r}",
    )

    code, defects = verdict(FIXTURE_TABLE, heavy_map={"cargo-gone": "machete"})
    case(
        "a row mapped to a heavy guard that left the table is reported",
        code == 1 and any("no longer carries" in d for d in defects),
        f"a stale heavy-guard mapping answered {code} with {defects!r}",
    )

    code, defects = verdict(FIXTURE_TABLE, ci_map={"machete": "cargo nowhere"})
    case(
        "a workflow line the workflows no longer carry is reported",
        code == 1 and any("no workflow carries any more" in d for d in defects),
        f"a mapping onto a removed workflow line answered {code} with "
        f"{defects!r}; a row that promises a hosted run nobody performs is the "
        f"same drift a third time",
    )

    code, _ = verdict(None)
    case(
        "an absent table is nothing measured, not a pass",
        code == 2,
        "a tree without the method reference reported a pass, so a clone, a "
        "worktree and a hosted runner would all certify a table they never read",
    )

    if failures:
        print(f"\n{len(failures)} self-test failure(s):\n", file=sys.stderr)
        for line in failures:
            print(f"  {line}\n", file=sys.stderr)
        return 1
    print(
        "\nall cases pass: a complete table passes, a missing row, a declared "
        "boundary that launches nothing, a boundary the row omits, an absent "
        "script, an absent recipe, an unparsable cell and an unknown token each "
        "fail, and an absent table is reported as nothing measured"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--selftest", action="store_true", help="run the self-test")
    parser.add_argument(
        "--root", default=str(REPO_ROOT), help="tree to read (default: this repository)"
    )
    args = parser.parse_args()

    if args.selftest:
        return selftest()

    defects, rows, subjects = measure(Path(args.root))
    return report(defects, rows, subjects)


if __name__ == "__main__":
    sys.exit(main())
