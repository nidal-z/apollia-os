#!/usr/bin/env python3
"""One language per file, on the code as well as on the prose.

`docs/agents/FORBIDDEN.md` forbids mixing French and English in the same file,
and allocates the code to English: doc-comments in English, inline comments
"being translated to English as each file is touched". Nothing measured either
half. `check_prose.py` judges the em-dash, the tracker references and the PII;
`check_docs_lang.py` judges the language of a documentation page against the
locale serving it, under `docs/site/` alone. Between the two, every `.rs`,
`.ts`, `.svelte`, `.py`, `.sh`, `.yml` and `.toml` file of the tree was
unjudged, and a convention with no guard is a wish: measured on this tree
before this file existed, 819 French comment lines and 475 French string lines
sat in code that is otherwise English, including twenty-two files touched
after the rule was written.

The half that matters most is the one a comment sweep misses. A French
`#[error("erreur SQLite : {0}")]`, a French `expect()` message and a French
assertion message are not comments: they are text the binary prints to an
operator, or that a failing test prints to a developer, in the middle of an
otherwise English output. So the unit judged here is the line, whatever it
carries.

Two rules, because a code file and a document do not fail the same way.

  code       Every French line is a defect. The allocation says the code is
             English, so there is no second language to mix with: one line is
             enough. `FRENCH_DATA` names the files whose French is *content*
             rather than prose (a locale catalogue, a French vocabulary table
             a parser matches on, a fixture of French user text), each with
             the exact number of French lines it is allowed, as a two-sided
             ratchet: a file above its number is a regression, a file below it
             is debt paid that must lower the number in the same commit.

  document   A document may be written in French: `NEVER mix` is not `NEVER
             French`. What fails is the mixture, so a document is red when it
             carries more than two French lines *and* more than two English
             lines. `BILINGUAL_DATA` names the data files that carry both
             languages by construction, one value per locale.

Both rules read the git inventory, never the disk, so the guard judges the
same set of files whatever tree it runs in.

Detection is a closed list of function words per language plus the accented
letters of French, counted per line, with the majority language winning: an
English line quoting a French label ("the `Dépannage` section") counts one
accent and one English function word, and is not French. That case is real,
it produced eighteen false positives in the sweep this guard replaces, and it
is pinned by `--selftest`.

Exit codes:
    0  every tracked file is one language, and the code is English
    1  at least one file breaks a rule
    2  nothing measured: the git inventory is unreadable or empty

Usage:
    python3 scripts/check_file_language.py
    python3 scripts/check_file_language.py --list
    python3 scripts/check_file_language.py --selftest
"""

import argparse
import contextlib
import io
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

# Function words: the most frequent words of running prose, and the ones that
# almost never appear inside an identifier. The two lists are disjoint on
# purpose, so a word can never vote for both languages.
FR_WORDS = {
    "afin", "alors", "aucun", "aucune", "aussi", "autre", "autres", "aux",
    "avant", "avec", "avoir", "car", "ce", "cela", "ces", "cet", "cette",
    "chaque", "comme", "dans", "depuis", "des", "doit", "doivent", "donc",
    "dont", "du", "elle", "encore", "en", "entre", "est", "et", "fait",
    "faut", "de",
    "hors", "ici", "jamais", "la", "le", "les", "leur", "leurs", "lors",
    "lui", "mais", "meme", "moins", "ne", "nous", "ou", "par", "pas",
    "pendant", "peut", "peuvent", "plus", "pour", "puis", "qu", "quand",
    "que", "qui", "rien", "sans", "se", "selon", "sera", "ses", "seulement",
    "si", "sinon", "soit", "son", "sont", "sur", "tous", "tout", "toute",
    "toutes", "toujours", "trop", "un", "une", "vers", "voir", "vous",
    "affiche", "ajoute", "appel", "champ", "chemin", "erreur", "fichier",
    "ligne", "liste", "renvoie", "retourne", "utilise", "valeur",
    # Content words with no English homograph, gathered from the lines the
    # function words alone left behind on this tree.
    "ainsi", "apres", "attendu", "attendue", "auquel", "autant", "beaucoup",
    "cependant", "chacun", "desormais", "ecrit", "ecrite",
    "ensuite", "etre", "facon", "gras", "jusqu", "lequel",
    "lorsque", "lorsqu", "malgre", "neanmoins", "parfois", "plutot",
    "pourquoi", "pourtant", "presque", "puisque", "quelque", "quelques",
    "souvent", "surtout", "tandis", "tellement", "toutefois", "vide",
    "aucun", "aucune", "chaine", "couleur", "lecture", "ecriture", "obtenu",
    "obtenue", "premiere", "derniere", "suivante", "precedent", "precedente",
    # The vocabulary of a diagnostic message. Four `#[error]` attributes and a
    # notification body survived every sweep of this campaign because not one
    # of `invalide`, `introuvable`, `indisponible`, `inconnu` and `manquant`
    # was on this list: the line carried no French evidence at all, so no
    # arbitration between the languages ever took place. Each has no English
    # homograph, which is why they can sit here and `active` or `refuse`
    # cannot: those two read as English on a `data-testid` and produced
    # forty-nine hits naming no language when they were tried.
    "invalide", "invalides", "introuvable", "introuvables", "indisponible",
    "indisponibles", "inconnu", "inconnue", "inconnus", "inconnues",
    "manquant", "manquante", "manquants", "manquantes",
    "supprime", "supprimee", "supprimes", "supprimees",
    "purgee", "purgees", "entree", "entrees",
}
EN_WORDS = {
    "a", "all", "also", "an", "and", "any", "are", "as", "at", "be",
    "because", "been", "before", "both", "but", "by", "can", "did", "do",
    "does", "each", "either", "every", "for", "from", "has", "have", "how",
    "if", "in", "into", "is", "it", "its", "just", "like", "may", "must",
    "no", "not", "of", "on", "once", "one", "only", "or", "should", "since",
    "so", "than", "that", "the", "their", "them", "then", "there", "these",
    "they", "this", "those", "to", "unless", "until", "was", "were", "what",
    "when", "which", "while", "why", "will", "with", "would",
}

# The French words above that an English text also writes, that name a locale,
# or that a short identifier collides with. They vote, but they cannot carry a
# line on their own.
FR_WEAK = {
    "car", "de", "du", "en", "est", "et", "fait", "la", "le", "ne", "ou",
    "par", "pas", "plus", "qu", "se", "si", "son", "sur", "tout", "un", "une",
    "voir",
}

ACCENTS = re.compile(r"[àâçéèêëîïôöûùüÿœ]", re.I)
# A word, and not a run of letters inside a longer token: without the two
# lookarounds, the hexadecimal of a pinned GitHub action reads as French
# ("...57747ce8" yields `ce`) and every workflow file fails on its own SHAs.
# A leading dot is excluded too: `fontFamily.sans` is a property, not `sans`.
WORD = re.compile(r"(?<![A-Za-z0-9_.])[A-Za-zÀ-ÖØ-öø-ÿŒœ]+(?![A-Za-z0-9_])")

# Spans that are not prose in either language: a URL, an i18n key, a locale
# tag, a CSS font keyword. They are blanked before the words are counted, so
# `font-sans` stops voting French through `sans`. Measured on this tree, each
# of them produced a hit that named no language at all.
NEUTRAL = re.compile(r"https?://\S*|\blocale\b|\bfr-FR\b|\ben-US\b|sans-serif|font-sans|\bsans\s*:")


# GIVEN / WHEN / THEN mark the structure of a test comment, in every language:
# `TESTING.md` requires them in that exact uppercase form. Counted as English
# they outvoted the French of the sentence around them, and every French test
# comment of the tree read as English.
GWT = re.compile(r"\b(?:GIVEN|WHEN|THEN|AND|BUT|SAFETY|REASON|TODO|NOTE)\b")


# A literal in a diagnostic-message position: the text an operator reads out
# of an error, a panic or a formatted message. Judging *every* literal was
# measured first and refused: it named twelve English comments that quote a
# French interface label ("the desktop \"Modifier les arguments\" dialog"),
# which is the same false positive the line rule was built to avoid, with
# double quotes in place of backticks. A message position is not a quotation.
MSG_LITERAL = re.compile(
    r'(?:\#\[error\(|format!\(|panic!\(|unreachable!\(|\.expect\('
    r'|\.context\(|\.with_context\(|write!\([^,\n]*,)\s*"((?:[^"\\\n]|\\.)*)"'
)
# A backticked span is a quotation, not code. This file's own docstring quotes
# `#[error("erreur SQLite : {0}")]` inside an English sentence, and so does the
# comment that explains the single-word case; without this the string rule
# would call both French.
BACKTICKED = re.compile(r"`[^`\n]*`")


def _words(line: str) -> list[str]:
    return [w.lower() for w in WORD.findall(NEUTRAL.sub(" ", GWT.sub(" ", line)))]


def french_message(line: str) -> bool:
    """True when a diagnostic message on the line reads as French on its own.

    The line rule counts the whole line, so an identifier votes as loudly as
    the text a user reads. A notification body that formatted a French
    sentence offered six words of which one was French, and a message diluted
    below the threshold by the code around it is invisible. Judging the
    literal on its own words removes the dilution. A literal of fewer than two
    words is skipped, because a one-word literal is a key or an identifier far
    more often than a sentence.
    """
    return any(
        _line_is_french(literal)
        for literal in MSG_LITERAL.findall(BACKTICKED.sub(" ", line))
        if len(_words(literal)) >= 2
    )


def is_french(line: str) -> bool:
    """True when the line, or a message it carries, reads as French."""
    return _line_is_french(line) or french_message(line)


def _line_is_french(line: str) -> bool:
    """True when the text of the line reads as French rather than as English."""
    words = _words(line)
    if not words:
        return False
    fr = sum(1 for w in words if w in FR_WORDS)
    en = sum(1 for w in words if w in EN_WORDS)
    if ACCENTS.search(NEUTRAL.sub(" ", line)):
        # An accent is French evidence unless the line is English prose that
        # quotes a French word: two English function words are enough to say
        # so, and that is exactly the case the previous sweep got wrong
        # eighteen times. A bare data line carrying a French label has no
        # English evidence at all, and counts as French, because it is: the
        # allowance table below is where such a line gets declared.
        return en < 2 or fr >= en
    if fr < en:
        return False
    strong = {w for w in words if w in FR_WORDS} - FR_WEAK
    if len(strong) >= 2:
        return True
    # One unambiguous French word carries a line only when nothing on it reads
    # as English: `#[error("erreur SQLite : {0}")]` has one, and no accent.
    return len(strong) == 1 and en == 0 and len(words) >= 2


def is_english(line: str) -> bool:
    """True when the line reads as English rather than as French."""
    words = _words(line)
    if not words:
        return False
    fr = sum(1 for w in words if w in FR_WORDS)
    en = sum(1 for w in words if w in EN_WORDS)
    return en > fr and en >= 2


# Files whose content is code, and therefore has to be English. A suffix is
# enough: the rule is about the language a reader of that file expects, and a
# `.rs` file is read as code wherever it sits.
CODE_SUFFIXES = {
    ".bat", ".cjs", ".css", ".html", ".js", ".mjs", ".plist", ".ps1", ".py",
    ".rs", ".sh", ".sql", ".svelte", ".toml", ".ts", ".tsx", ".yaml", ".yml",
}
CODE_NAMES = {"justfile", "Cross.toml", ".gitignore"}

# Documents: judged on the mixture, not on the language.
DOC_SUFFIXES = {".json", ".md", ".txt"}

# Out of this guard's reach, each for a stated reason.
OUT_OF_SCOPE = (
    # The documentation site has its own language guard, which judges a page
    # against the locale serving it: `scripts/check_docs_lang.py`. Two guards
    # over one tree would answer the same question twice and drift apart.
    "docs/site/",
    # Byte fixtures for the fuzz targets: their whole purpose is to carry text
    # no parser expects, accents included.
    "fuzz/seeds/",
)

# Documents written in French on purpose. `NEVER mix` is not `NEVER French`,
# so these are legal as long as they stay one language; they are named here
# because the mixture rule would otherwise fire on the English fragments they
# quote (a path, a Tailwind class, a table header).
FRENCH_DOCUMENTS = (
    "crates/apollia-desktop/ui/figma/GUIDE-FIGMA-FIRST.md",
    "crates/apollia-desktop/ui/src/lib/design/breakpoints.md",
    "crates/apollia-desktop/ui/src/lib/i18n/operator-glossary.md",
    "docs/README.md",
    "packaging/README.md",
)

# Data files that carry both languages by construction, and the reason each
# one does. A locale catalogue quotes the endonym of the other language
# ("Français" under `language_fr`); a connector enrichment ships one label per
# locale; a gesture script types what the operator types and names the capture
# after the page it illustrates.
BILINGUAL_DATA = (
    "crates/apollia-desktop/src/mcp/enrichments.json",
    "crates/apollia-desktop/ui/src/lib/i18n/en.json",
    "crates/apollia-desktop/ui/src/lib/i18n/fr.json",
    "scripts/automation/",
)

# Code files whose French is content rather than prose, with the exact number
# of French lines each is allowed. Two-sided: a file above its number is a
# regression, a file below it is debt paid whose number must come down in the
# same commit, otherwise the table records a maximum nobody comes back to.
#
# Every entry is one of four kinds, and nothing else gets in:
#   locale     a catalogue or a table of French user-facing text
#   parser     French words a parser matches on, or folds
#   fixture    French user text a test feeds to the code under test
#   prompt     French text sent to a model, because the operator writes French
FRENCH_DATA: dict[str, int] = {
    # locale: the French half of a bilingual surface
    "crates/apollia-desktop/src/i18n.rs": 9,
    "crates/apollia-desktop/ui/src/lib/i18n/identicalLocales.ts": 4,
    "crates/apollia-desktop/ui/src/components/settings/stt/SttEssentialSection.svelte": 2,
    "crates/apollia-memory/src/profile_schema.rs": 26,
    "scripts/check_release_artifacts.py": 1,
    # parser: French words the code matches on, or folds
    "agents/system/onboarding-agent/agent.py": 36,
    "crates/apollia-cli/src/commands/chat_stream.rs": 3,
    "crates/apollia-cli/src/commands/chat_stream/classify.rs": 1,
    "crates/apollia-cli/src/commands/run.rs": 2,
    "crates/apollia-desktop/ui/src/components/settings/profile/profileForm.ts": 1,
    "crates/apollia-desktop/ui/src/lib/command-palette/actions.ts": 1,
    "crates/apollia-desktop/ui/tests/settings/hotkey-capture.spec.ts": 1,
    "crates/apollia-llm/src/meta/parse_automation.rs": 30,
    "crates/apollia-memory/src/lib.rs": 1,
    "crates/apollia-memory/src/search.rs": 2,
    "crates/apollia-memory/src/store.rs": 1,
    "crates/apollia-memory/src/user_memory.rs": 1,
    "scripts/check_docs_lang.py": 8,
    "scripts/check_docs_routes.py": 4,
    # This file: its own French word list, and the French lines its self-test
    # feeds to the detector. A guard that exempted itself silently would be
    # the first place a French line could hide.
    "scripts/check_file_language.py": 44,
    "scripts/check_i18n_catalogue.py": 20,
    # parser: the French vocabulary of the method reference it reads, the
    # bracketed location tokens and the column header, plus the fixture table
    # its self-test parses. The table is French; a guard that read it in
    # English would match nothing.
    "scripts/check_method_references.py": 18,
    "scripts/check_python_rules.py": 4,
    "tests/cli/seed/files/agents/onboarding-agent/agent.py": 28,
    "tests/test_onboarding_heuristic_recovery.py": 9,
    # prompt: French text sent to a model, because the operator writes French
    "crates/apollia-desktop/src/commands/chat/title.rs": 17,
    # fixture: French user text, or a multi-byte character, a test feeds to the
    # code under test
    "crates/apollia-cli/src/commands/mod.rs": 2,
    "crates/apollia-core/src/observability.rs": 1,
    "crates/apollia-core/src/utils.rs": 2,
    "crates/apollia-desktop/ui/src/lib/automations/humanize.test.ts": 20,
    "crates/apollia-desktop/ui/src/lib/chat/attachments.test.ts": 1,
    "crates/apollia-desktop/ui/src/lib/i18n/audit-i18n.test.ts": 12,
    "crates/apollia-desktop/ui/src/lib/i18n/i18n-locale-switch.test.ts": 1,
    "crates/apollia-desktop/ui/src/lib/utils/slugify.test.ts": 3,
    "crates/apollia-llm/tests/interval_junction.rs": 3,
    "crates/apollia-mcp/src/sanitize.rs": 1,
    "crates/apollia-notifications/src/channels/webhook.rs": 2,
    "crates/apollia-runtime/src/api/routes_timeline.rs": 1,
    "sdk/tests/test_formatting.py": 2,
    "tests/integration/test_chat_memory.rs": 4,
    "tests/integration/test_onboarding.rs": 9,
    "tests/test_onboarding_agent.py": 41,
    "tests/test_onboarding_contracts.py": 2,
    # Latin, not French: the lorem ipsum of a search-result fixture shares its
    # function words with French ("et", "est", "qui", "in"). Named rather than
    # taught to the detector, because a Latin lexicon would blunt it.
    "crates/apollia-tools/src/tools/web_search/tests/fixtures/ddg_empty.html": 3,
}


def tracked_files() -> list[str]:
    proc = subprocess.run(
        ["git", "ls-files"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        return []
    return [line for line in proc.stdout.splitlines() if line]


def kind(rel: str) -> str | None:
    """`code`, `document`, or None when the file is out of reach."""
    if rel.startswith(OUT_OF_SCOPE):
        return None
    path = Path(rel)
    if path.name in CODE_NAMES:
        return "code"
    if path.suffix in CODE_SUFFIXES:
        return "code"
    if path.suffix in DOC_SUFFIXES:
        return "document"
    return None


def measure(root: Path, files: list[str]) -> tuple[list[str], dict[str, int], int]:
    """Return the failures, the French-line count per code file, and the files read."""
    failures: list[str] = []
    counts: dict[str, int] = {}
    read = 0
    for rel in sorted(files):
        what = kind(rel)
        if what is None:
            continue
        path = root / rel
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        read += 1
        lines = text.splitlines()
        french = [(n, line) for n, line in enumerate(lines, 1) if is_french(line)]
        if what == "code":
            counts[rel] = len(french)
            allowed = FRENCH_DATA.get(rel)
            if allowed is None:
                for n, line in french:
                    failures.append(f"{rel}:{n}: {line.strip()[:110]}")
            elif len(french) != allowed:
                verb = "above" if len(french) > allowed else "below"
                failures.append(
                    f"{rel}: {len(french)} French lines, {verb} the {allowed} "
                    f"the table allows. Move the number, in this commit"
                )
            continue
        if rel in FRENCH_DOCUMENTS or rel.startswith(BILINGUAL_DATA):
            continue
        english = [n for n, line in enumerate(lines, 1) if is_english(line)]
        if len(french) > 2 and len(english) > 2:
            failures.append(
                f"{rel}: {len(french)} French lines and {len(english)} English "
                f"lines in one document, first French line {french[0][0]}"
            )
    return failures, counts, read


def report(failures: list[str], read: int, listing: bool) -> int:
    if read == 0:
        print("nothing measured: no tracked file in reach", file=sys.stderr)
        return 2
    if not failures:
        print(f"file language: {read} files read, none mixes its languages")
        return 0
    shown = failures if listing else failures[:40]
    for line in shown:
        print(line)
    if len(shown) < len(failures):
        print(f"... {len(failures) - len(shown)} more (pass --list for all)")
    print(f"\n{len(failures)} language failure(s) over {read} files read")
    return 1


# ── Self-test ────────────────────────────────────────────────────────────────
# Both directions on every property, because a detector that answered "not
# French" to everything would satisfy the negative half of each pair.

FRENCH_LINES = (
    "// Garde d'inactivite: si aucune ligne SSE n'arrive pendant ce delai",
    '#[error("erreur SQLite : {0}")]',
    '.expect("la table doit exister apres la migration")',
    "# attraper les regressions en moins de 15 min de feedback",
    "  * Store des traces d'exécution event-sourced.",
    # No accent, and one French word the list had to gain: this exact shape
    # sat in production through the whole campaign because the line offered
    # the detector no French evidence at all.
    '    #[error("agent.toml invalide : {0}")]',
)

# Diagnostic messages the *line* rule cannot see, because the English code
# around them outvotes them. Only the message rule reaches these.
FRENCH_MESSAGE_LINES = (
    'if it is not there, return Err(E::X(format!("agent introuvable dans le registre")));',
    'let m = if ok { "fine" } else { format!("canal desktop indisponible : {id}") };',
)
ENGLISH_LINES = (
    "// Guard against inactivity: if no SSE line arrives within this delay",
    '#[error("sqlite error: {0}")]',
    '.expect("the table must exist after the migration")',
    "# catch regressions in under 15 min of feedback",
    "  * Event-sourced execution trace store.",
    # The case the sweep this guard replaces got wrong eighteen times: an
    # English sentence that quotes a French label.
    "// the `Dépannage` section of the catalogue is the one that moved",
    "// returns `Réglages` when the locale asks for it, and `Settings` otherwise",
    # The same case with double quotes, which is why the message rule is bound
    # to a message position rather than applied to every literal: applied to
    # every literal it named twelve of these on this tree.
    '/// Used by the desktop "Modifier les arguments" dialog when it reopens',
    '// THEN they differ - FR is "Lecture de {path}", not "Reading {path}"',
)


def selftest() -> int:
    failures: list[str] = []

    def case(name: str, ok: bool, detail: str) -> None:
        if ok:
            print(f"  ok    {name}")
        else:
            print(f"  FAIL  {name}")
            failures.append(f"{name}: {detail}")

    print("detector: both directions on the same lines")
    missed = [line for line in FRENCH_LINES if not is_french(line)]
    case(
        "every French line is recognised",
        not missed,
        f"{len(missed)} French line(s) read as not French: {missed!r}. A "
        f"detector that recognises nothing passes every tree",
    )
    blind = [line for line in FRENCH_MESSAGE_LINES if _line_is_french(line)]
    case(
        "the line rule alone is blind to a diluted message",
        not blind,
        f"{len(blind)} line(s) were already caught by the line rule, so the "
        f"message rule below would be green without measuring anything",
    )
    diluted = [line for line in FRENCH_MESSAGE_LINES if not is_french(line)]
    case(
        "a French message survives the English code around it",
        not diluted,
        f"{len(diluted)} French message(s) read as not French: {diluted!r}. "
        f"A message diluted by identifiers is how a French `#[error]` reaches "
        f"an operator with nothing having judged it",
    )
    invented = [line for line in ENGLISH_LINES if is_french(line)]
    case(
        "no English line is called French",
        not invented,
        f"{len(invented)} English line(s) read as French: {invented!r}. A "
        f"guard that fires on compliant content is a guard someone switches off",
    )
    case(
        "an English line is recognised as English",
        is_english("// the value is returned to the caller when it is not empty"),
        "the mixture rule counts the English side, so a detector blind to "
        "English would report every French document as single-language",
    )
    case(
        "a French line is not counted as English",
        not is_english("// la valeur est rendue a l'appelant quand elle existe"),
        "a line counted on both sides makes the mixture rule fire on a "
        "single-language file",
    )

    print("\nrules: a code file and a document do not fail the same way")
    root = REPO_ROOT
    case(
        "one French line fails a code file",
        len(measure(root, [])[0]) == 0
        and _fake_verdict("x.rs", ["fn a() {}", "// la ligne est en francais et elle compte"]),
        "a single French line in code has to fail: the allocation leaves no "
        "second language for it to mix with",
    )
    case(
        "an English code file passes",
        not _fake_verdict("x.rs", ["fn a() {}", "// the line is in English"]),
        "a compliant file was reported, so the rule above would be green "
        "because the guard fires on anything",
    )
    case(
        "a French-only document passes",
        not _fake_verdict("x.md", ["# Titre", "Cette page est en francais.", "Elle le reste."]),
        "`NEVER mix` is not `NEVER French`: a single-language document is "
        "compliant, and reporting it would push the rule behind an exclusion",
    )
    case(
        "a mixed document fails",
        _fake_verdict(
            "x.md",
            [
                "Cette page melange les deux langues.",
                "Elle contient des phrases en francais.",
                "Et aussi une troisieme phrase francaise ici.",
                "This page also carries English sentences.",
                "The mixture is what the rule refuses, not the language.",
                "That is the defect this rule exists to name.",
            ],
        ),
        "a document carrying both languages was passed, which is the rule of "
        "FORBIDDEN.md going unmeasured",
    )

    print("\nratchet: the allowance table fails in both directions")
    case(
        "a code file above its allowance fails",
        _fake_verdict("x.rs", ['let a = "café";', 'let b = "thé";'], allow={"x.rs": 1}),
        "a file that grew past its allowance was passed, so the table would "
        "record a maximum that keeps climbing",
    )
    case(
        "a code file below its allowance fails",
        _fake_verdict("x.rs", ['let a = "café";'], allow={"x.rs": 2}),
        "debt paid without the number coming down leaves an allowance nobody "
        "comes back to retire",
    )
    case(
        "a code file exactly at its allowance passes",
        not _fake_verdict("x.rs", ['let a = "café";'], allow={"x.rs": 1}),
        "the exact count was reported, so the two cases above would be green "
        "because the ratchet refuses everything",
    )

    print("\ninventory: the guard reads git, and says when it read nothing")
    empty_failures, _, empty_read = measure(REPO_ROOT, [])
    quiet = io.StringIO()
    with contextlib.redirect_stderr(quiet), contextlib.redirect_stdout(quiet):
        empty_code = report(empty_failures, empty_read, False)
    case(
        "an empty inventory is nothing measured, not a pass",
        empty_read == 0 and empty_code == 2,
        "an empty inventory reported a pass, which is a guard that examined "
        "nothing announcing success",
    )

    if failures:
        print(f"\n{len(failures)} self-test failure(s):\n", file=sys.stderr)
        for line in failures:
            print(f"  {line}\n", file=sys.stderr)
        return 1
    print(
        "\nall cases pass: the detector names both languages and neither "
        "invents the other, one French line fails a code file while a "
        "French-only document passes and a mixed one fails, the allowance "
        "table fails above and below its number, and an empty inventory is "
        "reported as nothing measured"
    )
    return 0


def _fake_verdict(name: str, lines: list[str], allow: dict[str, int] | None = None) -> bool:
    """Run the two rules over a subject held in memory. True when it fails."""
    import tempfile

    global FRENCH_DATA
    saved = FRENCH_DATA
    if allow is not None:
        FRENCH_DATA = allow
    try:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / name).write_text("\n".join(lines) + "\n", encoding="utf-8")
            failures, _, read = measure(root, [name])
            return read == 1 and bool(failures)
    finally:
        FRENCH_DATA = saved


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--list", action="store_true", help="print every failure")
    parser.add_argument("--selftest", action="store_true", help="run the self-test")
    args = parser.parse_args()

    if args.selftest:
        return selftest()

    files = tracked_files()
    if not files:
        print("nothing measured: `git ls-files` returned nothing", file=sys.stderr)
        return 2
    failures, _, read = measure(REPO_ROOT, files)
    return report(failures, read, args.list)


if __name__ == "__main__":
    sys.exit(main())
