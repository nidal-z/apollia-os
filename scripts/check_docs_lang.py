#!/usr/bin/env python3
"""Fail when a documentation page sits under the locale of the other language.

The operator help was migrated from French to English in one commit that
renamed nothing, and for three weeks the project's own overlay kept describing
`operator-help` as "still French under the default locale". Nothing measured
the language of a page against the locale serving it, so a page pasted into
the wrong tree, or a migration that missed a file, would be served verbatim
under a locale that promises the other language.

The classifier is a closed list of function words per language, counted after
stripping front matter, fenced code blocks, inline code, HTML comments and
URLs. Function words are the most frequent words of running prose and almost
never appear in identifiers, so a real page separates by an order of magnitude
(measured on this tree: the narrowest French page counts fr=52 en=38, every
other page is wider). A page whose dominant language disagrees with its locale
is reported; a page with no dominant language at all is reported too, because
a page this method cannot classify is a page a reader cannot classify either.

The inventory is `git ls-files`, never the disk, so the guard reads the same
set of files whatever tree it runs in.

Exit codes:
    0  every tracked page sits under the locale of its language
    1  at least one page is under the wrong locale, or classifies as neither
    2  nothing measured: no tracked page under a root

Usage:
    python3 scripts/check_docs_lang.py
    python3 scripts/check_docs_lang.py --all       (print every page with counts)
    python3 scripts/check_docs_lang.py --selftest
"""

import argparse
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

EN_ROOT = Path("docs/site/docs")
FR_ROOT = Path("docs/site/i18n/fr/docusaurus-plugin-content-docs/current")

FR_WORDS = {"le", "la", "les", "des", "une", "est", "et", "dans", "pour", "que", "qui",
            "pas", "sur", "avec", "vous", "ce", "cette", "ces", "sont", "votre", "vos",
            "ou", "mais", "aussi", "comme", "par", "au", "aux", "du", "elle", "il", "nous",
            "être", "fait", "peut", "si", "sans", "plus", "très", "chaque"}
EN_WORDS = {"the", "and", "is", "of", "to", "in", "that", "with", "for", "you", "your",
            "this", "are", "it", "on", "be", "as", "not", "or", "an", "by", "from", "can",
            "which", "when", "if", "each", "has", "have", "will", "into", "than", "then",
            "they", "their", "there", "these", "those", "what", "how"}

FENCE = re.compile(r"```.*?```", re.S)
INLINE = re.compile(r"`[^`\n]*`")
COMMENT = re.compile(r"<!--.*?-->", re.S)
URL = re.compile(r"https?://\S+")
FRONT = re.compile(r"\A---\n.*?\n---\n", re.S)
WORD = re.compile(r"[a-zA-Zàâçéèêëîïôûùüÿœ']+")

PAGE_SUFFIXES = (".md", ".mdx")


def tracked_pages(repo: Path, root: Path) -> list[Path]:
    result = subprocess.run(
        ["git", "ls-files", "-z", "--", str(root)],
        cwd=repo,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"`git ls-files` exited {result.returncode} on {root}: "
            f"{result.stderr.strip()!r}"
        )
    return sorted(
        Path(entry)
        for entry in result.stdout.split("\0")
        if entry and entry.endswith(PAGE_SUFFIXES)
    )


def classify(path: Path) -> tuple[str, int, int, int]:
    """Return (language, fr count, en count, total words) for one page."""
    text = path.read_text(encoding="utf-8", errors="replace")
    text = FRONT.sub("", text)
    for rx in (FENCE, COMMENT, INLINE, URL):
        text = rx.sub(" ", text)
    words = [w.lower() for w in WORD.findall(text)]
    fr = sum(1 for w in words if w in FR_WORDS)
    en = sum(1 for w in words if w in EN_WORDS)
    lang = "fr" if fr > en else "en" if en > fr else "?"
    return lang, fr, en, len(words)


def report(
    repo: Path = REPO_ROOT,
    roots: tuple[tuple[Path, str], ...] = ((EN_ROOT, "en"), (FR_ROOT, "fr")),
    show_all: bool = False,
) -> int:
    bad = 0
    total = 0
    for root, expect in roots:
        try:
            pages = tracked_pages(repo, root)
        except RuntimeError as exc:
            print(f"check_docs_lang: NO COVERAGE, {exc}", file=sys.stderr)
            return 2
        if not pages:
            print(
                f"check_docs_lang: NO COVERAGE, git tracks no page under {root}",
                file=sys.stderr,
            )
            return 2
        for page in pages:
            total += 1
            lang, fr, en, n = classify(repo / page)
            if show_all or lang != expect:
                flag = "" if lang == expect else "  <-- MISMATCH"
                stream = sys.stdout if lang == expect else sys.stderr
                print(f"  {page}  fr={fr} en={en} words={n} -> {lang}{flag}", file=stream)
            if lang != expect:
                bad += 1
    print(f"check_docs_lang: {total} pages, {bad} under the wrong locale")
    if bad:
        print(
            "\nA page served under a locale that promises the other language "
            "is wrong for every reader who lands on it. Move it to the tree "
            "of its language, or translate it in place.",
            file=sys.stderr,
        )
        return 1
    return 0


def _case(name: str, condition: bool) -> bool:
    print(f"  {'ok  ' if condition else 'FAIL'}  {name}")
    return condition


EN_PAGE = """\
---
title: A page
---

# A page

This is the body of the page, and it explains what you can do with the tool
when you install it on your machine. It is written in English from the first
line to the last, which is what the English tree promises.
"""

FR_PAGE = """\
---
title: Une page
---

# Une page

Ceci est le corps de la page, et il explique ce que vous pouvez faire avec
l'outil une fois que vous l'avez installé sur votre machine. Il est écrit en
français de la première ligne à la dernière, ce que l'arbre français promet.
"""


def selftest() -> int:
    print("docs lang: both directions on a built subject")
    root = Path(tempfile.mkdtemp(prefix="check-docs-lang-"))
    try:
        en = root / "docs/site/docs"
        fr = root / "docs/site/i18n/fr/docusaurus-plugin-content-docs/current"
        en.mkdir(parents=True)
        fr.mkdir(parents=True)
        (en / "page.md").write_text(EN_PAGE, encoding="utf-8")
        (fr / "page.md").write_text(FR_PAGE, encoding="utf-8")
        subprocess.run(["git", "init", "-q"], cwd=root, check=True)
        subprocess.run(["git", "add", "docs"], cwd=root, check=True)

        en_rel = Path("docs/site/docs")
        fr_rel = Path("docs/site/i18n/fr/docusaurus-plugin-content-docs/current")

        def run() -> int:
            return report(root, ((en_rel, "en"), (fr_rel, "fr")))

        results = [
            # The positive control. Without it, a classifier that reported
            # every page as mismatched would satisfy the case below while
            # being worthless.
            _case("a page in the language of its locale passes", run() == 0),
        ]

        # The defect this guard exists for: the French page copied under the
        # English root, byte for byte, served as English.
        (en / "page.md").write_text(FR_PAGE, encoding="utf-8")
        results.append(_case("a French page under the English root is reported", run() == 1))
        (en / "page.md").write_text(EN_PAGE, encoding="utf-8")

        # A page the classifier cannot separate is a defect, not a pass.
        (en / "page.md").write_text("---\ntitle: x\n---\n\n# x\n\n`code` only\n",
                                    encoding="utf-8")
        results.append(_case("a page with no dominant language is reported", run() == 1))
        (en / "page.md").write_text(EN_PAGE, encoding="utf-8")

        results.append(
            _case(
                "a root matching no tracked page reports nothing measured",
                report(root, ((Path("docs/site/docs/none"), "en"),)) == 2,
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
    parser = argparse.ArgumentParser(
        description="Classify each documentation page and match it to its locale."
    )
    parser.add_argument(
        "--all", action="store_true", help="print every page with its counts"
    )
    parser.add_argument(
        "--selftest",
        action="store_true",
        help="replay a positive and a negative control on a fixture, never on the tree",
    )
    args = parser.parse_args()
    if args.selftest:
        sys.exit(selftest())
    sys.exit(report(show_all=args.all))


if __name__ == "__main__":
    main()
