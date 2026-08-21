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

Exit codes:
    0  the catalogue was read, every expected label carries a French message
    1  at least one label is untranslated, orphaned, missing, or wrongly exempt
    2  nothing was measured: no catalogue, no category file, or no label key

Usage:
    python3 scripts/check_i18n_catalogue.py
    python3 scripts/check_i18n_catalogue.py --selftest
"""

import contextlib
import io
import json
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

    print(
        f"check_i18n_catalogue: {len(entries)} label entries read in "
        f"{catalogue_path}, {len(expected)} expected from "
        f"{category_root}/**/_category_.json"
    )
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
            print(f"  {LABEL_PREFIX}{label}", file=sys.stderr)
            print(f"    {why}", file=sys.stderr)
        print(
            "\nWrite the French message in the catalogue. If the two languages "
            "really spell the label the same, add it to "
            "`IDENTICAL_IN_BOTH_LANGUAGES` in this file, where the next reader "
            "sees it and where the count above moves.",
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
    if "--selftest" in sys.argv[1:]:
        sys.exit(selftest())
    sys.exit(report())


if __name__ == "__main__":
    main()
