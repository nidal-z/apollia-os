#!/usr/bin/env python3
"""Fail when a documentation page is written but never rendered.

Docusaurus writes one output file per route. When a `_category_.json` declares
`link.type = "generated-index"` on the slug its own directory already serves
through `index.md`, the two routes land on the same output file and the
generated index wins. The hand-written page is still parsed, still listed in
the sidebar as a card, and never rendered as a page.

That matters beyond the lost prose. The build's link verifier collects the
links it finds while rendering, so a page that is never rendered has its links
collected from nowhere: a dead internal route written in it travels through
`npm run build` in exit 0, with `onBrokenLinks: 'throw'` set and working. Five
sections of this site were in that state, in both locales, which put ten source
pages and their links outside the reach of the only judge the repository has
for them.

What counts as a violation: a `_category_.json` that declares a `link` while
its directory holds an `index.md` or an `index.mdx`. What does not: a `link` in
a directory with no index page, which is the normal way to give a category a
generated landing page, and an index page in a directory whose `_category_.json`
declares no `link`, which is the convention that makes the index page the
category landing page.

A second rule, on the same walk. A `_category_.json` under
`i18n/*/docusaurus-plugin-content-docs/` is a dead file: the build never opens
it, in any locale. Measured with an `fs` tracer on a French build, which opened
twenty category files, all of them under `docs/site/docs`, and none under
`i18n/fr`, while the same tracer showed the 97 French page files being read.
The code says the same, `plugin-content-docs/lib/sidebars/index.js` calling
`readCategoriesMetadata(version.contentPath)` with `contentPath` fixed at
`siteDir/docs`. Eighteen such files existed here, carrying labels no reader
ever saw, and the guard used to walk them and count them as coverage. They are
reported now so that one coming back is loud rather than decorative.

A third rule, on the page inventory rather than the category walk. The site's
default locale is English, and the route of a page under it must read as
English: no accented character, no French word among its segments. Fifty pages
of the operator help were served under French URLs because a migration
translated their content without touching their file names and nothing judged
the language of a route. The route is the declared `slug:` when the front
matter carries one, and the file path otherwise, which is exactly the order
Docusaurus applies. The inventory is `git ls-files`, never the disk, so the
rule reads the same set of files whatever tree it runs in, and a fixture is
driven through the detector on every run: a French path, a French accent and a
clean English slug, so a run that measured a clean tree is distinguishable
from a detector that stopped firing.

What this does not catch, and it is deliberate: every other reason a source
page may fail to render, such as `draft: true` or a route collision introduced
by a plugin. That class is only observable after a build, and this guard reads
sources. The `slug` a `_category_.json` gives its generated index is also not
judged here: those files are shared by every locale, so an English slug there
would rename the French category routes with it, and that trade is a decision,
not a guard's.

Exit codes:
    0  every category file read, no masked index page, no dead file, every
       default-locale route in English
    1  an index page is masked by its own category link, a dead file exists,
       or a default-locale page has a French route
    2  nothing was measured, a section that must be covered was not read, or
       the French-route detector failed its own fixture

Usage:
    python3 scripts/check_docs_routes.py
"""

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

# The whole site tree, both locales, and it stays that way even though the
# French mirror carries no `_category_.json` any more. Narrowing the walk to
# `docs/site/docs` would make the second rule below unable to see a file coming
# back under `i18n/`, which is the only thing that keeps the mirror clean.
SCAN_ROOT = Path("docs/site")

# A category file here governs nothing: the build reads category metadata from
# `docs/site/docs` whatever locale it renders. See the module docstring for the
# measurement.
LOCALIZED_MARKER = ("i18n", "docusaurus-plugin-content-docs")

# Derived or vendored output. `docs/reference/api` is not excluded: it is
# regenerated at build time and carries no `_category_.json` today, and if the
# generator ever emits one it should be judged like any other.
EXCLUDED_DIRS = {
    "node_modules",
    "build",
    ".docusaurus",
    "dist",
    ".git",
}

INDEX_NAMES = ("index.md", "index.mdx")

# The default locale of the site, whose routes must read as English.
DEFAULT_LOCALE_ROOT = Path("docs/site/docs")

FRONT_MATTER = re.compile(r"\A---\r?\n(.*?)\r?\n---\r?\n", re.DOTALL)
SLUG_LINE = re.compile(r"^slug\s*:\s*(\S+)\s*$", re.MULTILINE)

# French words, matched as whole hyphen-separated route segments. The list is
# closed and short on purpose: every word here is unambiguously French in a
# URL, so a hit is a verdict and not a suspicion. English words that France
# also spells (`installation`, `notifications`, `chat`, `action`) are absent
# by construction.
FRENCH_ROUTE_WORDS = frozenset({
    # articles, pronouns, prepositions
    "le", "la", "les", "un", "une", "des", "du", "de", "d", "l",
    "et", "ou", "sur", "avec", "au", "aux", "votre", "vos", "sa", "ses",
    "son", "mon", "ne", "pas", "est",
    # verbs seen in a slug
    "installer", "configurer", "connecter", "telecharger", "choisir",
    "consulter", "demarrer", "mesurer", "programmer", "suivre", "activer",
    "discuter", "approuver", "refuser", "inspecter", "cabler", "comprendre",
    "gerer", "tester", "nettoyer", "surveiller", "creer", "lier", "naviguer",
    "trouver", "utiliser", "reinitialiser", "mettre", "repond", "transcrit",
    "demarre",
    # nouns and adjectives seen in a slug or a directory name
    "depannage", "dictee", "vocale", "memoire", "historique", "taches",
    "couts", "portee", "chargement", "differe", "palier", "autonomie",
    "fichiers", "outil", "projet", "projets", "profil", "modele", "modeles",
    "locaux", "routage", "hybride", "serveur", "propre", "connexion",
    "fournisseur", "compagnonne", "clavier", "visite", "guidee", "donnees",
    "controle", "observabilite", "automatisations", "canal", "vue",
    "ensemble", "jour", "bloque", "refusee", "rien",
})

# The fixture the detector must fire on, and the case it must let through,
# driven on every run. The French entries are composed so that a clean tree
# and a dead detector cannot both answer green.
ROUTE_FIXTURE = [
    ("operator-help/installer-un-agent.md", None, True),
    ("guide/présentation.md", None, True),
    ("operator-help/agents/install-an-agent.md",
     "/operator-help/agents/install-an-agent", False),
]

# The sections this guard was written for, asserted positively so that
# narrowing SCAN_ROOT fails the run instead of quietly reporting green on a
# smaller set. A count alone would not do it: a walk that found many files
# proves it walked, not that it walked here.
#
# The five French anchors this list used to carry are gone with the files
# themselves. Twelve English anchors take their place, the operator-help
# subsections, which is the zone the `link` blocks now live in and which the
# list did not name before. Named coverage goes from ten to seventeen in the
# same move that drops the walk from 38 files to 20, and the dead-file count
# printed beside it is what explains the drop instead of letting it vanish.
REQUIRED_COVERAGE = [
    Path("docs/site/docs/explanation/_category_.json"),
    Path("docs/site/docs/how-to/_category_.json"),
    Path("docs/site/docs/operator-help/_category_.json"),
    Path("docs/site/docs/reference/_category_.json"),
    Path("docs/site/docs/tutorials/_category_.json"),
    Path("docs/site/docs/operator-help/agents/_category_.json"),
    Path("docs/site/docs/operator-help/automatisations/_category_.json"),
    Path("docs/site/docs/operator-help/chat/_category_.json"),
    Path("docs/site/docs/operator-help/controle/_category_.json"),
    Path("docs/site/docs/operator-help/installation/_category_.json"),
    Path("docs/site/docs/operator-help/integrations/_category_.json"),
    Path("docs/site/docs/operator-help/memoire/_category_.json"),
    Path("docs/site/docs/operator-help/notifications/_category_.json"),
    Path("docs/site/docs/operator-help/observabilite/_category_.json"),
    Path("docs/site/docs/operator-help/projets/_category_.json"),
    Path("docs/site/docs/operator-help/transversal/_category_.json"),
    Path("docs/site/docs/operator-help/troubleshooting/_category_.json"),
]


def iter_category_files(root: Path) -> list[Path]:
    """Return every `_category_.json` under `root`, derived trees excluded."""
    if not root.is_dir():
        return []
    files = []
    for path in root.rglob("_category_.json"):
        if not path.is_file():
            continue
        if EXCLUDED_DIRS.intersection(path.relative_to(root).parts):
            continue
        files.append(path)
    return sorted(set(files))


def index_sibling(category_file: Path) -> Path | None:
    """Return the index page that `category_file` sits next to, if any."""
    for name in INDEX_NAMES:
        candidate = category_file.parent / name
        if candidate.is_file():
            return candidate
    return None


def masked_pages(root: Path) -> list[tuple[Path, Path, str]]:
    """Return every index page a sibling category link takes the route from.

    Each entry is the category file, the index page it masks, and the `link`
    declaration that masks it, all as read from disk. Pure with respect to the
    tree it is given, so a caller can drive it on a fixture that violates the
    rule as well as on the repository, which is the only way to know the
    detector fires at all.
    """
    hits = []
    for category_file in iter_category_files(root):
        try:
            data = json.loads(category_file.read_text(encoding="utf-8"))
        except (OSError, ValueError) as exc:
            raise ValueError(f"{category_file}: {exc}") from exc
        link = data.get("link")
        if not link:
            continue
        index = index_sibling(category_file)
        if index is None:
            continue
        hits.append((category_file, index, json.dumps(link, sort_keys=True)))
    return hits


def dead_localized_files(root: Path) -> list[Path]:
    """Return every category file the build will never open.

    Pure with respect to the tree it is given, like `masked_pages`, so a caller
    can drive it on a fixture that carries one as well as on the repository,
    which is the only way to know the detector fires at all.
    """
    dead = []
    for category_file in iter_category_files(root):
        parts = category_file.relative_to(root).parts
        if all(marker in parts for marker in LOCALIZED_MARKER):
            dead.append(category_file)
    return dead


def tracked_default_locale_pages() -> list[tuple[str, str | None]]:
    """Return `(path, slug)` for every page git tracks under the default locale.

    The path is relative to the locale root, the slug is the raw front-matter
    value when the page declares one. Raises `RuntimeError` when git itself
    fails, so that an inventory nobody could read reports nothing measured
    rather than an empty list.
    """
    pathspecs = [
        f"{DEFAULT_LOCALE_ROOT}/**/*.md",
        f"{DEFAULT_LOCALE_ROOT}/**/*.mdx",
        f"{DEFAULT_LOCALE_ROOT}/*.md",
        f"{DEFAULT_LOCALE_ROOT}/*.mdx",
    ]
    result = subprocess.run(
        ["git", "ls-files", "-z", "--", *pathspecs],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"`git ls-files` exited {result.returncode}: {result.stderr.strip()!r}"
        )
    pages = []
    for entry in sorted(set(result.stdout.split("\0"))):
        if not entry:
            continue
        rel = Path(entry).relative_to(DEFAULT_LOCALE_ROOT)
        text = (REPO_ROOT / entry).read_text(encoding="utf-8")
        slug = None
        matter = FRONT_MATTER.match(text)
        if matter:
            found = SLUG_LINE.search(matter.group(1))
            if found:
                slug = found.group(1).strip().strip("'\"")
        pages.append((str(rel), slug))
    return pages


def route_of(rel_path: str, slug: str | None) -> str:
    """Return the route Docusaurus serves for a page, as the guard models it.

    An absolute slug is the whole route. A relative slug replaces the last
    path segment. Without a slug the route is the file path, extension
    dropped, an `index` stem folding into its directory. Number prefixes are
    left in place: they carry no language, so the rule reads through them.
    """
    if slug and slug.startswith("/"):
        return slug.strip("/")
    stem = re.sub(r"\.mdx?$", "", rel_path)
    parts = stem.split("/")
    if parts[-1] == "index":
        parts = parts[:-1]
    if slug:
        parts = parts[:-1] + [slug.strip("/")]
    return "/".join(parts)


def french_routes(pages: list[tuple[str, str | None]]) -> list[tuple[str, str, str]]:
    """Return `(path, route, reason)` for every page whose route is not English.

    Pure with respect to the inventory it is given, so a caller can drive it
    on a fixture that violates the rule as well as on the repository, which is
    the only way to know the detector fires at all.
    """
    hits = []
    for rel_path, slug in pages:
        route = route_of(rel_path, slug)
        accented = sorted({ch for ch in route if ord(ch) > 127})
        if accented:
            hits.append((rel_path, route, f"accented: {' '.join(accented)}"))
            continue
        tokens = [t for t in re.split(r"[/_.-]+", route.lower()) if t]
        words = sorted(set(tokens) & FRENCH_ROUTE_WORDS)
        if words:
            hits.append((rel_path, route, f"french: {' '.join(words)}"))
    return hits


def route_detector_broken() -> str | None:
    """Drive the fixture through the detector, return what failed, if anything."""
    hits = french_routes([(p, s) for p, s, _ in ROUTE_FIXTURE])
    flagged = {path for path, _, _ in hits}
    for path, _, expected in ROUTE_FIXTURE:
        if expected and path not in flagged:
            return f"fixture {path} should be flagged and was not"
        if not expected and path in flagged:
            return f"fixture {path} should pass and was flagged"
    return None


def uncovered_required(root: Path) -> list[Path]:
    """Return the sections that exist on disk but fell outside the walk."""
    scanned = set(iter_category_files(root))
    missing = []
    for rel in REQUIRED_COVERAGE:
        path = REPO_ROOT / rel
        if path.is_file() and path not in scanned:
            missing.append(rel)
    return missing


def main() -> int:
    root = REPO_ROOT / SCAN_ROOT
    files = iter_category_files(root)
    if not files:
        print(
            f"check_docs_routes: NO COVERAGE, no _category_.json under {SCAN_ROOT}",
            file=sys.stderr,
        )
        return 2

    missing = uncovered_required(root)
    if missing:
        print(
            "check_docs_routes: NO COVERAGE, these sections exist but were not "
            "read:",
            file=sys.stderr,
        )
        for rel in missing:
            print(f"  {rel}", file=sys.stderr)
        return 2

    try:
        hits = masked_pages(root)
    except ValueError as exc:
        print(f"check_docs_routes: unreadable category file, {exc}", file=sys.stderr)
        return 2

    broken = route_detector_broken()
    if broken:
        print(
            f"check_docs_routes: NO COVERAGE, the French-route detector failed "
            f"its own fixture: {broken}",
            file=sys.stderr,
        )
        return 2

    try:
        pages = tracked_default_locale_pages()
    except (RuntimeError, OSError, UnicodeDecodeError) as exc:
        print(f"check_docs_routes: unreadable page inventory, {exc}", file=sys.stderr)
        return 2
    if not pages:
        print(
            f"check_docs_routes: NO COVERAGE, git tracks no page under "
            f"{DEFAULT_LOCALE_ROOT}",
            file=sys.stderr,
        )
        return 2
    french = french_routes(pages)

    dead = dead_localized_files(root)
    with_index = sum(1 for path in files if index_sibling(path) is not None)
    print(
        f"check_docs_routes: {len(files)} category files read under {SCAN_ROOT}, "
        f"{with_index} next to an index page, {len(hits)} masked, "
        f"{len(dead)} dead under a localized tree, {len(pages)} default-locale "
        f"pages read, {len(french)} on a French route"
    )
    sys.stdout.flush()

    if dead:
        print(
            f"\n{len(dead)} category file(s) under a localized tree. The build "
            f"reads category metadata from {SCAN_ROOT}/docs whatever locale it "
            f"renders, so whatever these declare governs nothing, and the next "
            f"reader will believe it does:\n",
            file=sys.stderr,
        )
        for path in dead:
            print(f"  {path.relative_to(REPO_ROOT)}", file=sys.stderr)
        print(
            "\nDelete them. A category label for a locale is translated in that "
            "locale's `current.json`, which is the only file the build reads it "
            "from.",
            file=sys.stderr,
        )
        return 1

    if hits:
        print(
            f"\n{len(hits)} index page(s) are written but never rendered. Their "
            f"category declares a link on the route the index page already "
            f"serves, so the generated page wins and the source page, with every "
            f"link in it, leaves the build's link verifier:\n",
            file=sys.stderr,
        )
        for category_file, index, link in hits:
            rel_cat = category_file.relative_to(REPO_ROOT)
            rel_idx = index.relative_to(REPO_ROOT)
            print(f"  {rel_cat}", file=sys.stderr)
            print(f"    masks {rel_idx}", file=sys.stderr)
            print(f"    link  {link}", file=sys.stderr)
        print(
            "\nRemove the `link` block. Docusaurus then takes the index page as "
            "the category landing page, and the sidebar entry keeps working.",
            file=sys.stderr,
        )
        return 1

    if french:
        print(
            f"\n{len(french)} page(s) under the default locale are served on a "
            f"route that does not read as English. The default locale promises "
            f"English, and the route is part of the page:\n",
            file=sys.stderr,
        )
        for rel_path, route, reason in french:
            print(f"  {DEFAULT_LOCALE_ROOT / rel_path}", file=sys.stderr)
            print(f"    route  /{route}", file=sys.stderr)
            print(f"    {reason}", file=sys.stderr)
        print(
            "\nDeclare an English `slug:` in the page's front matter. The file "
            "keeps its name, its history and its French mirror; only the URL "
            "changes, and the old URL gets a redirect in "
            "docs/site/plugins/operator-help-redirects.js.",
            file=sys.stderr,
        )
        return 1

    print(
        "check_docs_routes: every index page owns its route, every "
        "default-locale route reads as English"
    )
    return 0


if __name__ == "__main__":
    argparse.ArgumentParser(description=__doc__.splitlines()[0]).parse_args()
    sys.exit(main())
