#!/usr/bin/env python3
"""Fail when a French sidebar label is still the English one, or has no source.

Docusaurus reads every `_category_.json` from `docs/site/docs`, and only from
there, whatever locale it is building: `plugin-content-docs/lib/sidebars/index.js`
calls `readCategoriesMetadata(version.contentPath)`, and `contentPath` is
`siteDir/docs` in both locales. The French label of a category can therefore
come from exactly one place, the catalogue
`i18n/fr/docusaurus-plugin-content-docs/current.json`, and a category with no
entry there renders its English label in the French sidebar with nothing said
about it.

Twelve categories of the operator help were in that state, so a reader browsing
the French site saw `Cross-cutting` and `Troubleshooting` in a sidebar whose
every page title was French.

The expected key set is derived, never hard-coded: it comes from the labels the
`_category_.json` files declare, and it is crossed both ways, so a category
renamed on the English side is reported on the day of the rename instead of
silently falling back to English while its translation sits in the catalogue
under a key nothing reads.

Two categories break the naive "one key per `_category_.json`" derivation, both
measured against `docusaurus write-translations`, not guessed:

  - `docs/reference/cli` carries a category file and produces no key. Its
    directory holds only `index.md`, so the auto-generated sidebar makes it a
    document entry, and document entries carry no label translation key. The
    exception holds only while the directory has that one page; a second page
    turns it into a category and the guard asks for the key again.
  - `api` produces a key with no category file of its own. That category is
    emitted by the OpenAPI plugin, `categoryLinkSource: 'tag'`, inside a
    directory git ignores.

The exemption for labels both languages spell the same is named one by one, and
driven from both sides. An entry in it whose message differs from its label is
reported too: somebody wrote a real translation, so the exemption line has
become a lie and must go. That is what keeps this list from turning into a
silencer, and it satisfies property 5 of `scripts/check_selftest.py`, "a rule
carrying a named exemption reports when the exemption grows", by naming every
exempted label on every run.

What this does not catch, and it is deliberate: a message that differs from its
label while still being English, say `Operator help` translated as
`Operator help center`. No rule here can see it without a language detector,
and this file legitimately contains English words. The defect this guard exists
for is a translation never written, which is the shape it does catch.

The sidebar catalogue is not the only one. Twelve theme strings sat in English
or unaccented French under the French locale while this guard was green,
because it read `current.json` and nothing else. Three more subjects are
therefore judged, each with the same both-ways exemption discipline:

  - `code.json`, the theme's own strings. A translation never written is a
    message equal to the English default, and the defaults are read from
    `@docusaurus/theme-translations/locales/base` rather than hard-coded, so
    a theme upgrade moves the reference with it. That directory arrives with
    `npm ci` under `docs/site`, and its absence is zero coverage, never a
    pass: this guard's CI boundary is the `docs-build` job, after the
    install. `theme.blog.*` keys are skipped and counted, `blog: false` in
    `docusaurus.config.js`.
  - `docusaurus-theme-classic/navbar.json` and `footer.json`. Their keys
    embed the English source (`item.label.Reference`), so a message equal to
    its own key suffix is a translation never written, the same rule the
    sidebar catalogue already had.
  - unaccented French. `Signaler un probleme` and `Francais` are not English,
    not the default, and not French either. A short named list of unaccented
    spellings that are never correct French is matched as whole words against
    every French message and the `fr` locale label of `docusaurus.config.js`.

Exit codes:
    0  every catalogue was read, every expected label carries a French message
    1  at least one label is untranslated, orphaned, missing, or wrongly exempt
    2  nothing was measured: no catalogue, no category file, no label key, or
       the theme's base defaults are absent (run `npm ci` under docs/site)

Usage:
    python3 scripts/check_i18n_catalogue.py
    python3 scripts/check_i18n_catalogue.py --selftest
"""

import argparse
import contextlib
import io
import json
import re
import shutil
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

CATEGORY_ROOT = Path("docs/site/docs")
CATALOGUE = Path("docs/site/i18n/fr/docusaurus-plugin-content-docs/current.json")

LABEL_PREFIX = "sidebar.docs.category."

# Labels the two languages spell identically. Written by hand, one line per
# label, because it is a judgement about language and not a measurement. Six of
# the twenty labels, and the count is printed on every run so that a seventh
# cannot slip in unnoticed.
IDENTICAL_IN_BOTH_LANGUAGES = (
    "Agents",
    "Architecture",
    "Chat",
    "Installation",
    "Notifications",
    "api",
)

# Categories the file walk cannot get right, both measured against
# `docusaurus write-translations` rather than assumed. See the module docstring.
NO_KEY_DESPITE_A_FILE = {"CLI reference"}
KEY_WITHOUT_A_FILE = ("api",)

# Keys that are not category labels and are deliberately left alone. Named here
# rather than dropped by a silent filter: `version.label` holds `Next`, the
# Docusaurus default, and the site publishes no version selector.
NOT_A_LABEL = ("version.label",)

# ── The theme catalogues ─────────────────────────────────────────────────────

CODE_CATALOGUE = Path("docs/site/i18n/fr/code.json")
THEME_CLASSIC_DIR = Path("docs/site/i18n/fr/docusaurus-theme-classic")
BASE_LOCALE_DIR = Path("docs/site/node_modules/@docusaurus/theme-translations/locales/base")
SITE_CONFIG = Path("docs/site/docusaurus.config.js")

# `code.json` keys whose French message legitimately equals the English
# default. Written by hand, one line per key, because it is a judgement about
# language and not a measurement; driven from both sides like every exemption
# here, so an entry that gains a real translation is reported as stale.
CODE_IDENTICAL = (
    "theme.admonition.danger",
    "theme.admonition.info",
    "theme.navbar.mobileVersionsDropdown.label",
    "theme.tags.tagsPageTitle",
)

# Navbar and footer labels both languages spell identically.
CLASSIC_IDENTICAL = (
    # The footer column that groups the links back to the product. A brand name
    # is not translated, and calling the column anything else in French would
    # name something the reader cannot find.
    "Apollia",
    "Architecture",
    "Discussions",
    "GitHub",
)

# Unaccented spellings that are never correct French, mapped to the word that
# was meant. Whole-word matches only, so `problème` spelled correctly never
# fires, and the list stays short enough to re-read.
UNACCENTED = {
    "evenement": "événement",
    "evenements": "événements",
    "francais": "français",
    "francaise": "française",
    "probleme": "problème",
    "problemes": "problèmes",
    "reference": "référence",
    "references": "références",
    "systeme": "système",
    "systemes": "systèmes",
}

LABEL_KEY = re.compile(r"(?:^|\.)(?:label|title)\.(.+)$")
FR_LOCALE_LABEL = re.compile(r"fr:\s*\{\s*label:\s*'([^']*)'")
WORD = re.compile(r"[A-Za-zàâçéèêëîïôûùüÿœÀÂÇÉÈÊËÎÏÔÛÙÜŸŒ]+")


def base_defaults(base_dir: Path) -> dict[str, str]:
    """Return the theme's English default messages, keyed like `code.json`."""
    defaults: dict[str, str] = {}
    if not base_dir.is_dir():
        return defaults
    for path in sorted(base_dir.glob("*.json")):
        data = json.loads(path.read_text(encoding="utf-8"))
        for key, value in data.items():
            if isinstance(value, dict) and isinstance(value.get("message"), str):
                defaults[key] = value["message"]
            elif isinstance(value, str):
                defaults[key] = value
    return defaults


def code_catalogue_faults(
    entries: dict[str, str],
    defaults: dict[str, str],
    exemption: tuple[str, ...] = CODE_IDENTICAL,
) -> tuple[list[tuple[str, str]], int]:
    """Judge `code.json` against the English defaults.

    Returns the faults and the count of `theme.blog.*` keys skipped, so the
    caller can print the skip instead of letting it happen silently.
    """
    faults: list[tuple[str, str]] = []
    exempt = set(exemption)
    skipped = 0
    for key in sorted(entries):
        if key.startswith("theme.blog."):
            skipped += 1
            continue
        message = entries[key]
        default = defaults.get(key)
        if default is None:
            continue
        if message == default and key not in exempt:
            faults.append(
                (key, "message equals the English default, never translated")
            )
        elif message != default and key in exempt:
            faults.append(
                (
                    key,
                    "exempted as identical in both languages, yet a real "
                    "translation was written, so the exemption line is now false",
                )
            )
    return faults, skipped


def classic_catalogue_faults(
    entries: dict[str, str],
    exemption: tuple[str, ...] = CLASSIC_IDENTICAL,
) -> list[tuple[str, str]]:
    """Judge a navbar or footer catalogue by its own key suffixes."""
    faults: list[tuple[str, str]] = []
    exempt = set(exemption)
    for key in sorted(entries):
        found = LABEL_KEY.search(key)
        if found is None:
            continue
        source = found.group(1)
        message = entries[key]
        if message == source and source not in exempt:
            faults.append((key, "message equals the key's English label, never translated"))
        elif message != source and source in exempt:
            faults.append(
                (
                    key,
                    "exempted as identical in both languages, yet a real "
                    "translation was written, so the exemption line is now false",
                )
            )
    return faults


def unaccented_faults(
    messages: dict[str, str],
    unaccented: dict[str, str] = None,
) -> list[tuple[str, str]]:
    """Report every message carrying a whole word from the unaccented list."""
    table = UNACCENTED if unaccented is None else unaccented
    faults: list[tuple[str, str]] = []
    for key in sorted(messages):
        for word in WORD.findall(messages[key]):
            meant = table.get(word.lower())
            if meant is not None:
                faults.append(
                    (key, f"unaccented French: {word!r} where {meant!r} was meant")
                )
    return faults


def declared_labels(root: Path) -> dict[str, Path]:
    """Return every category label declared under `root`, with its file.

    Pure with respect to the tree it is given, so a caller can drive it on a
    fixture as well as on the repository, which is the only way to know the
    derivation fires at all.
    """
    labels: dict[str, Path] = {}
    if not root.is_dir():
        return labels
    for path in sorted(root.rglob("_category_.json")):
        if "node_modules" in path.parts or "build" in path.parts:
            continue
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, ValueError) as exc:
            raise ValueError(f"{path}: {exc}") from exc
        label = data.get("label")
        if isinstance(label, str) and label:
            labels[label] = path
    return labels


def expected_labels(root: Path) -> dict[str, Path | None]:
    """Return the labels the catalogue must carry a key for."""
    expected: dict[str, Path | None] = {
        label: path
        for label, path in declared_labels(root).items()
        if label not in NO_KEY_DESPITE_A_FILE
    }
    for label in KEY_WITHOUT_A_FILE:
        expected.setdefault(label, None)
    return expected


def label_entries(catalogue: dict[str, object]) -> dict[str, str]:
    """Return the label keys of a catalogue, mapped to their message.

    Keys carrying a further segment, such as
    `...link.generated-index.description`, are not labels and are left out.
    """
    entries: dict[str, str] = {}
    for key, value in catalogue.items():
        if not key.startswith(LABEL_PREFIX):
            continue
        label = key[len(LABEL_PREFIX) :]
        if ".link." in key or label.endswith(".description"):
            continue
        if isinstance(value, dict) and isinstance(value.get("message"), str):
            entries[label] = value["message"]
    return entries


def catalogue_faults(
    expected: dict[str, Path | None],
    entries: dict[str, str],
    exemption: tuple[str, ...] = IDENTICAL_IN_BOTH_LANGUAGES,
) -> list[tuple[str, str]]:
    """Return every label at fault, with the rule it breaks.

    Six rules, and each is stated so that its opposite is also a fault, which
    is what stops the exemption from becoming a list of silence.
    """
    faults: list[tuple[str, str]] = []
    exempt = set(exemption)

    for label in sorted(expected):
        if label not in entries:
            faults.append((label, "no entry in the catalogue, translation missing"))

    for label in sorted(entries):
        message = entries[label]
        if label not in expected:
            faults.append(
                (label, "entry in the catalogue but no category declares this label")
            )
            continue
        if message == label and label not in exempt:
            faults.append((label, "message identical to the key, never translated"))
        elif message != label and label in exempt:
            faults.append(
                (
                    label,
                    "exempted as identical in both languages, yet a real "
                    "translation was written, so the exemption line is now false",
                )
            )

    for label in sorted(exempt):
        if label not in expected:
            faults.append(
                (label, "exempted label that no category declares, orphan exemption")
            )

    return faults


def report(
    category_root: Path = CATEGORY_ROOT, catalogue_path: Path = CATALOGUE
) -> int:
    root = REPO_ROOT / category_root
    path = REPO_ROOT / catalogue_path

    try:
        expected = expected_labels(root)
    except ValueError as exc:
        print(
            f"check_i18n_catalogue: NO COVERAGE, unreadable category, {exc}",
            file=sys.stderr,
        )
        return 2
    if len(expected) <= len(KEY_WITHOUT_A_FILE):
        print(
            f"check_i18n_catalogue: NO COVERAGE, no `_category_.json` read under "
            f"{category_root}",
            file=sys.stderr,
        )
        return 2

    try:
        catalogue = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        print(
            f"check_i18n_catalogue: NO COVERAGE, {catalogue_path} unreadable, {exc}",
            file=sys.stderr,
        )
        return 2

    entries = label_entries(catalogue)
    if not entries:
        print(
            f"check_i18n_catalogue: NO COVERAGE, {catalogue_path} carries no "
            f"`{LABEL_PREFIX}*` key",
            file=sys.stderr,
        )
        return 2

    faults = catalogue_faults(expected, entries)
    exempt_present = sorted(
        label for label in IDENTICAL_IN_BOTH_LANGUAGES if label in entries
    )
    untouched = [key for key in NOT_A_LABEL if key in catalogue]

    # ── The theme catalogues ─────────────────────────────────────────────────
    defaults = base_defaults(REPO_ROOT / BASE_LOCALE_DIR)
    if not defaults:
        print(
            f"check_i18n_catalogue: NO COVERAGE, no English default read under "
            f"{BASE_LOCALE_DIR}. The theme's base locale arrives with the "
            f"install: cd docs/site && npm ci",
            file=sys.stderr,
        )
        return 2
    try:
        code_entries = {
            key: value["message"]
            for key, value in json.loads(
                (REPO_ROOT / CODE_CATALOGUE).read_text(encoding="utf-8")
            ).items()
            if isinstance(value, dict) and isinstance(value.get("message"), str)
        }
        classic_entries: dict[str, dict[str, str]] = {}
        for name in ("navbar.json", "footer.json"):
            classic_entries[name] = {
                key: value["message"]
                for key, value in json.loads(
                    (REPO_ROOT / THEME_CLASSIC_DIR / name).read_text(encoding="utf-8")
                ).items()
                if isinstance(value, dict) and isinstance(value.get("message"), str)
            }
        config_text = (REPO_ROOT / SITE_CONFIG).read_text(encoding="utf-8")
    except (OSError, ValueError) as exc:
        print(f"check_i18n_catalogue: NO COVERAGE, theme catalogue unreadable, {exc}",
              file=sys.stderr)
        return 2
    fr_label = FR_LOCALE_LABEL.search(config_text)
    if fr_label is None:
        print(
            f"check_i18n_catalogue: NO COVERAGE, no `fr:` locale label found in "
            f"{SITE_CONFIG}",
            file=sys.stderr,
        )
        return 2

    theme_faults: list[tuple[str, str]] = []
    code_faults, blog_skipped = code_catalogue_faults(code_entries, defaults)
    theme_faults += [(f"{CODE_CATALOGUE.name}: {key}", why) for key, why in code_faults]
    for name, file_entries in classic_entries.items():
        theme_faults += [
            (f"{name}: {key}", why)
            for key, why in classic_catalogue_faults(file_entries)
        ]
    accent_subjects = {
        f"{CODE_CATALOGUE.name}: {key}": message
        for key, message in code_entries.items()
    }
    for name, file_entries in classic_entries.items():
        for key, message in file_entries.items():
            accent_subjects[f"{name}: {key}"] = message
    accent_subjects["docusaurus.config.js fr locale label"] = fr_label.group(1)
    theme_faults += unaccented_faults(accent_subjects)

    print(
        f"check_i18n_catalogue: {len(entries)} label entries read in "
        f"{catalogue_path}, {len(expected)} expected from "
        f"{category_root}/**/_category_.json"
    )
    print(
        f"check_i18n_catalogue: {len(code_entries)} theme strings in "
        f"{CODE_CATALOGUE.name} judged against {len(defaults)} English defaults "
        f"({blog_skipped} `theme.blog.*` skipped, blog disabled), plus "
        f"{sum(len(v) for v in classic_entries.values())} navbar and footer "
        f"labels and the `fr` locale label"
    )
    print(
        f"check_i18n_catalogue: code.json exempted as identical in both "
        f"languages: {', '.join(CODE_IDENTICAL)}"
    )
    faults += theme_faults
    print(
        f"check_i18n_catalogue: {len(exempt_present)} exempted as identical in "
        f"both languages: {', '.join(exempt_present)}"
    )
    print(
        f"check_i18n_catalogue: not a label, left alone: "
        f"{', '.join(untouched) if untouched else 'none'}"
    )
    sys.stdout.flush()

    if faults:
        print(
            f"\n{len(faults)} defect(s). The French sidebar takes its category "
            f"labels from this catalogue and from nowhere else, so an entry "
            f"that is missing or untranslated renders the English label with "
            f"nothing said about it:\n",
            file=sys.stderr,
        )
        for label, why in faults:
            shown = label if ": " in label or "locale label" in label else LABEL_PREFIX + label
            print(f"  {shown}", file=sys.stderr)
            print(f"    {why}", file=sys.stderr)
        print(
            "\nWrite the French message in the catalogue. If the two languages "
            "really spell the label the same, add it to the matching exemption "
            "list in this file (`IDENTICAL_IN_BOTH_LANGUAGES`, `CODE_IDENTICAL` "
            "or `CLASSIC_IDENTICAL`), where the next reader sees it and where "
            "the count above moves.",
            file=sys.stderr,
        )
        return 1

    print("check_i18n_catalogue: every expected label carries a French message")
    return 0


def _case(name: str, condition: bool) -> bool:
    print(f"  {'ok  ' if condition else 'FAIL'}  {name}")
    return condition


@contextlib.contextmanager
def _quiet():
    """Drive `report` for its exit code without printing its verdict twice."""
    with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(
        io.StringIO()
    ):
        yield


def _entry(message: str) -> dict[str, str]:
    return {"message": message, "description": "irrelevant to the rule"}


def selftest() -> int:
    print("i18n catalogue: both directions on a built subject")
    root = Path(tempfile.mkdtemp(prefix="check-i18n-catalogue-"))
    try:
        expected: dict[str, Path | None] = {
            "Troubleshooting": Path("x"),
            "Chat": Path("y"),
        }

        def faults(entries: dict[str, str], exemption=("Chat",)) -> list[str]:
            return [why for _, why in catalogue_faults(expected, entries, exemption)]

        results = [
            _case(
                "a label outside the exemption, untranslated, is named",
                faults({"Troubleshooting": "Troubleshooting", "Chat": "Chat"})
                == ["message identical to the key, never translated"],
            ),
            # Positive control for the case above. Without it, a detector red on
            # everything would satisfy the negative half while being worthless.
            _case(
                "the same label, translated, raises nothing",
                not faults({"Troubleshooting": "Dépannage", "Chat": "Chat"}),
            ),
            # The same shape, exempted and not exempted, so the exemption is
            # shown to be what makes the difference rather than the label.
            _case(
                "an exempted label whose message equals its key is not a fault",
                not faults({"Troubleshooting": "Dépannage", "Chat": "Chat"})
                and faults(
                    {"Troubleshooting": "Dépannage", "Chat": "Chat"}, exemption=()
                )
                == ["message identical to the key, never translated"],
            ),
            _case(
                "an exempted label carrying a real translation is a fault",
                faults({"Troubleshooting": "Dépannage", "Chat": "Discussion"})
                == [
                    "exempted as identical in both languages, yet a real "
                    "translation was written, so the exemption line is now false"
                ],
            ),
            _case(
                "an exemption naming a label no category declares is a fault",
                faults(
                    {"Troubleshooting": "Dépannage", "Chat": "Chat"},
                    exemption=("Chat", "Gone"),
                )
                == ["exempted label that no category declares, orphan exemption"],
            ),
            _case(
                "a declared label with no catalogue entry is a fault",
                faults({"Chat": "Chat"})
                == ["no entry in the catalogue, translation missing"],
            ),
            _case(
                "a catalogue entry no category declares is a fault",
                faults(
                    {
                        "Troubleshooting": "Dépannage",
                        "Chat": "Chat",
                        "Renamed": "Renommé",
                    }
                )
                == ["entry in the catalogue but no category declares this label"],
            ),
        ]

        # The theme catalogues, both directions each: equality with the
        # English default, the key-suffix rule, and the unaccented list.
        defaults = {
            "theme.x.expand": "Expand",
            "theme.blog.read": "Read more",
            "theme.adm.info": "info",
        }
        ok_entries = {
            "theme.x.expand": "Déplier",
            "theme.blog.read": "Read more",
            "theme.adm.info": "info",
        }
        code_ok, skipped = code_catalogue_faults(
            ok_entries, defaults, exemption=("theme.adm.info",)
        )
        results.append(
            _case(
                "a translated theme string and an exempted identical one pass, "
                "blog keys skipped and counted",
                not code_ok and skipped == 1,
            )
        )
        code_bad, _ = code_catalogue_faults(
            {"theme.x.expand": "Expand"}, defaults, exemption=()
        )
        results.append(
            _case(
                "a theme string equal to its English default is named",
                [why for _, why in code_bad]
                == ["message equals the English default, never translated"],
            )
        )
        code_stale, _ = code_catalogue_faults(
            {"theme.adm.info": "infos"}, defaults, exemption=("theme.adm.info",)
        )
        results.append(
            _case(
                "an exempted theme key carrying a real translation is stale",
                len(code_stale) == 1
                and "exemption line is now false" in code_stale[0][1],
            )
        )
        results.append(
            _case(
                "an untranslated navbar label is named, a translated one passes",
                [
                    why
                    for _, why in classic_catalogue_faults(
                        {"item.label.Reference": "Reference"}, exemption=()
                    )
                ]
                == ["message equals the key's English label, never translated"]
                and not classic_catalogue_faults(
                    {"item.label.Reference": "Référence"}, exemption=()
                ),
            )
        )
        results.append(
            _case(
                "an exempted classic label carrying a real translation is stale",
                len(
                    classic_catalogue_faults(
                        {"item.label.GitHub": "Forge"}, exemption=("GitHub",)
                    )
                )
                == 1,
            )
        )
        results.append(
            _case(
                "unaccented French is named, accented French passes",
                len(unaccented_faults({"a": "Signaler un probleme"})) == 1
                and not unaccented_faults({"a": "Signaler un problème"}),
            )
        )

        # The derivation itself, on a built subject, so that the key set the
        # rules above are fed is known to come from files and not from a
        # literal. `CLI reference` must drop out, `api` must appear.
        (root / "reference" / "cli").mkdir(parents=True)
        (root / "reference" / "cli" / "_category_.json").write_text(
            json.dumps({"label": "CLI reference", "position": 1}), encoding="utf-8"
        )
        (root / "operator-help" / "chat").mkdir(parents=True)
        (root / "operator-help" / "chat" / "_category_.json").write_text(
            json.dumps({"label": "Chat", "position": 3}), encoding="utf-8"
        )
        derived = expected_labels(root)
        results.append(
            _case(
                "the expected set is derived from files, `CLI reference` out, `api` in",
                sorted(derived) == ["Chat", "api"],
            )
        )

        # Nothing measured is not the same as nothing wrong, on both inputs.
        # `report` joins its arguments onto REPO_ROOT, and joining an absolute
        # path yields that path, so the fixtures below drive the real entry
        # point rather than a copy of its body.
        empty = root / "empty"
        empty.mkdir()
        with _quiet():
            no_category = report(empty, REPO_ROOT / CATALOGUE)
        results.append(
            _case(
                "a root with no category file reports nothing measured, not a pass",
                no_category == 2,
            )
        )

        labelless = root / "labelless.json"
        labelless.write_text(
            json.dumps({"version.label": _entry("Next")}), encoding="utf-8"
        )
        with _quiet():
            no_label = report(REPO_ROOT / CATEGORY_ROOT, labelless)
        results.append(
            _case(
                "a catalogue with no label key reports nothing measured, not a pass",
                no_label == 2,
            )
        )

        # Positive control on the same entry point: without it the two codes
        # above would prove the fixtures were empty only if `report` can reach
        # a verdict at all.
        with _quiet():
            real = report()
        results.append(
            _case(
                "positive control: the same entry point reaches a verdict on the tree",
                real in (0, 1),
            )
        )

        print()
        if all(results):
            print(f"self-test: all {len(results)} cases pass")
            return 0
        print(f"self-test: {results.count(False)} of {len(results)} cases fail")
        return 1
    finally:
        shutil.rmtree(root, ignore_errors=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--selftest", action="store_true", help="replay the fixture controls instead of measuring the tree"
    )
    if parser.parse_args().selftest:
        sys.exit(selftest())
    sys.exit(report())


if __name__ == "__main__":
    main()
