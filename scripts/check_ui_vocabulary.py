#!/usr/bin/env python3
"""Fail when a text of the desktop interface carries a banned technical term.

The interface vocabulary is data: `crates/apollia-desktop/ui/src/lib/i18n/
glossary.json` names, per locale, the terms that stay out of the interface
while the developer mode is off (`banned`, plus the keys of `banned_synonyms`).
This guard holds the two catalogues, `en.json` and `fr.json`, to that list.

The rule, refutable: for every leaf key of a catalogue outside the `dev`
namespace, the value contains no banned term of its locale, as a whole word.
Case is ignored, except for a term written with a capital letter in the
glossary (an acronym such as `MCP`, `GGUF`, `ReAct`), which only matches as
written; so `react to an event` is prose and `ReAct` is the technique. A plural
in `s` is the same word. ICU placeholders (`{name}`) are variable names, not
displayed text, and are not read. A faulty value exits 1 with the key and the
term.

Exemptions, three kinds:

  dev namespace   keys under `dev.` are the texts of the developer mode; they
                  may name anything, and the call side below keeps them there.
  exempt blocks   the namespaces of the screens mounted only in developer
                  mode (`memory`, `observability`, `transcriptions`) are exempt as a
                  block, on one condition the guard checks: their route sits in
                  the developer group of the sidebar (`DEVELOPER_NAV` in
                  `src/lib/components/app/Sidebar.svelte`). A route taken out of
                  the group fails, since its texts would then reach every user.
  named keys      the `exemptions` list of each locale of the glossary: the
                  debt measured when the guard arrived. It is a ratchet that
                  only descends: an exempted key whose value no longer carries
                  a banned term fails too, until it leaves the list, and so
                  does an exempted key the catalogue no longer has.

The call side. A `$t("dev.…")` (or `get(t)("dev.…")`) in a file that never
reads `developerMode` fails. This is an approximation at the file level: a file
that reads the mode once may still render a `dev.` text outside the branch that
tests it, and the guard cannot tell. It only proves the file knows the mode
exists.

Out of scope, said here so nobody reads the green as more than it is: the texts
rendered by Rust or Python (the guide's tables, error messages built by the
runtime or the SDK) never pass through these catalogues, and this guard does
not see them.

Exit codes: 0 clean, 1 at least one finding, 2 nothing measured (a catalogue
or the glossary missing, or no leaf key read).

Usage:
    python3 scripts/check_ui_vocabulary.py
"""

import argparse
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
UI_SRC = REPO_ROOT / "crates/apollia-desktop/ui/src"
I18N_DIR = UI_SRC / "lib/i18n"
GLOSSARY = I18N_DIR / "glossary.json"
SIDEBAR = UI_SRC / "lib/components/app/Sidebar.svelte"
LOCALES = ("en", "fr")

DEV_NAMESPACE = "dev"
# Namespace of a screen mounted only in developer mode -> its sidebar route.
EXEMPT_BLOCKS = {
    "memory": "memory",
    "observability": "observability",
    "transcriptions": "transcriptions",
}

PLACEHOLDER = re.compile(r"\{[^{}]*\}")
DEV_CALL = re.compile(r"(?:\$t|\$_|get\(t\))\(\s*[\"'`]dev\.")
DEVELOPER_NAV = re.compile(
    r"DEVELOPER_NAV\b[^=]*=\s*\[(?P<body>.*?)\];", re.DOTALL
)
NAV_ROUTE = re.compile(r"route:\s*[\"'](?P<route>[^\"']+)[\"']")


def leaf_values(catalogue: dict, prefix: str = "") -> dict[str, str]:
    """Every leaf of a nested catalogue, keyed by its dotted path."""
    leaves: dict[str, str] = {}
    for key, value in catalogue.items():
        path = f"{prefix}{key}"
        if isinstance(value, dict):
            leaves.update(leaf_values(value, f"{path}."))
        elif isinstance(value, str):
            leaves[path] = value
    return leaves


def banned_terms(locale: dict) -> list[str]:
    """The banned terms of one locale of the glossary, synonyms included."""
    return list(locale.get("banned", [])) + list(locale.get("banned_synonyms", {}))


def _term_pattern(term: str) -> re.Pattern[str]:
    flags = 0 if any(c.isupper() for c in term) else re.IGNORECASE
    return re.compile(rf"(?<!\w){re.escape(term)}s?(?!\w)", flags)


def terms_in(value: str, terms: list[str]) -> list[str]:
    """The banned terms a displayed value carries, in glossary order."""
    text = PLACEHOLDER.sub(" ", value)
    return [term for term in terms if _term_pattern(term).search(text)]


def namespace(key: str) -> str:
    return key.split(".", 1)[0]


def developer_routes(sidebar_text: str) -> set[str]:
    """The routes listed in the developer group of the sidebar."""
    match = DEVELOPER_NAV.search(sidebar_text)
    if match is None:
        return set()
    return {m.group("route") for m in NAV_ROUTE.finditer(match.group("body"))}


def catalogue_findings(
    lang: str, leaves: dict[str, str], terms: list[str], exemptions: list[str]
) -> list[str]:
    """Faulty values and stale exemptions of one locale."""
    findings: list[str] = []
    exempt = set(exemptions)
    for key in sorted(leaves):
        ns = namespace(key)
        if ns == DEV_NAMESPACE or ns in EXEMPT_BLOCKS:
            continue
        found = terms_in(leaves[key], terms)
        if found and key not in exempt:
            findings.append(f"{lang}.json {key}: banned term {', '.join(found)}")
    for key in sorted(exempt):
        if key not in leaves:
            findings.append(
                f"glossary.json {lang}.exemptions: {key} is not a key of "
                f"{lang}.json any more; remove it from the list"
            )
        elif namespace(key) == DEV_NAMESPACE or namespace(key) in EXEMPT_BLOCKS:
            findings.append(
                f"glossary.json {lang}.exemptions: {key} is already exempt by "
                f"its namespace; remove it from the list"
            )
        elif not terms_in(leaves[key], terms):
            findings.append(
                f"glossary.json {lang}.exemptions: {key} carries no banned term "
                f"any more; the ratchet only descends, remove it from the list"
            )
    return findings


def block_findings(routes: set[str]) -> list[str]:
    """Exempt blocks whose route left the developer group of the sidebar."""
    return [
        f"Sidebar.svelte: route {route} of the exempt namespace {ns} is not in "
        f"the developer group (DEVELOPER_NAV); its texts would reach every user"
        for ns, route in sorted(EXEMPT_BLOCKS.items())
        if route not in routes
    ]


def call_findings(files: dict[str, str]) -> list[str]:
    """Files that render a `dev.` text without reading `developerMode`."""
    return [
        f"{rel}: renders a dev. text but never reads developerMode"
        for rel, text in sorted(files.items())
        if DEV_CALL.search(text) and "developerMode" not in text
    ]


def _source_files() -> dict[str, str]:
    files: dict[str, str] = {}
    for path in sorted(UI_SRC.rglob("*")):
        if path.suffix not in (".svelte", ".ts") or not path.is_file():
            continue
        if path.name.endswith((".test.ts", ".spec.ts")):
            continue
        files[str(path.relative_to(REPO_ROOT))] = path.read_text(encoding="utf-8")
    return files


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__.split("\n\n", 1)[0],
        epilog=__doc__.split("\n\n", 1)[1],
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.parse_args()

    inputs = [GLOSSARY, SIDEBAR, *(I18N_DIR / f"{lang}.json" for lang in LOCALES)]
    missing = [path for path in inputs if not path.is_file()]
    if missing:
        for path in missing:
            print(f"nothing measured: {path.relative_to(REPO_ROOT)} is missing", file=sys.stderr)
        return 2

    glossary = json.loads(GLOSSARY.read_text(encoding="utf-8"))
    findings: list[str] = []
    counts: list[str] = []
    for lang in LOCALES:
        leaves = leaf_values(json.loads((I18N_DIR / f"{lang}.json").read_text(encoding="utf-8")))
        if not leaves:
            print(f"nothing measured: {lang}.json has no leaf key", file=sys.stderr)
            return 2
        locale = glossary.get(lang, {})
        terms = banned_terms(locale)
        if not terms:
            print(f"nothing measured: glossary.json {lang} bans no term", file=sys.stderr)
            return 2
        exemptions = list(locale.get("exemptions", []))
        findings += catalogue_findings(lang, leaves, terms, exemptions)
        counts.append(f"{lang}: {len(leaves)} keys, {len(terms)} terms, {len(exemptions)} exempted")

    routes = developer_routes(SIDEBAR.read_text(encoding="utf-8"))
    findings += block_findings(routes)
    files = _source_files()
    findings += call_findings(files)

    summary = f"{'; '.join(counts)}; {len(files)} source files read for dev. calls"
    if findings:
        for finding in findings:
            print(finding, file=sys.stderr)
        print(f"\n{len(findings)} finding(s). {summary}", file=sys.stderr)
        return 1
    print(f"ui vocabulary: clean. {summary}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
