#!/usr/bin/env python3
"""The configuration reference page lists exactly the keys of the registry.

`apollia_core::config::keys::KEYS` is the one registry of the keys of
`apollia.toml`: the default file written on a first start is generated from it,
`apollia-os config keys` prints it, and a Rust test holds it to the structures.
The public reference page, `docs/site/docs/reference/configuration.md`, and its
French mirror carry a table per section between two markers. Before this guard
the page was derived from the structures, top level only, and a key nested one
table down (`tools.web_search.brave.timeout_secs`) was on no page at all.

The guard reads the registry from its source, one entry per line, and refuses
to judge when an entry line escapes the parser: a count of `key(` and
`withdrawn(` calls that differs from the entries read is a parse defect, not a
green. It then reads, in each page, every table row between the markers whose
first cell is a backticked path, and compares:

  - a registry key absent from the page is a red;
  - a page row naming no registry key is a red;
  - a default that differs from the registry is a red (a backticked default
    must equal the registry literal; an unset key is written without one).

`--selftest` replays the check on modified copies of the two pages, in a
temporary folder: a row removed, a row added, a default changed must each turn
it red, and the unmodified copies must stay green.

Usage:
    python3 scripts/check_config_reference.py [--selftest]

Exit codes: 0 every page matches the registry, 1 a mismatch, 2 nothing
measured (the registry or a page is absent, or the registry could not be read).
"""

import argparse
import re
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
REGISTRY = REPO_ROOT / "crates" / "apollia-core" / "src" / "config" / "keys.rs"
PAGES = [
    REPO_ROOT / "docs" / "site" / "docs" / "reference" / "configuration.md",
    REPO_ROOT / "docs" / "site" / "i18n" / "fr" / "docusaurus-plugin-content-docs"
    / "current" / "reference" / "configuration.md",
]
BEGIN = "<!-- BEGIN GENERATED: config-fields -->"
END = "<!-- END GENERATED: config-fields -->"


@dataclass(frozen=True)
class Key:
    path: str
    ty: str
    default: str | None
    reload: str
    doc: str
    lockable: bool
    withdrawn: str | None


def _string_literal(token: str) -> str:
    token = token.strip()
    if not (token.startswith('"') and token.endswith('"')):
        raise ValueError(f"not a string literal: {token}")
    body = token[1:-1]
    return re.sub(r'\\(["\\])', r"\1", body)


def _split_args(text: str) -> list[str]:
    """Top-level comma split, strings and brackets respected."""
    args, depth, current, in_string, escaped = [], 0, [], False, False
    for ch in text:
        if in_string:
            current.append(ch)
            if escaped:
                escaped = False
            elif ch == "\\":
                escaped = True
            elif ch == '"':
                in_string = False
            continue
        if ch == '"':
            in_string = True
        elif ch in "([":
            depth += 1
        elif ch in ")]":
            depth -= 1
        elif ch == "," and depth == 0:
            args.append("".join(current).strip())
            current = []
            continue
        current.append(ch)
    if "".join(current).strip():
        args.append("".join(current).strip())
    return args


def _type_name(token: str) -> str:
    token = token.removeprefix("KeyType::")
    enum = re.fullmatch(r'Enum\(&\[(.*)\]\)', token)
    if enum:
        values = [_string_literal(v) for v in _split_args(enum.group(1))]
        return "one of " + ", ".join(values)
    names = {
        "Boolean": "boolean",
        "Integer": "integer",
        "Float": "float",
        "String": "string",
        "Path": "path",
        "StringArray": "string array",
        "TableArray": "table array",
        "Table": "table",
    }
    if token not in names:
        raise ValueError(f"unknown key type: {token}")
    return names[token]


def read_registry(path: Path = REGISTRY) -> list[Key]:
    """Every entry of `KEYS`, in order. Raises ValueError on a line it cannot read."""
    source = path.read_text(encoding="utf-8")
    start = source.find("pub const KEYS: &[ConfigKey] = &[")
    if start == -1:
        raise ValueError(f"`pub const KEYS` is not in {path}")
    end = source.find("\n];", start)
    block = source[start:end]
    keys: list[Key] = []
    for raw in block.splitlines()[1:]:
        line = raw.strip()
        if not line or line.startswith("//"):
            continue
        lockable = line.startswith("locked(")
        if lockable:
            line = line.removeprefix("locked(").removesuffix("),") + ","
        if line.startswith("key(") and line.endswith("),"):
            args = _split_args(line[len("key("):-2])
            if len(args) != 5:
                raise ValueError(f"a key entry with {len(args)} arguments: {raw.strip()}")
            path_, ty, default, reload, doc = args
            keys.append(Key(
                path=_string_literal(path_),
                ty=_type_name(ty),
                default=None if default == "None" else _string_literal(
                    default.removeprefix("Some(").removesuffix(")")),
                reload={"Reloadable": "reloadable", "Restart": "restart"}[reload],
                doc=_string_literal(doc),
                lockable=lockable,
                withdrawn=None,
            ))
        elif line.startswith("withdrawn(") and line.endswith("),"):
            path_, detail = _split_args(line[len("withdrawn("):-2])
            keys.append(Key(_string_literal(path_), "table", None, "reloadable",
                            "", False, _string_literal(detail)))
        else:
            raise ValueError(f"an entry line the parser cannot read: {raw.strip()}")
    calls = len(re.findall(r"^\s*(?:locked\()?(?:key|withdrawn)\(", block, re.MULTILINE))
    if calls != len(keys):
        raise ValueError(f"{calls} entries written, {len(keys)} read")
    return keys


ROW = re.compile(r"^\|\s*`([a-z0-9_.]+)`\s*\|(.*)\|\s*$")


def page_rows(text: str) -> list[tuple[str, list[str]]] | None:
    """`(path, other cells)` of every keyed row between the markers."""
    if BEGIN not in text or END not in text:
        return None
    block = text.split(BEGIN, 1)[1].split(END, 1)[0]
    rows = []
    for line in block.splitlines():
        m = ROW.match(line.strip())
        if m:
            rows.append((m.group(1), [c.strip() for c in m.group(2).split("|")]))
    return rows


def compare(keys: list[Key], text: str, label: str) -> list[str]:
    rows = page_rows(text)
    if rows is None:
        return [f"{label}: the markers of the generated block are missing"]
    findings = []
    by_path = {k.path: k for k in keys}
    seen: dict[str, int] = {}
    for path, cells in rows:
        seen[path] = seen.get(path, 0) + 1
        key = by_path.get(path)
        if key is None:
            findings.append(f"{label}: `{path}` is on the page but not in the registry")
            continue
        if key.withdrawn is not None:
            continue
        default_cell = cells[1] if len(cells) > 1 else ""
        written = re.fullmatch(r"`(.*)`", default_cell)
        if key.default is None and written:
            findings.append(f"{label}: `{path}` shows a default the registry does not have")
        elif key.default is not None and (not written or written.group(1) != key.default):
            findings.append(
                f"{label}: `{path}` default is {default_cell!r}, registry `{key.default}`")
    for path in by_path:
        if path not in seen:
            findings.append(f"{label}: `{path}` is in the registry but not on the page")
    for path, count in seen.items():
        if count > 1:
            findings.append(f"{label}: `{path}` appears {count} times")
    return findings


def check(registry: Path, pages: list[Path]) -> int:
    if not registry.is_file():
        print(f"NOTHING MEASURED: {registry} is absent")
        return 2
    try:
        keys = read_registry(registry)
    except ValueError as exc:
        print(f"NOTHING MEASURED: the registry could not be read: {exc}")
        return 2
    findings = []
    for page in pages:
        if not page.is_file():
            print(f"NOTHING MEASURED: {page} is absent")
            return 2
        label = str(page.relative_to(REPO_ROOT)) if page.is_relative_to(REPO_ROOT) else str(page)
        findings += compare(keys, page.read_text(encoding="utf-8"), label)
    if findings:
        print(f"check_config_reference: RED, {len(findings)} finding(s)")
        for f in findings:
            print(f"  {f}")
        return 1
    print(f"check_config_reference: GREEN, {len(keys)} registry entries on {len(pages)} pages")
    return 0


def selftest() -> int:
    keys = read_registry(REGISTRY)
    live = next(k for k in keys if k.withdrawn is None and k.default is not None)
    failures = []
    with tempfile.TemporaryDirectory() as tmp:
        for page in PAGES:
            text = page.read_text(encoding="utf-8")
            row = next(line for line in text.splitlines()
                       if line.startswith(f"| `{live.path}` |"))
            cases = {
                "unmodified": (text, True),
                "a row removed": (text.replace(row + "\n", "", 1), False),
                "a row added": (text.replace(row, row + "\n| `api.not_a_key` | integer | `1` | x | x |", 1), False),
                "a default changed": (text.replace(
                    row, row.replace(f"`{live.default}`", "`changed`", 1), 1), False),
            }
            for name, (body, green) in cases.items():
                copy = Path(tmp) / f"{name.replace(' ', '_')}.md"
                copy.write_text(body, encoding="utf-8")
                found = compare(keys, copy.read_text(encoding="utf-8"), copy.name)
                if bool(found) == green:
                    failures.append(f"{page.name} {name}: expected "
                                    f"{'green' if green else 'red'}, got {found or 'green'}")
    if failures:
        print("check_config_reference --selftest: RED")
        for f in failures:
            print(f"  {f}")
        return 1
    print(f"check_config_reference --selftest: GREEN, 4 cases on {len(PAGES)} pages")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args()
    if args.selftest:
        return selftest()
    return check(REGISTRY, PAGES)


if __name__ == "__main__":
    sys.exit(main())
