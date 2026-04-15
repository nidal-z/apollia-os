"""review-assistant — Post-implementation verification assistant for Apollia OS.

Third and final link of the development pipeline (spec → dev → review). Takes a
TaskSpec and the current workspace diff, and verifies three dimensions:

- **Completeness**: are all required layers from the TaskSpec covered by the diff?
- **Conformity**: are project standards respected (forbidden dependencies, dangerous
  patterns, comment conventions)?
- **Test coverage**: do existing tests still pass, and are new functions covered?

Produces a structured 🟢/🟡/🔴 actionable report. Every 🔴 BLOQUANT item maps
to a concrete fix required before committing. Never modifies files without explicit
user confirmation.

Tools required  : file_read, bash_executor
Tools optional  : file_write, file_edit
LLM backend     : precise (for follow-up questions)
"""

from __future__ import annotations

import json
import re
import unicodedata
from datetime import date
from typing import Any

from apollia.agents import AIPResult, ConversationalAgent

from assistants.shared.project_bootstrap import ProjectContextBootstrap


# ---------------------------------------------------------------------------
# Report level constants
# ---------------------------------------------------------------------------

_LGTM = "🟢 LGTM"
_ATTENTION = "🟡 ATTENTION"
_BLOQUANT = "🔴 BLOQUANT"


# ---------------------------------------------------------------------------
# Memory keys
# ---------------------------------------------------------------------------

_MEMORY_SOURCE: str = "review-assistant"


# ---------------------------------------------------------------------------
# Bootstrap
# ---------------------------------------------------------------------------


class ReviewContextBootstrap(ProjectContextBootstrap):
    """Bootstrap for review-assistant.

    No extra scopes beyond the common project snapshot (workspace rules,
    tech stack, git state).  The base snapshot is sufficient for
    post-implementation verification.

    When another development-domain agent (e.g. dev-assistant) has already
    bootstrapped the same project in a shared namespace, the snapshot can
    be loaded directly without re-bootstrap.
    """


# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

_TASK_SPEC_DIR: str = ".apollia/tasks"

_SLUG_NON_ALNUM: re.Pattern[str] = re.compile(r"[^a-z0-9]+")

_SPEC_REF_RE: re.Pattern[str] = re.compile(
    r"(?:\.apollia/tasks/|taskspec\s+|spec(?:ification)?\s*[:\s]\s*)"
    r"([a-z0-9][a-z0-9\-]{0,62})(?:\.md)?",
    re.IGNORECASE,
)

_DIFF_FILE_RE: re.Pattern[str] = re.compile(r"^\+\+\+ b/(.+)$", re.MULTILINE)

_DIFF_ADDITION_RE: re.Pattern[str] = re.compile(r"^\+(?!\+\+)(.*)$", re.MULTILINE)

_CHECKED_LAYER_RE: re.Pattern[str] = re.compile(
    r"^-\s+\[x\]\s+(.+?)(?:\s+✅)?\s*$", re.MULTILINE | re.IGNORECASE
)


# ---------------------------------------------------------------------------
# Conformity patterns
# ---------------------------------------------------------------------------

# Patterns whose violations are always 🔴 BLOQUANT.
_BLOQUANT_PATTERNS: list[tuple[re.Pattern[str], str]] = [
    (re.compile(r"\.unwrap\(\)"), "unwrap() hors contexte de test"),
    (re.compile(r"\btodo!\(\)"), "todo!() en production"),
    (re.compile(r"\bpanic!\("), "panic!() en production"),
    (re.compile(r"\bas\s+any\b"), "cast `as any` TypeScript"),
    (re.compile(r"//\s*@ts-ignore"), "suppression de vérification TypeScript (@ts-ignore)"),
    (re.compile(r"//\s*@ts-nocheck"), "suppression de vérification TypeScript (@ts-nocheck)"),
]

# Labels of patterns that are acceptable inside test files.
_TEST_SAFE_PATTERN_LABELS: frozenset[str] = frozenset((
    "unwrap() hors contexte de test",
    "todo!() en production",
    "panic!() en production",
))

# Patterns whose violations are 🟡 ATTENTION (non-blocking).
_ATTENTION_PATTERNS: list[tuple[re.Pattern[str], str]] = [
    (re.compile(r"\*\s*@param\b"), "paramètre JSDoc auto-généré"),
    (re.compile(r"//\s*(?:Cette|This)\s+(?:function|méthode|method)\b", re.IGNORECASE), "commentaire évident"),
    (re.compile(r"//\s*TODO\b"), "TODO laissé dans le code"),
    (re.compile(r"//\s*FIXME\b"), "FIXME laissé dans le code"),
    (re.compile(r"\bconsole\.log\b"), "console.log laissé dans le code"),
    (re.compile(r"\bdbg!\("), "dbg!() Rust laissé dans le code"),
    (re.compile(r"\beprintln!\("), "eprintln!() Rust en production"),
]

# File path substrings that identify test files.
_TEST_FILE_INDICATORS: frozenset[str] = frozenset((
    "test_", "_test.", ".test.", ".spec.", "/tests/", "/test/",
    "_tests.", "test/", "tests/", "spec/",
))


# ---------------------------------------------------------------------------
# Layer → file-pattern mapping for completeness checks
# ---------------------------------------------------------------------------

# Maps a lowercased keyword (substring-matched against a layer name's parts)
# to a list of regex patterns expected in diff file paths for that layer.
_LAYER_KEYWORD_PATTERNS: dict[str, list[str]] = {
    "database": [r"\.sql$", r"migration", r"schema", r"model", r"/db/", r"repository"],
    "données": [r"\.sql$", r"migration", r"schema", r"model", r"/db/", r"repository"],
    "api": [r"route", r"controller", r"handler", r"/api/", r"router", r"endpoint"],
    "backend": [r"service", r"handler", r"usecase", r"repository", r"server"],
    "types": [r"type", r"interface", r"schema", r"contract"],
    "interfaces": [r"interface", r"contract", r"schema"],
    "contrats": [r"contract", r"interface", r"schema"],
    "logique": [r"domain", r"business", r"usecase", r"service", r"logic"],
    "métier": [r"domain", r"business", r"usecase", r"service"],
    "domain": [r"domain", r"entity", r"usecase", r"entities"],
    "frontend": [r"\.tsx?", r"\.jsx?", r"\.vue", r"\.svelte", r"component", r"page", r"view"],
    "ui": [r"\.tsx?", r"\.jsx?", r"component", r"style", r"\.css", r"\.scss"],
    "composants": [r"component", r"\.tsx?", r"\.jsx?", r"\.vue", r"\.svelte"],
    "câblage": [r"config", r"setup", r"init", r"bootstrap", r"wiring"],
    "configuration": [r"config", r"settings", r"\.env", r"\.toml$", r"\.yaml$", r"\.json$"],
    "intégration": [r"integration", r"e2e", r"config", r"setup"],
    "tests": [r"test", r"spec"],
    "unitaires": [r"test_", r"_test\.", r"\.test\.", r"_spec\.", r"\.spec\."],
    "documentation": [r"\.md$", r"doc", r"readme", r"changelog"],
    "guides": [r"\.md$", r"guide", r"tutorial"],
}

# Test runner detection — (workspace marker file, shell command).
_TEST_RUNNERS: tuple[tuple[str, str], ...] = (
    ("package.json", "npm test 2>&1"),
    ("Cargo.toml", "cargo test 2>&1"),
    ("pyproject.toml", "python -m pytest -x --tb=short 2>&1"),
    ("setup.py", "python -m pytest -x --tb=short 2>&1"),
    ("go.mod", "go test ./... 2>&1"),
)


# ---------------------------------------------------------------------------
# Language detection
# ---------------------------------------------------------------------------

_FRENCH_MARKERS: frozenset[str] = frozenset((
    "bonjour", "salut", "bonsoir", "coucou",
    "je", "nous", "vous", "il", "elle", "ils",
    "est", "sont", "avec", "pour", "dans", "sur", "par",
    "une", "un", "le", "la", "les", "des", "du",
    "mon", "ton", "son", "notre", "de",
    "revois", "vérifie", "analyse", "contrôle", "inspecte",
    "rapport", "résultat", "résultats", "problème", "erreur",
    "oui", "non", "merci", "svp", "stp",
))


def _detect_language(text: str) -> str:
    """Return ``"fr"`` when *text* is likely French, ``"en"`` otherwise.

    Uses whole-word token matching to avoid false positives from common
    substrings (e.g. ``"tu"`` appearing inside ``"feature"``).
    """
    tokens = set(re.split(r"\W+", text.lower()))
    hits = sum(1 for m in _FRENCH_MARKERS if m in tokens)
    return "fr" if hits >= 2 else "en"


# ---------------------------------------------------------------------------
# Slug helpers
# ---------------------------------------------------------------------------

def _slugify(title: str) -> str:
    """Convert *title* to a URL-safe lowercase ASCII slug (max 64 chars)."""
    normalized = unicodedata.normalize("NFD", title.lower())
    ascii_str = normalized.encode("ascii", "ignore").decode("ascii")
    slug = _SLUG_NON_ALNUM.sub("-", ascii_str).strip("-")
    return (slug or "review")[:64]


# ---------------------------------------------------------------------------
# File I/O helpers
# ---------------------------------------------------------------------------

async def _read_file(ctx: Any, path: str) -> str | None:
    """Read *path* via the ``file_read`` tool.

    Returns the file content on success, ``None`` when the file does not
    exist, the tool is unavailable, or an error is reported by the tool.
    """
    if ctx.tools is None:
        return None
    try:
        result = await ctx.tools.call("file_read", {"path": path})
        if not isinstance(result, dict):
            return None
        if result.get("error"):
            return None
        content = result.get("content", "")
        return content if content else None
    except Exception:
        return None


async def _run_bash(ctx: Any, cmd: str) -> str | None:
    """Execute *cmd* via ``bash_executor`` and return stdout.

    Returns ``None`` when bash_executor is unavailable or reports an error
    that produced no output.
    """
    if ctx.tools is None:
        return None
    try:
        available = ctx.tools.list_tools()
        if "bash_executor" not in available:
            return None
        result = await ctx.tools.call("bash_executor", {"cmd": cmd})
        if not isinstance(result, dict):
            return None
        stdout = result.get("stdout", result.get("output", "")) or ""
        return stdout if stdout else None
    except Exception:
        return None


# ---------------------------------------------------------------------------
# Project rules parsing
# ---------------------------------------------------------------------------

_MAX_RULES_CHARS: int = 4_000


def _parse_forbidden_deps(raw: str) -> list[str]:
    """Extract dependency names explicitly forbidden in *raw*."""
    patterns = [
        r"\b([a-zA-Z][\w\-]+)\b\W+INTERDIT\b",
        r"INTERDIT[^:]*:\s*([a-zA-Z][\w\-]+)",
        r"\b([a-zA-Z][\w\-]+)\b\W+interdit\b",
        r"forbidden[^:]*:\s*([a-zA-Z][\w\-]+)",
        r"\b([a-zA-Z][\w\-]+)\b\W+is\s+forbidden\b",
        r"\b([a-zA-Z][\w\-]+)\b\W+not\s+allowed\b",
    ]
    found: set[str] = set()
    for pat in patterns:
        for m in re.finditer(pat, raw):
            dep = m.group(1).strip()
            if len(dep) >= 2:
                found.add(dep)
    return sorted(found)


def _parse_forbidden_deps_json(raw_json: str) -> list[str]:
    """Safely deserialise a JSON list of forbidden dependency names."""
    try:
        result = json.loads(raw_json)
        if isinstance(result, list):
            return [str(dep) for dep in result if dep]
        return []
    except (json.JSONDecodeError, TypeError):
        return []


def _parse_rules(raw_text: str) -> dict[str, str]:
    """Parse *raw_text* from workspace rules and return categorised rules.

    Extracts forbidden dependencies and truncates the raw text to a
    manageable size for system prompt injection.
    """
    forbidden = _parse_forbidden_deps(raw_text)
    truncated = raw_text[:_MAX_RULES_CHARS]
    if len(raw_text) > _MAX_RULES_CHARS:
        truncated += "\n[… règles tronquées …]"
    return {
        "raw": truncated,
        "forbidden_deps": json.dumps(forbidden),
    }


# ---------------------------------------------------------------------------
# TaskSpec loading
# ---------------------------------------------------------------------------

def _extract_spec_slug(text: str) -> str | None:
    """Extract a TaskSpec slug from *text* if present."""
    match = _SPEC_REF_RE.search(text)
    if match:
        return match.group(1).lower()
    return None


async def _find_task_spec(ctx: Any, user_input: str) -> str | None:
    """Locate and return a TaskSpec for the current review session.

    Tries (1) an explicit slug from the user message, then (2) a file listing
    to find the most recently named spec. Returns ``None`` when no spec is found.
    """
    slug = _extract_spec_slug(user_input)
    if slug:
        return await _read_file(ctx, f"{_TASK_SPEC_DIR}/{slug}.md")

    if ctx.tools is None:
        return None
    try:
        available = ctx.tools.list_tools()
        if "file_list" not in available:
            return None
        result = await ctx.tools.call("file_list", {"path": _TASK_SPEC_DIR})
        if not isinstance(result, dict):
            return None
        files = result.get("files", result.get("entries", []))
        md_files = sorted(
            f if isinstance(f, str) else f.get("name", "")
            for f in files
            if (f if isinstance(f, str) else f.get("name", "")).endswith(".md")
        )
        if md_files:
            return await _read_file(ctx, f"{_TASK_SPEC_DIR}/{md_files[-1]}")
    except Exception:
        pass
    return None


def _extract_spec_title(spec_content: str | None) -> str:
    """Return the human-readable title from a TaskSpec, or ``"review"``."""
    if not spec_content:
        return "review"
    for line in spec_content.splitlines():
        stripped = line.strip()
        if stripped.startswith("# TaskSpec"):
            return stripped.lstrip("#").strip()
        if stripped.startswith("# "):
            return stripped.lstrip("#").strip()
    return "review"


def _extract_required_layers(spec_content: str) -> list[str]:
    """Return layers marked ``[x]`` (without ✅) in *spec_content*."""
    return [
        m.group(1).strip()
        for m in _CHECKED_LAYER_RE.finditer(spec_content)
        if "✅" not in m.group(0)
    ]


# ---------------------------------------------------------------------------
# Git diff retrieval
# ---------------------------------------------------------------------------

async def _get_diff(ctx: Any) -> str:
    """Return the current diff for the workspace.

    Tries staged+unstaged vs HEAD first, then the last commit's diff.
    Returns an empty string when bash_executor is unavailable.
    """
    diff = await _run_bash(ctx, "git diff HEAD 2>&1")
    if diff and diff.strip():
        return diff

    diff = await _run_bash(ctx, "git diff HEAD~1 2>&1")
    if diff and diff.strip():
        return diff

    return ""


# ---------------------------------------------------------------------------
# Diff parsing helpers
# ---------------------------------------------------------------------------

def _parse_diff_files(diff: str) -> list[str]:
    """Return the list of file paths modified in *diff*.

    Parses ``+++ b/path`` lines from standard unified diff output.
    """
    return _DIFF_FILE_RE.findall(diff)


def _parse_diff_additions(diff: str) -> list[str]:
    """Return lines added (``+`` prefix) in *diff*, excluding ``+++`` header lines."""
    return _DIFF_ADDITION_RE.findall(diff)


def _is_test_file(path: str) -> bool:
    """Return ``True`` when *path* indicates a test file."""
    lower = path.lower()
    return any(ind in lower for ind in _TEST_FILE_INDICATORS)


def _parse_diff_hunks(
    diff: str,
) -> list[tuple[str, bool, list[tuple[str, int]]]]:
    """Parse *diff* into per-file hunks with annotated additions.

    Returns a list of ``(file_path, is_test_file, additions)`` tuples where
    each addition is a ``(line_text, approximate_line_number)`` pair.
    """
    segments: list[tuple[str, bool, list[tuple[str, int]]]] = []
    current_file = ""
    current_is_test = False
    current_additions: list[tuple[str, int]] = []
    line_num = 0

    for raw_line in diff.splitlines():
        if raw_line.startswith("+++ b/"):
            if current_file:
                segments.append((current_file, current_is_test, current_additions))
            current_file = raw_line[6:]
            current_is_test = _is_test_file(current_file)
            current_additions = []
            line_num = 0
        elif raw_line.startswith("@@ "):
            m = re.search(r"\+(\d+)", raw_line)
            line_num = (int(m.group(1)) - 1) if m else 0
        elif raw_line.startswith("+") and not raw_line.startswith("+++"):
            line_num += 1
            current_additions.append((raw_line[1:], line_num))
        elif not raw_line.startswith("-"):
            line_num += 1

    if current_file:
        segments.append((current_file, current_is_test, current_additions))

    return segments


# ---------------------------------------------------------------------------
# Completeness check
# ---------------------------------------------------------------------------

def _layer_to_file_patterns(layer_name: str) -> list[str]:
    """Return file path regex patterns that indicate *layer_name* is covered.

    Extracts keywords from the layer name (split on ``/``, lowercased) and
    looks them up in ``_LAYER_KEYWORD_PATTERNS``.
    """
    parts = [p.strip().lower() for p in layer_name.split("/")]
    patterns: list[str] = []
    for part in parts:
        for kw, kw_patterns in _LAYER_KEYWORD_PATTERNS.items():
            if kw in part:
                patterns.extend(kw_patterns)
    return patterns


def _diff_covers_layer(layer_name: str, diff_files: list[str]) -> bool:
    """Return ``True`` when at least one file in *diff_files* satisfies *layer_name*.

    Returns ``True`` for unknown layers (no patterns mapped) to avoid false alarms.
    """
    patterns = _layer_to_file_patterns(layer_name)
    if not patterns:
        return True

    for file_path in diff_files:
        lower_path = file_path.lower()
        for pat in patterns:
            if re.search(pat, lower_path):
                return True
    return False


def _check_completeness(
    spec_content: str | None,
    diff: str,
) -> list[tuple[str, str]]:
    """Verify that every required layer in *spec_content* is covered by *diff*.

    Returns a list of ``(level, message)`` pairs using the ``_LGTM``,
    ``_ATTENTION``, and ``_BLOQUANT`` constants.
    """
    if not spec_content:
        return [(_ATTENTION, "Aucune TaskSpec trouvée — vérification de complétude ignorée")]

    layers = _extract_required_layers(spec_content)
    if not layers:
        return [(_ATTENTION, "Aucune couche cochée [x] dans la TaskSpec — rien à vérifier")]

    diff_files = _parse_diff_files(diff)
    if not diff_files:
        return [
            (_BLOQUANT, f"{layer} : diff vide, aucun fichier modifié")
            for layer in layers
        ]

    items: list[tuple[str, str]] = []
    for layer in layers:
        if _diff_covers_layer(layer, diff_files):
            items.append((_LGTM, f"{layer} : fichiers correspondants présents dans le diff"))
        else:
            items.append((_BLOQUANT, f"{layer} : aucun fichier correspondant dans le diff"))
    return items


# ---------------------------------------------------------------------------
# Conformity check
# ---------------------------------------------------------------------------

def _check_conformity(
    diff: str,
    rules: dict[str, str],
) -> list[tuple[str, str]]:
    """Scan *diff* additions for standard violations and forbidden dependencies.

    Checks each added line against ``_BLOQUANT_PATTERNS`` and
    ``_ATTENTION_PATTERNS``. Also checks for forbidden dependencies extracted
    from *rules*. Test files are exempt from test-safe patterns (``unwrap``,
    ``todo!``, ``panic!``).

    Returns a list of ``(level, message)`` pairs.
    """
    hunks = _parse_diff_hunks(diff)
    if not hunks:
        return [(_LGTM, "Aucune addition dans le diff — conformité OK")]

    items: list[tuple[str, str]] = []
    found_any_issue = False

    for file_path, is_test, additions in hunks:
        for line_text, line_num in additions:
            for pat, label in _BLOQUANT_PATTERNS:
                if label in _TEST_SAFE_PATTERN_LABELS and is_test:
                    continue
                if pat.search(line_text):
                    snippet = line_text.strip()[:80]
                    items.append(
                        (_BLOQUANT, f"{label} ({file_path}:{line_num}): `{snippet}`")
                    )
                    found_any_issue = True

            for pat, label in _ATTENTION_PATTERNS:
                if pat.search(line_text):
                    snippet = line_text.strip()[:80]
                    items.append(
                        (_ATTENTION, f"{label} ({file_path}:{line_num}): `{snippet}`")
                    )
                    found_any_issue = True

    forbidden_list = _parse_forbidden_deps_json(rules.get("forbidden_deps", "[]"))
    for dep in forbidden_list:
        dep_re = re.compile(
            r"""['"]""" + re.escape(dep) + r"""['"]"""
            r"""|use\s+""" + re.escape(dep.replace("-", "_")) + r"""\b"""
            r"""|extern\s+crate\s+""" + re.escape(dep.replace("-", "_")) + r"""\b""",
            re.IGNORECASE,
        )
        for file_path, _is_test, additions in hunks:
            for line_text, line_num in additions:
                if dep_re.search(line_text):
                    snippet = line_text.strip()[:80]
                    items.append(
                        (_BLOQUANT, f"Dépendance interdite `{dep}` ({file_path}:{line_num}): `{snippet}`")
                    )
                    found_any_issue = True

    if not found_any_issue:
        items.append((_LGTM, "Aucune dépendance interdite ni pattern dangereux détecté"))

    return items


# ---------------------------------------------------------------------------
# Test check
# ---------------------------------------------------------------------------

async def _detect_test_command(ctx: Any) -> str | None:
    """Return the test command appropriate for this workspace.

    Probes for known manifest files in order and returns the first match.
    Returns ``None`` when no recognized test runner is detected.
    """
    for workspace_file, command in _TEST_RUNNERS:
        content = await _read_file(ctx, workspace_file)
        if content is not None:
            return command
    return None


def _find_new_public_functions(additions: list[str]) -> list[str]:
    """Return names of new public functions or routes added in *additions*.

    Recognises patterns across TypeScript, Rust, Python, and Go. Express-style
    route registrations are also detected.
    """
    _fn_patterns: list[re.Pattern[str]] = [
        re.compile(r"^pub\s+(?:async\s+)?fn\s+(\w+)\s*\("),                          # Rust pub fn
        re.compile(r"^(?:async\s+)?fn\s+(\w+)\s*\("),                                # Rust fn
        re.compile(r"^(?:export\s+)?(?:async\s+)?function\s+(\w+)\s*[\(\[]"),        # JS/TS function
        re.compile(r"^(?:export\s+)?(?:async\s+)?def\s+(\w+)\s*\("),                 # Python def
        re.compile(r"^(?:export\s+)?(?:const|let)\s+(\w+)\s*=\s*(?:async\s+)?\("),  # TS arrow fn
        re.compile(r"(?:app|router)\.(get|post|put|delete|patch)\s*\("),              # Express routes
    ]
    found: list[str] = []
    seen: set[str] = set()
    for line in additions:
        stripped = line.strip()
        for pat in _fn_patterns:
            m = pat.match(stripped)
            if m:
                name = m.group(1) if m.lastindex and m.lastindex >= 1 else stripped[:40]
                if name not in seen:
                    found.append(name)
                    seen.add(name)
    return found


async def _check_tests(
    ctx: Any,
    diff: str,
) -> list[tuple[str, str]]:
    """Run the project's test suite and detect uncovered new functions.

    Returns a list of ``(level, message)`` pairs covering the test run result
    and any new functions without matching test files in the diff.
    """
    items: list[tuple[str, str]] = []
    additions = _parse_diff_additions(diff)

    test_cmd = await _detect_test_command(ctx)

    if test_cmd is None:
        items.append((_ATTENTION, "Aucun runner de tests détecté dans ce workspace"))
    else:
        output = await _run_bash(ctx, test_cmd)
        if output is None:
            items.append(
                (_ATTENTION, f"Impossible d'exécuter les tests (`{test_cmd}` indisponible ou erreur)")
            )
        else:
            lower_out = output.lower()
            has_failure = any(
                kw in lower_out
                for kw in ("failed", "error:", "failures:", "panicked", "test result: fail")
            )
            test_count_m = re.search(r"(\d+)\s+(?:passed|test)", output, re.IGNORECASE)
            test_count = int(test_count_m.group(1)) if test_count_m else 0

            if has_failure:
                failure_lines = [
                    line.strip()
                    for line in output.splitlines()
                    if any(k in line.lower() for k in ("fail", "error:", "panick", "assert"))
                ][:5]
                failure_summary = " | ".join(failure_lines) if failure_lines else output[:200]
                items.append((_BLOQUANT, f"Tests en échec — {failure_summary}"))
            elif test_count > 0:
                items.append((_LGTM, f"{test_count} tests passent"))
            else:
                items.append((_LGTM, "Tests exécutés sans échec détecté"))

    diff_files = _parse_diff_files(diff)
    has_test_files_in_diff = any(_is_test_file(f) for f in diff_files)
    new_fns = _find_new_public_functions(additions)

    if new_fns and not has_test_files_in_diff:
        fn_list = ", ".join(f"`{fn}`" for fn in new_fns[:5])
        suffix = f" (et {len(new_fns) - 5} autres)" if len(new_fns) > 5 else ""
        items.append(
            (_ATTENTION, f"Nouvelles fonctions sans test visible dans le diff : {fn_list}{suffix}")
        )
    elif new_fns and has_test_files_in_diff:
        items.append(
            (_LGTM, f"{len(new_fns)} nouvelle(s) fonction(s) — fichiers de tests présents dans le diff")
        )

    return items


# ---------------------------------------------------------------------------
# Report construction
# ---------------------------------------------------------------------------

def _format_section(title: str, items: list[tuple[str, str]]) -> str:
    """Format a named report section from *items*."""
    lines = [f"### {title}"]
    for level, message in items:
        lines.append(f"{level} — {message}")
    return "\n".join(lines)


def _build_report(
    spec_title: str,
    completeness: list[tuple[str, str]],
    conformity: list[tuple[str, str]],
    tests: list[tuple[str, str]],
    lang: str,
) -> str:
    """Assemble the final 🟢/🟡/🔴 report from the three check results.

    Counts blocking and attention items to produce an actionable summary footer.
    """
    today = date.today().isoformat()
    all_items = completeness + conformity + tests
    blocants = sum(1 for level, _ in all_items if level == _BLOQUANT)
    attentions = sum(1 for level, _ in all_items if level == _ATTENTION)

    if lang == "fr":
        comp_title = "Complétude des couches"
        conf_title = "Conformité aux standards"
        test_title = "Tests"
        summary_title = "Résumé"
        summary_lines = (
            [f"🔴 {blocants} blocant(s) à corriger avant merge"]
            if blocants > 0
            else ["🟢 Aucun blocant — prêt pour merge"]
        )
        if attentions > 0:
            summary_lines.append(f"🟡 {attentions} point(s) d'attention (non-bloquants)")
        if blocants == 0 and attentions == 0:
            summary_lines.append("🟢 Implémentation conforme aux standards du projet")
    else:
        comp_title = "Layer completeness"
        conf_title = "Standards conformity"
        test_title = "Tests"
        summary_title = "Summary"
        summary_lines = (
            [f"🔴 {blocants} blocking item(s) to fix before merge"]
            if blocants > 0
            else ["🟢 No blockers — ready to merge"]
        )
        if attentions > 0:
            summary_lines.append(f"🟡 {attentions} attention item(s) (non-blocking)")
        if blocants == 0 and attentions == 0:
            summary_lines.append("🟢 Implementation conforms to project standards")

    sections = [
        f"## Review — {spec_title} — {today}",
        "",
        _format_section(comp_title, completeness),
        "",
        _format_section(conf_title, conformity),
        "",
        _format_section(test_title, tests),
        "",
        f"### {summary_title}",
        *summary_lines,
    ]
    return "\n".join(sections)


# ---------------------------------------------------------------------------
# Task input extraction
# ---------------------------------------------------------------------------

def _extract_task_input(task: Any) -> tuple[str, list[dict[str, str]]]:
    """Extract user text and conversation history from *task*.

    Supports dicts (API format), objects with attributes (PyO3 runtime format),
    and multi-turn A2A parts.
    """
    task_input = (
        task.get("input") if isinstance(task, dict) else getattr(task, "input", None)
    )

    if task_input is None:
        return "", []

    if isinstance(task_input, dict):
        parts = task_input.get("parts", [])
        if parts and isinstance(parts[0], dict):
            input_text: str = parts[0].get("text", "")
        else:
            input_text = str(task_input.get("text", ""))
    elif hasattr(task_input, "parts"):
        parts = task_input.parts
        input_text = parts[0].text if parts else str(task_input)
    elif hasattr(task_input, "text"):
        input_text = task_input.text
    else:
        input_text = str(task_input)

    raw_history = (
        task.get("history", []) if isinstance(task, dict)
        else getattr(task, "history", [])
    )
    history: list[dict[str, str]] = []
    for msg in raw_history or []:
        if isinstance(msg, dict):
            role_raw = msg.get("role", "user")
            role = "assistant" if role_raw == "agent" else role_raw
            msg_parts = msg.get("parts", [])
            text = (
                msg_parts[0]["text"]
                if msg_parts and isinstance(msg_parts[0], dict)
                else str(msg.get("text", msg))
            )
            history.append({"role": role, "content": text})
        elif hasattr(msg, "role"):
            role = "assistant" if msg.role == "agent" else msg.role
            msg_parts = getattr(msg, "parts", [])
            text = msg_parts[0].text if msg_parts else str(msg)
            history.append({"role": role, "content": text})

    return input_text, history


# ---------------------------------------------------------------------------
# System prompts
# ---------------------------------------------------------------------------

_SYSTEM_PROMPT_FR: str = """\
Tu es **review-assistant**, assistant de vérification du pipeline Apollia OS \
(spec → dev → **review**).

Tu analyses le diff courant et la TaskSpec correspondante pour détecter les \
problèmes avant le merge. Tu produis un rapport 🟢/🟡/🔴 structuré et actionnable.

## Contraintes absolues

1. **Tu ne modifies JAMAIS un fichier sans demande explicite de l'utilisateur.**
2. Si tu proposes d'écrire des tests manquants, tu demandes confirmation d'abord.
3. Réponds dans la même langue que l'utilisateur (FR/EN auto-détecté).
4. Sois factuel et concis — chaque item du rapport doit être vérifiable.

## Niveaux du rapport

- 🔴 BLOQUANT : doit être corrigé avant tout merge
- 🟡 ATTENTION : non-bloquant mais recommandé
- 🟢 LGTM : conforme, pas d'action requise\
"""

_SYSTEM_PROMPT_EN: str = """\
You are **review-assistant**, the verification assistant of the Apollia OS pipeline \
(spec → dev → **review**).

You analyse the current diff and the matching TaskSpec to detect issues before merge. \
You produce a structured, actionable 🟢/🟡/🔴 report.

## Absolute constraints

1. **You NEVER modify a file without explicit user confirmation.**
2. If you propose writing missing tests, you ask for confirmation first.
3. Respond in the user's detected language (FR/EN auto-detected).
4. Be factual and concise — every report item must be verifiable.

## Report levels

- 🔴 BLOQUANT: must be fixed before any merge
- 🟡 ATTENTION: non-blocking but recommended
- 🟢 LGTM: compliant, no action required\
"""


# ---------------------------------------------------------------------------
# Module-level manifest function (AIP contract)
# ---------------------------------------------------------------------------

def manifest() -> dict[str, Any]:
    """Return the AIP manifest of review-assistant."""
    return {
        "name": "review-assistant",
        "version": "1.0.0",
        "description": (
            "Assistant de vérification Apollia OS — analyse le diff courant et la TaskSpec "
            "pour détecter les couches manquantes, violations de standards et tests absents. "
            "Produit un rapport 🟢/🟡/🔴 actionnable. Ne modifie aucun fichier sans confirmation. "
            "Troisième maillon du pipeline spec → dev → review."
        ),
        "execution_mode": "auto",
        "agent_type": "assistant",
        "tools_required": ["file_read", "bash_executor"],
        "tools_optional": ["file_write", "file_edit"],
        "tools_requiring_approval": ["bash_executor"],
        "packages": [],
        "memory_namespace": "review-assistant",
        "llm_backend": "precise",
        "supports_streaming": True,
        "supports_a2a": True,
        "step_budget": {"max_steps": 50, "max_tool_calls": 40, "wall_clock_secs": 600},
        "tags": ["dev", "review", "verification", "pipeline"],
        "max_concurrent_tasks": 1,
        "dangerous_tools_allowed": False,
        "examples": [
            "Vérifie l'implémentation par rapport à la spec user-auth",
            "Analyse le diff actuel et liste les tests manquants",
            "Cherche les violations des standards Apollia dans ce commit",
            "Le code de la PR respecte-t-il les principes du projet ?",
            "Mode pre-commit : vérifie avant que git-worker committe",
        ],
        "limitations": [
            "Ne modifie aucun fichier sans confirmation explicite de l'utilisateur",
            "Évalue la conformité aux standards projet, pas la pertinence fonctionnelle ou métier",
            "Nécessite bash_executor pour lire le diff git — non disponible hors runtime Apollia",
            "Ne bloque jamais sur un style personnel — uniquement sur des violations de règles "
            "projet explicites",
            "Si aucune TaskSpec n'est trouvée, signale l'absence et continue sur le diff seul",
        ],
        "setup_notes": (
            "Nécessite que bash_executor soit disponible pour accéder au diff git. "
            "Vérifie trois dimensions : complétude (toutes les couches TaskSpec couvertes ?), "
            "conformité (standards du projet respectés ?) et tests (couverture suffisante ?). "
            "Produit un rapport structuré 🟢 LGTM / 🟡 ATTENTION / 🔴 BLOQUANT. "
            "Fonctionne sur n'importe quelle codebase sans configuration préalable. "
            "Une TaskSpec dans .apollia/tasks/ améliore la précision de la vérification "
            "de complétude, mais n'est pas obligatoire."
        ),
        "skills": [
            {
                "id": "review",
                "name": "Vérifier une implémentation",
                "description": (
                    "Analyse le diff courant et la TaskSpec pour détecter les couches "
                    "manquantes, les violations de standards et les tests absents. "
                    "Produit un rapport 🟢/🟡/🔴 actionnable."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "request": {
                        "type": "string",
                        "description": "Description de la review à effectuer ou slug de TaskSpec",
                        "required": True,
                    },
                    "spec_slug": {
                        "type": "string",
                        "description": "Slug de la TaskSpec à vérifier (ex. : user-auth)",
                        "required": False,
                    },
                },
            },
        ],
    }


# ---------------------------------------------------------------------------
# Agent
# ---------------------------------------------------------------------------

class ReviewAssistant(ConversationalAgent):
    """Post-implementation verification assistant — third link of the dev pipeline.

    Analyses the current git diff against a TaskSpec to detect completeness gaps,
    standard violations, and missing tests. Produces an actionable 🟢/🟡/🔴
    report without modifying any file unless explicitly requested.

    Uses ``ReviewContextBootstrap`` to load project context (workspace rules,
    tech stack, git state) from semantic memory. On session N+1, the snapshot
    is reloaded without re-reading workspace files.

    Workflow per ``run()`` call:

    1. Bootstrap project context (first turn) or reload snapshot.
    2. Locate the TaskSpec from the user message or ``.apollia/tasks/``.
    3. Retrieve the current diff via ``bash_executor``.
    4. Check completeness: every ``[x]`` layer must have matching files in the diff.
    5. Check conformity: scan additions for forbidden patterns and dependencies.
    6. Check tests: run the test suite, detect new uncovered functions.
    7. Return the assembled report.
    """

    SYSTEM_PROMPT: str = _SYSTEM_PROMPT_FR
    MAX_TURNS: int = 20
    TEMPERATURE: float = 0.1

    def __init__(self) -> None:
        self._bootstrap = ReviewContextBootstrap()

    def manifest(self) -> dict[str, Any]:
        """Return the AIP manifest of review-assistant."""
        return manifest()

    async def run(self, task: Any, ctx: Any) -> dict[str, Any]:
        """Execute one review turn for *task*.

        Bootstraps project context on the first turn, retrieves the diff,
        runs all three checks, and returns the assembled 🟢/🟡/🔴 report.
        """
        input_text, history = _extract_task_input(task)

        if not input_text:
            return AIPResult.failed("NO_INPUT", "No input provided in task")

        lang = _detect_language(input_text)
        is_first_turn = not history

        if is_first_turn:
            self._current_lang: str = lang

            if await self._bootstrap.needs_bootstrap(ctx):
                await self._bootstrap.run_bootstrap(ctx)
            snapshot = await self._bootstrap.load_snapshot(ctx)

            raw_rules = (
                snapshot.get("workspace_rules", "")
                if snapshot
                else (
                    ctx.workspace.rules or ""
                    if getattr(ctx, "workspace", None) is not None
                    else ""
                )
            )
            self._rules: dict[str, str] = (
                _parse_rules(raw_rules)
                if raw_rules
                else {"raw": "", "forbidden_deps": "[]"}
            )
        else:
            lang = getattr(self, "_current_lang", lang)

        rules = getattr(self, "_rules", {"raw": "", "forbidden_deps": "[]"})

        self.SYSTEM_PROMPT = _SYSTEM_PROMPT_FR if lang == "fr" else _SYSTEM_PROMPT_EN

        spec_content = await _find_task_spec(ctx, input_text)
        diff = await _get_diff(ctx)

        completeness = _check_completeness(spec_content, diff)
        conformity = _check_conformity(diff, rules)
        tests = await _check_tests(ctx, diff)

        spec_title = _extract_spec_title(spec_content)
        report = _build_report(spec_title, completeness, conformity, tests, lang)

        if ctx.memory is not None:
            await ctx.memory.record(
                content=f"Review: {spec_title} — {date.today().isoformat()}",
                importance=0.6,
            )

        return AIPResult.completed(report)


# ---------------------------------------------------------------------------
# Module-level agent instance (AIP contract)
# ---------------------------------------------------------------------------

agent = ReviewAssistant()
