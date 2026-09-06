#!/usr/bin/env python3
"""Recompute, from the code, the inventory of product capabilities and their
coverage.

This is the axis the rest of the test instrumentation hangs from. It is not a
checked-in table: every number below is enumerated from an artefact of this
working tree, so a capability added today enters the denominator today and a
capability removed leaves it, without anyone editing a list.

Five surfaces, each with its own enumerator:

  cli         leaves of the clap tree, walked from `target/debug/apollia-os
              --help` recursively (`check_cli_e2e_coverage.enumerate_leaves`).
  desktop     gestures, that is the `data-testid` anchors rendered by an
              interactive element. The anchor corpus comes from
              `scripts/automation/tools/validate.build_corpus`, the repo's own
              resolver; this module only classifies what that corpus resolves.
  api         the `operationId` values of `clients/openapi.json`.
  tools       the native tools registered by
              `crates/apollia-tools/src/native_dispatcher.rs`.
  connectors  the operations declared by the connector families.

Three columns per capability, and the definitions are the point:

  unit        the capability's name appears inside a `#[cfg(test)]` region of a
              module that IMPLEMENTS it, not anywhere in the tree. The loose
              rule ("the name appears in some test") reported 18 connector
              operations covered; all 18 came from one enumeration array in a
              scopes test that executes nothing. Under the strict rule the
              answer is 0, which is the true one.
  e2e         an instrument EXECUTES the capability: a CLI track invocation, an
              automation script action step, an eval suite. A mention, a label,
              a comment or a doc-comment counts for nothing.
  dead        no production caller, so the capability is out of the
              denominator: neither covered nor a hole. Derived by crossing
              `check_optional_builders.BASELINE`, not hardcoded here.

Exit codes, and the 2 is the one that matters:

  0  every denominator was measured and no inconsistency was found
  1  an inconsistency: a duplicate id, a covered count above its total, or a
     capability declared dead that an instrument nevertheless executes
  2  nothing measured on at least one denominator (binary absent or stale,
     empty corpus, missing OpenAPI document). A 2 must never read as a pass.

Usage:
    python3 scripts/capability_inventory.py [--bin PATH] [--surface NAME]
    python3 scripts/capability_inventory.py --json
    python3 scripts/capability_inventory.py --selftest
"""

# REASON: print-call: this module is a CLI entry point; print() is its
# user-facing output channel, the same carve-out the Rust rule gives
# apollia-cli.

import argparse
import glob
import importlib.util
import json
import os
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO_ROOT / "scripts"))

import binary_freshness  # noqa: E402
import check_cli_e2e_coverage as cli_cov  # noqa: E402
import check_optional_builders  # noqa: E402


def load_validator():
    """Load the automation validator by path, as the other guards that read it do.

    It is loaded rather than imported because `scripts/automation/tools` is not
    an importable package: a bare `import validate` reads as a third-party
    distribution to `check_ci_workflows.py`, which then asks for a wheel that
    does not exist.
    """
    path = REPO_ROOT / "scripts/automation/tools/validate.py"
    spec = importlib.util.spec_from_file_location("automation_validate", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_gesture_classifier():
    """Load the desktop gesture classifier, by path and for the same reason.

    The walk that decides gesture against state marker belongs to
    `scripts/automation/tools/uncovered.py`, which owns the desktop corpus and
    already resolves component forwarding, prop spreads and composed suffixes.
    It was once written a second time in this file, and the two answers
    disagreed, 1388 against 538. Two answers to one question, with nothing to
    say which is the tree's, is the shape of defect this inventory exists to
    catch, so it does not get to carry one of its own.
    """
    path = REPO_ROOT / "scripts/automation/tools/uncovered.py"
    spec = importlib.util.spec_from_file_location("automation_uncovered", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


automation = load_validator()
uncovered = load_gesture_classifier()

DEFAULT_BIN = REPO_ROOT / "target/debug/apollia-os"
TRACKS_DIR = REPO_ROOT / "tests/cli/tracks"
AUTOMATION_DIR = REPO_ROOT / "scripts/automation"
OPENAPI = REPO_ROOT / "clients/openapi.json"
CLI_COMMANDS = REPO_ROOT / "crates/apollia-cli/src/commands"
API_DIR = REPO_ROOT / "crates/apollia-runtime/src/api"
TOOLS_DIR = REPO_ROOT / "crates/apollia-tools/src/tools"
DISPATCHER = REPO_ROOT / "crates/apollia-tools/src/native_dispatcher.rs"
CONNECTOR_SPECS = (
    REPO_ROOT / "crates/apollia-connectors/src/google/mod.rs",
    REPO_ROOT / "crates/apollia-connectors/src/microsoft/mod.rs",
)
CONNECTOR_BRIDGE = REPO_ROOT / "crates/apollia-runtime/src/connectors_bridge"
UI_ROOT = REPO_ROOT / "crates/apollia-desktop/ui/src"
EVAL_SUITES = REPO_ROOT / "evals"

SURFACES = ("cli", "desktop", "api", "tools", "connectors")

# Automation step kinds that ACT on an element. `waitFor`, `expect`,
# `captureText` and `waitGone` observe one; they exercise a state marker, not a
# gesture, and counting them was how an observed anchor became an exercised one.
ACTION_KINDS = frozenset({"click", "fill", "setChecked", "selectOption", "press"})

# Of those, the kinds that reach ONLY the element the anchor names. The runner
# descends into a wrapper for the others, deliberately and by documented
# design: `fillEl` says so in a comment before doing it, and `setChecked` names
# the case in its own error message, "not a checkbox, switch, or wrapper of
# one". `click` calls `el.click()` and stops there.
#
# The distinction is what makes the cross-check below mean anything. Without it
# an anchor on a wrapper reached by `fill` reads exactly like an anchor on a
# wrapper that a `click` never activates, and the second is a step that passes
# while doing nothing. Reported together, three legitimate sites hid one real
# defect, and the report was believed before the runner was read to the end.
NON_DESCENDING_KINDS = frozenset({"click", "press"})

# HTML elements a user acts on directly.
INTERACTIVE_TAGS = frozenset({"button", "input", "select", "textarea", "a", "summary", "option"})
HANDLER = re.compile(
    r"\bon(?:click|change|input|keydown|keyup|submit|mousedown|pointerdown|toggle)\s*="
    r"|\bon:(?:click|change|input|keydown|submit)\b"
)
ROLE = re.compile(
    r'role\s*=\s*["\'](?:button|tab|menuitem|menuitemcheckbox|switch|checkbox|radio'
    r'|link|option|combobox|slider|textbox|searchbox|spinbutton)["\']'
)
TESTID_ATTR = re.compile(r"(?:data-testid|dataTestId|testId|testid)\s*[=:]\s*")
# A builder verdict that says the capability has no production caller.
UNWIRED = re.compile(r"^(?:defect|unwired|dead\b)", re.I)


# ─── Rust source helpers ─────────────────────────────────────────────────────


def split_rust(src: str) -> tuple[str, str]:
    """Split a Rust source into (production text, test text).

    The split is by brace balance from each `#[cfg(test)]`, not by proximity:
    matching an attribute by how near it sits drops production code from the
    sweep, which is the failure `check_panic_free.py` records.
    """
    prod: list[str] = []
    tests: list[str] = []
    i = 0
    for m in re.finditer(r"#\[cfg\(test\)\]", src):
        if m.start() < i:
            continue
        k = src.find("{", m.end())
        if k < 0:
            continue
        depth = 0
        j = k
        while j < len(src):
            if src[j] == "{":
                depth += 1
            elif src[j] == "}":
                depth -= 1
                if depth == 0:
                    break
            j += 1
        prod.append(src[i : m.start()])
        tests.append(src[m.start() : j + 1])
        i = j + 1
    prod.append(src[i:])
    return "".join(prod), "".join(tests)


def rust_sources(root: Path) -> dict[str, tuple[str, str]]:
    """Every `.rs` under *root*, already split into production and test text."""
    out = {}
    for path in sorted(glob.glob(f"{root}/**/*.rs", recursive=True)):
        try:
            out[path] = split_rust(Path(path).read_text(encoding="utf-8"))
        except OSError:
            continue
    return out


def read(path) -> str:
    try:
        return Path(path).read_text(encoding="utf-8")
    except OSError:
        return ""


def pascal(token: str) -> str:
    return "".join(part.capitalize() for part in re.split(r"[-_]", token))


# ─── Surface: CLI ────────────────────────────────────────────────────────────


def cli_scope(noun: str, commands_dir: Path) -> list[str]:
    """The modules that implement the subtree of a top-level CLI noun."""
    stem = noun.replace("-", "_")
    files: list[str] = []
    direct = commands_dir / f"{stem}.rs"
    if direct.exists():
        files.append(str(direct))
    folder = commands_dir / stem
    if folder.is_dir():
        files += sorted(glob.glob(f"{folder}/**/*.rs", recursive=True))
    if files:
        return files
    # `do` lives in `do_cmd.rs`, `plan` in `plan_cache.rs`: a reserved word or a
    # longer stem, never a different subtree.
    return sorted(
        str(p)
        for p in commands_dir.glob(f"{stem}_*.rs")
    )


def extract_cli(bin_path: Path, tracks_dir: Path, commands_dir: Path) -> dict:
    if not bin_path.exists():
        return unmeasured("cli", f"{bin_path} is absent; build it with "
                                 "cargo build -p apollia-cli --bin apollia-os")
    # `require` prints the binary's provenance on `report`, whose default is
    # bound to stdout at import time; naming stderr keeps `--json` a document
    # and nothing else.
    stale = binary_freshness.require(bin_path, REPO_ROOT, report=sys.stderr)
    if stale is not None:
        return unmeasured("cli", f"{bin_path} was not produced by this tree")
    leaves = cli_cov.enumerate_leaves(str(bin_path))
    if not leaves:
        return unmeasured("cli", "the --help walk enumerated no leaf")

    tracks = sorted(tracks_dir.glob("*.sh"))
    per_track = {t.name: cli_cov.track_invocations(read(t)) for t in tracks}
    hits = cli_cov.classify(leaves, per_track) if tracks else {}

    cache: dict[str, list[str]] = {}
    caps = []
    for leaf in leaves:
        noun = leaf[0]
        if noun not in cache:
            cache[noun] = cli_scope(noun, commands_dir)
        files = cache[noun]
        variant = re.compile(r"\w*Command::" + pascal(leaf[-1]) + r"\b")
        unit = any(variant.search(split_rust(read(f))[1]) for f in files)
        name = " ".join(leaf)
        caps.append(
            {
                "id": name,
                "implemented_in": files,
                "unit": unit if files else False,
                "e2e": bool(hits.get(name)),
            }
        )
    nodes = {tuple(leaf[:i]) for leaf in leaves for i in range(1, len(leaf))}
    return {
        "surface": "cli",
        "measured": True,
        "capabilities": caps,
        "unit_measured": True,
        "e2e_measured": bool(tracks),
        "e2e_instrument": f"{len(tracks)} track(s) under tests/cli/tracks",
        "notes": {
            "intermediate_nodes": len(nodes) + 1,
            "unresolved_modules": sorted(
                {c["id"].split()[0] for c in caps if not c["implemented_in"]}
            ),
        },
    }


# ─── Surface: desktop ────────────────────────────────────────────────────────


def testid_sites(txt: str):
    """(tag, attribute text, offset after the attribute) per testid site."""
    out = []
    for m in TESTID_ATTR.finditer(txt):
        k = txt.rfind("<", 0, m.start())
        tag = None
        attrs = ""
        if k >= 0:
            tm = re.match(r"<([A-Za-z][\w.\-]*)", txt[k:])
            if tm:
                tag = tm.group(1)
                depth = 0
                j = k
                while j < len(txt):
                    c = txt[j]
                    if c == "{":
                        depth += 1
                    elif c == "}":
                        depth -= 1
                    elif c == ">" and depth == 0:
                        break
                    j += 1
                attrs = txt[k:j]
        out.append((tag, attrs, m.end()))
    return out


def props_of(txt: str) -> set[str]:
    props: set[str] = set()
    for pm in automation.PROPS_BLOCK.finditer(txt):
        props.update(re.findall(r"([A-Za-z_$][\w$]*)\s*(?:[,}]|=)", pm.group(1)))
        props.update(re.findall(r":\s*([A-Za-z_$][\w$]*)", pm.group(1)))
    return props


def classify_anchors(ui_root: Path):
    """Split the resolvable anchor corpus into gestures and state markers.

    The walk lives in `scripts/automation/tools/uncovered.py` and is consumed
    here rather than repeated: see load_gesture_classifier for why.

    Returns the shape the rest of this module expects: the resolvable corpus,
    the gesture subset, the files each anchor is written in, and the dynamic
    prefix families.
    """
    sites, carriers = uncovered.scan_sites(str(ui_root))
    components = uncovered.resolve_components(carriers)
    anchors, families, _unresolved = uncovered.anchors_from_sites(sites, components)

    corpus = set(anchors)
    # Three origins, not equally certain (see the classifier's own header). A
    # literal id renders, and so does a suffix a component appends to it. A
    # derived id is rebuilt from every string literal of its file and
    # over-generates by construction: measured on this tree, the danger page
    # holds four actions and the reconstruction names a hundred anchors for
    # them, `rounded-xl-confirm-input` among them. Counting those as holes
    # makes zero unreachable by construction, so they stay out of the
    # denominator until a recipe acts on one, which is the evidence that it
    # renders.
    gestures = {
        a for a, meta in anchors.items()
        if meta.get("kind") == "gesture" and meta.get("origin") in ("literal", "composed")
    }
    derived = {
        a for a, meta in anchors.items()
        if meta.get("kind") == "gesture" and meta.get("origin") == "derived"
    }
    owner: dict[str, set[str]] = {}
    for anchor, meta in anchors.items():
        if meta.get("file"):
            owner.setdefault(anchor, set()).add(meta["file"])
    return corpus, gestures, owner, families, derived
def automation_actions(scripts_dir: Path, kinds: frozenset[str] = ACTION_KINDS) -> set[str]:
    """Anchors an automation step acts on, by exact id, for the given kinds."""
    acted: set[str] = set()
    for path in sorted(scripts_dir.glob("*.json")):
        try:
            doc = json.loads(read(path))
        except json.JSONDecodeError:
            continue
        for step in doc.get("steps", []):
            if step.get("kind") in kinds and step.get("testid"):
                acted.add(step["testid"])
            # `sendChat` is the runner filling the composer and pressing its
            # send button, the two anchors `uncovered.py` credits it with.
            elif step.get("kind") == "sendChat" and "click" in kinds:
                acted.update(("chat-input", "chat-send-button"))
    return acted



def renders_components(ui_root: Path) -> bool:
    """Whether this tree's test style can reach a rendered component at all.

    Read from the manifest rather than from the test bodies: a grep for
    `render(` matches a locally defined helper, and one does exist here,
    rendering a schedule label rather than a component. A component-rendering
    library is a declared dependency or it is nothing.
    """
    manifest = ui_root / "package.json"
    if not manifest.is_file():
        return False
    try:
        declared = json.loads(manifest.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return False
    names = set(declared.get("dependencies") or {}) | set(declared.get("devDependencies") or {})
    return any(n.startswith("@testing-library/") for n in names)

def extract_desktop(ui_root: Path, scripts_dir: Path) -> dict:
    if not ui_root.is_dir():
        return unmeasured("desktop", f"{ui_root} is absent, so no anchor was resolved")
    corpus, gestures, owner, _, derived = classify_anchors(ui_root)
    if not corpus:
        return unmeasured("desktop", "the anchor corpus is empty")
    scripts = sorted(scripts_dir.glob("*.json")) if scripts_dir.is_dir() else []
    acted = automation_actions(scripts_dir) if scripts else set()
    clicked = automation_actions(scripts_dir, NON_DESCENDING_KINDS) if scripts else set()
    # A reconstructed anchor a step acts on is one a run has reached: it is
    # counted, and covered, on that evidence alone.
    promoted = derived & acted
    unverified = derived - acted
    gestures = gestures | promoted

    # A gesture is a DOM anchor, so a unit test can only reach it by rendering
    # the component and querying that anchor. This tree's vitest corpus does not
    # render: measured on it, `@testing-library` appears in no dependency of
    # package.json and one file of 125 calls `render(`. The 1100 tests it runs
    # cover logic and catalogues, which is a legitimate style and simply not one
    # a gesture can appear in.
    #
    # So the column is declared not applicable rather than answered with a small
    # number. Six of 804 reads as a failure and invites someone to write 798
    # tests that would assert a testid string against itself, which is the exact
    # shape of the mention-counting this axis exists to refuse.
    unit_pool: dict[str, str] = {}
    for path in glob.glob(f"{ui_root}/**/*.test.ts", recursive=True):
        unit_pool[path] = read(path)
    renders = renders_components(ui_root)

    caps = []
    for anchor in sorted(gestures):
        homes = sorted(owner.get(anchor, ()))
        dirs = {os.path.dirname(h) for h in homes}
        unit = any(
            os.path.dirname(p) in dirs and f'"{anchor}"' in txt
            for p, txt in unit_pool.items()
        )
        caps.append(
            {
                "id": anchor,
                "implemented_in": homes,
                "unit": unit,
                "e2e": anchor in acted,
            }
        )
    return {
        "surface": "desktop",
        "measured": True,
        "capabilities": caps,
        # See the comment above unit_pool: without a rendering test style there
        # is nothing here for a unit column to measure.
        "unit_measured": bool(unit_pool) and renders > 0,
        "unit_reason": (
            None
            if renders
            else (
                f"{len(unit_pool)} vitest file(s), and the manifest declares no "
                f"component-rendering library, so no test can query a gesture by its "
                f"anchor. The column is not applicable rather than nearly empty"
            )
        ),
        "e2e_measured": bool(scripts),
        "e2e_instrument": f"{len(scripts)} automation script(s)",
        "notes": {
            "addressable_anchors": len(corpus),
            "state_markers": len(corpus) - len(gestures),
            "acted_in_corpus": len(acted & corpus),
            # Only the non-descending kinds: a marker reached by `fill` or
            # `setChecked` is the runner doing what it says it does.
            "acted_but_classified_marker": sorted((clicked & corpus) - gestures),
            "reached_through_a_wrapper": sorted(((acted - clicked) & corpus) - gestures),
            "reconstructed_promoted_by_evidence": sorted(promoted),
            "reconstructed_unverified": len(unverified),
        },
    }


# ─── Surface: API ────────────────────────────────────────────────────────────


def normalise_path(raw: str) -> str:
    return re.sub(r"\{[^}]*\}", "{}", raw.split("?")[0].rstrip("/"))


def cli_called_paths(cli_root: Path) -> set[str]:
    """API paths the CLI really requests: a quoted literal in production code.

    A path written inside a `///` line is not a call. That bias put three
    operations on the reachable side of a hand count, `list_a2a_agents` among
    them, whose only CLI mention is a stale doc comment.
    """
    paths: set[str] = set()
    for path, (prod, _tests) in rust_sources(cli_root).items():
        prod = re.sub(r"^\s*//.*$", "", prod, flags=re.M)
        for m in re.finditer(r'"(/api/v1[^"]*)"', prod):
            paths.add(normalise_path(m.group(1)))
    return paths



# The operations no CLI leaf addresses carry their own integration probes, in
# `crates/apollia-runtime/src/api/unreached_by_cli.rs`. Its table declares an
# `id` per entry and its own doc-comment calls it "the name the coverage table
# counts": the intent was always that this axis read it. Until it did, eleven
# probes written against the real router counted for nothing here, and a table
# that ignores the work done against it teaches people to stop doing the work.
#
# The criterion stays execution: each id in that table is backed by a routing
# probe that sends a method the route refuses and expects the 405 axum only
# answers once the path has matched, and ten of the eleven by a live probe that
# enters the handler. A name in a comment would not appear in this table.
PROBE_TABLE = REPO_ROOT / "crates/apollia-runtime/src/api/unreached_by_cli.rs"


def router_probed_ops() -> set[str]:
    """Operation ids an integration probe exercises against the real router."""
    if not PROBE_TABLE.is_file():
        return set()
    body = PROBE_TABLE.read_text(encoding="utf-8", errors="ignore")
    return set(re.findall(r'\bid:\s*"([a-z0-9_]+)"', body))

def extract_api(openapi: Path, api_dir: Path, cli_root: Path) -> dict:
    if not openapi.exists():
        return unmeasured("api", f"{openapi} is absent, so no operation was enumerated")
    try:
        spec = json.loads(read(openapi))
    except json.JSONDecodeError as exc:
        return unmeasured("api", f"{openapi} does not parse: {exc}")
    ops = {}
    for path, methods in spec.get("paths", {}).items():
        for method, body in methods.items():
            if isinstance(body, dict) and "operationId" in body:
                ops[body["operationId"]] = (method.lower(), normalise_path(path))
    if not ops:
        return unmeasured("api", "the document declares no operationId")

    sources = rust_sources(api_dir)
    reachable = cli_called_paths(cli_root)
    probed = router_probed_ops()
    caps = []
    for op, (method, path) in sorted(ops.items()):
        homes = [f for f, (prod, _) in sources.items() if re.search(r"\bfn\s+" + re.escape(op) + r"\b", prod)]
        unit = any(op in sources[f][1] for f in homes)
        caps.append(
            {
                "id": op,
                "implemented_in": sorted(homes),
                "unit": unit,
                "e2e": path in reachable or op in probed,
                "e2e_by": "cli" if path in reachable else ("probe" if op in probed else None),
                "http": f"{method.upper()} {path}",
            }
        )
    return {
        "surface": "api",
        "measured": True,
        "capabilities": caps,
        "unit_measured": bool(sources),
        "e2e_measured": bool(reachable),
        "e2e_instrument": "transitive: the CLI requests the path, and every CLI leaf is invoked by a track",
        "notes": {"unreached_by_cli": [c["id"] for c in caps if not c["e2e"]]},
    }


# ─── Surface: native tools ───────────────────────────────────────────────────


# A tool whose only production path is a chat surface with a human in the
# loop. `ask_user` is registered by the dispatcher only when a pending-input
# registry is handed over, which the task-mode runner never does
# (`crates/apollia-cli/src/commands/start/runner.rs`, `pending_user_inputs:
# None`), so no eval task can drive it: evals/tools/README.md names the desktop
# automaton as its instrument. The automaton proves it the way it proves a
# gesture: a `sendChat` that asks the model for the tool, then an acting step
# on the card only that tool renders. Both halves are required; a prompt that
# merely names the tool is the mention this axis refuses to count.
TOOL_CHAT_SURFACES = {"ask_user": ("ask-user-skip", "ask-user-submit")}
CHAT_COMPOSERS = frozenset({"chat-input", "quickpicker-textarea"})


def tools_driven_by_automation(scripts_dir: Path) -> set[str]:
    """Tools a recipe asks the model for and then answers on the tool's own card."""
    driven: set[str] = set()
    if not scripts_dir.is_dir():
        return driven
    for path in sorted(scripts_dir.glob("*.json")):
        try:
            steps = json.loads(read(path)).get("steps", [])
        except json.JSONDecodeError:
            continue
        for tool, anchors in TOOL_CHAT_SURFACES.items():
            asked_at = None
            for i, step in enumerate(steps):
                # The ask is typed into a composer: the chat input through
                # `sendChat`, or the quickpicker's textarea through `fill`.
                composer = step.get("kind") == "sendChat" or (
                    step.get("kind") == "fill" and step.get("testid") in CHAT_COMPOSERS
                )
                if composer and tool in str(step.get("text", "")):
                    asked_at = i
                elif (asked_at is not None and step.get("kind") in ACTION_KINDS
                      and step.get("testid") in anchors):
                    driven.add(tool)
                    break
    return driven


def extract_tools(dispatcher: Path, tools_dir: Path, tracks_dir: Path, eval_suites: Path,
                  automation_dir: Path | None = None) -> dict:
    if not dispatcher.exists():
        return unmeasured("tools", f"{dispatcher} is absent, so no tool was enumerated")
    names = sorted(set(re.findall(r'is_active\("([a-z_]+)"\)', read(dispatcher))))
    if not names:
        return unmeasured("tools", "no is_active(\"...\") registration was found")

    sources = rust_sources(tools_dir)
    suites = []
    if eval_suites.is_dir():
        for ext in ("toml", "json", "yaml", "yml"):
            suites += sorted(glob.glob(f"{eval_suites}/**/*.{ext}", recursive=True))
    # A task id, not a mention. Every one of these suites names its tools in a
    # header comment and again inside prompts; counting those would credit a
    # tool the suite merely talks about, which is the error that reported 18
    # connector operations covered.
    suite_ids: set[str] = set()
    for suite in suites:
        text = read(suite)
        suite_ids.update(re.findall(r'^\s*id\s*=\s*"([^"]+)"', text, re.M))
        suite_ids.update(re.findall(r'"id"\s*:\s*"([^"]+)"', text))

    driven = tools_driven_by_automation(automation_dir) if automation_dir else set()
    caps = []
    for name in names:
        homes = [f for f, (prod, _) in sources.items() if f'"{name}"' in prod]
        unit = any(f'"{name}"' in sources[f][1] for f in homes)
        caps.append(
            {
                "id": name,
                "implemented_in": sorted(homes),
                "unit": unit,
                "e2e": (name in suite_ids if suites else False) or name in driven,
                "e2e_by": ("eval" if name in suite_ids else "desktop" if name in driven else None),
            }
        )
    return {
        "surface": "tools",
        "measured": True,
        "capabilities": caps,
        "unit_measured": bool(sources),
        "e2e_measured": bool(suites) or bool(driven),
        "e2e_instrument": (
            f"{len(suites)} eval suite(s) under {eval_suites}"
            + (f", and the automation corpus for {', '.join(sorted(driven))}" if driven else "")
            if suites
            else f"none: {eval_suites} holds no suite, so the column is blindness, not zero"
        ),
        "notes": {"unresolved_modules": [c["id"] for c in caps if not c["implemented_in"]]},
    }


# ─── Surface: connectors ─────────────────────────────────────────────────────



# The replay harness of `crates/apollia-connectors` serves a recorded or
# hand-written response to the real client method and compares what the client
# reads back against the `expect` the fixture declares. A fixture on disk is
# therefore an execution of that operation, not a mention of it: the crate's 66
# older tests all check the shape of the REQUEST, which is why this surface read
# zero for so long.
#
# Wired here for the same reason the router probes were: an axis that ignores an
# instrument written against it teaches people to stop writing instruments.
FIXTURES_DIR = REPO_ROOT / "crates/apollia-connectors/fixtures"


def replay_fixture_ops() -> set[str]:
    """Connector operations a replay fixture exercises end to end."""
    if not FIXTURES_DIR.is_dir():
        return set()
    ops: set[str] = set()
    for path in FIXTURES_DIR.glob("*.json"):
        try:
            declared = json.loads(path.read_text(encoding="utf-8")).get("operation")
        except (OSError, ValueError):
            continue
        if isinstance(declared, str):
            ops.add(declared)
    return ops

def connector_exemptions() -> tuple[frozenset[str], frozenset[str]]:
    """The operations the fixture guard already holds outside its denominator.

    Read from `scripts/check_connector_fixtures.py` rather than restated here.
    It carried them first, having measured them, and two files answering the
    same question with two numbers is the defect this inventory exists to
    surface: it said 51 declared and 35 uncovered while the guard said 41 and
    25, for the same tree at the same second.

    One set makes no HTTP call at all, the other has a client that reads nothing
    back, so no fixture can measure a reading for either.
    """
    guard = REPO_ROOT / "scripts/check_connector_fixtures.py"
    if not guard.is_file():
        return frozenset(), frozenset()
    spec = importlib.util.spec_from_file_location("connector_fixtures_guard", guard)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return (
        frozenset(getattr(module, "NO_UPSTREAM_CALL", ())),
        frozenset(getattr(module, "READS_NOTHING", ())),
    )


def extract_connectors(specs, bridge: Path, tracks_dir: Path, scripts_dir: Path) -> dict:
    ids: list[str] = []
    for spec in specs:
        ids += re.findall(r'id:\s*"([a-z_]+\.[a-z_]+)"', read(spec))
    no_call, reads_nothing = connector_exemptions()
    ids = sorted(set(ids) - no_call - reads_nothing)
    if not ids:
        return unmeasured("connectors", "no operation id was found in the connector families")

    sources = rust_sources(bridge)
    tracks = sorted(tracks_dir.glob("*.sh")) if tracks_dir.is_dir() else []
    invoked: set[str] = set()
    for track in tracks:
        for tokens in cli_cov.track_invocations(read(track)):
            invoked.update(tokens)
    acted = automation_actions(scripts_dir) if scripts_dir.is_dir() else set()
    replayed = replay_fixture_ops()

    caps = []
    for op in ids:
        # The dispatch arm is the implementation. The spec table that declares
        # the id is a description, and its neighbouring test is the enumeration
        # array that made 18 operations look covered.
        homes = [f for f, (prod, _) in sources.items() if f'"{op}" =>' in prod]
        unit = any(f'"{op}"' in sources[f][1] for f in homes)
        caps.append(
            {
                "id": op,
                "family": op.split(".")[0],
                "implemented_in": sorted(homes),
                "unit": unit,
                "e2e": op in invoked or op in acted or op in replayed,
                "e2e_by": ("cli" if op in invoked else "desktop" if op in acted
                           else "replay" if op in replayed else None),
            }
        )
    return {
        "surface": "connectors",
        "measured": True,
        "capabilities": caps,
        "unit_measured": bool(sources),
        "e2e_measured": bool(tracks or acted or replayed),
        "e2e_instrument": (
            f"{len(tracks)} CLI track(s), the automation corpus, "
            f"and {len(replayed)} replay fixture(s)"
        ),
        "notes": {
            "families": sorted({c["family"] for c in caps}),
            "unresolved_dispatch": [c["id"] for c in caps if not c["implemented_in"]],
        },
    }


# ─── Dead capabilities ───────────────────────────────────────────────────────


def dead_capabilities(inventory: dict, baseline: dict[str, str] | None = None) -> tuple[dict[str, str], list[str]]:
    """Cross the unwired-builder baseline with the enumerated capabilities.

    Returns (capability id -> the builder whose verdict names it, builders that
    name no enumerated capability). The second list is not noise: it is the
    dead surface living outside these five denominators, and stating it is what
    keeps the first list from reading as the whole truth.
    """
    marked: dict[str, str] = {}
    orphans: list[str] = []
    known = {
        cap["id"]: surface
        for surface, block in inventory.items()
        if block.get("measured")
        for cap in block["capabilities"]
    }
    for builder, verdict in (baseline or check_optional_builders.BASELINE).items():
        if not UNWIRED.match(verdict.strip()):
            continue
        # Only a name the verdict SPELLS OUT, inside backticks or quotes,
        # names a capability. Matching bare words made the English word
        # "default" collide with a desktop anchor of that name and reported a
        # live gesture as dead.
        tokens = {
            t
            for t in re.findall(r"`([^`]+)`|\"([^\"]+)\"", verdict)
            for t in t
            if len(t) >= 4
        }
        hits = [cap for cap in known if cap in tokens]
        if hits:
            for cap in hits:
                marked[cap] = builder
        else:
            orphans.append(builder)
    return marked, orphans


# ─── Assembly and verdict ────────────────────────────────────────────────────


def unmeasured(surface: str, reason: str) -> dict:
    return {
        "surface": surface,
        "measured": False,
        "reason": reason,
        "capabilities": [],
        "unit_measured": False,
        "e2e_measured": False,
        "e2e_instrument": "none",
        "notes": {},
    }


def totals(block: dict, dead: dict[str, str]) -> dict:
    live = [c for c in block["capabilities"] if c["id"] not in dead]
    unit = sum(1 for c in live if c["unit"])
    e2e = sum(1 for c in live if c["e2e"])
    return {
        "total": len(block["capabilities"]),
        "dead": len(block["capabilities"]) - len(live),
        "denominator": len(live),
        "unit": unit if block["unit_measured"] else None,
        "e2e": e2e if block["e2e_measured"] else None,
        "holes": (len(live) - e2e) if block["e2e_measured"] else None,
    }


def inconsistencies(inventory: dict, dead: dict[str, str]) -> list[str]:
    """Mechanical contradictions inside the inventory itself."""
    problems = []
    for surface, block in inventory.items():
        if not block.get("measured"):
            continue
        seen = set()
        for cap in block["capabilities"]:
            if cap["id"] in seen:
                problems.append(f"{surface}: duplicate capability id {cap['id']!r}")
            seen.add(cap["id"])
        agg = totals(block, dead)
        for column in ("unit", "e2e"):
            value = agg[column]
            if value is not None and value > agg["denominator"]:
                problems.append(
                    f"{surface}: {column} covers {value} of {agg['denominator']} live capabilities"
                )
        for cap in block["capabilities"]:
            if cap["id"] in dead and cap["e2e"]:
                problems.append(
                    f"{surface}: {cap['id']!r} is declared dead by "
                    f"{dead[cap['id']]} yet an instrument executes it"
                )
    return problems


def build(args) -> tuple[dict, dict[str, str], list[str]]:
    wanted = SURFACES if args.surface == "all" else (args.surface,)
    inventory: dict[str, dict] = {}
    if "cli" in wanted:
        inventory["cli"] = extract_cli(Path(args.bin), TRACKS_DIR, CLI_COMMANDS)
    if "desktop" in wanted:
        inventory["desktop"] = extract_desktop(UI_ROOT, AUTOMATION_DIR)
    if "api" in wanted:
        inventory["api"] = extract_api(OPENAPI, API_DIR, REPO_ROOT / "crates/apollia-cli/src")
    if "tools" in wanted:
        inventory["tools"] = extract_tools(DISPATCHER, TOOLS_DIR, TRACKS_DIR, Path(args.eval_suites),
                                           automation_dir=AUTOMATION_DIR)
    if "connectors" in wanted:
        inventory["connectors"] = extract_connectors(
            CONNECTOR_SPECS, CONNECTOR_BRIDGE, TRACKS_DIR, AUTOMATION_DIR
        )
    dead, orphans = dead_capabilities(inventory)
    return inventory, dead, orphans


def verdict(inventory: dict, dead: dict[str, str]) -> tuple[int, list[str]]:
    if any(not block.get("measured") for block in inventory.values()):
        return 2, [
            f"{block['surface']}: {block['reason']}"
            for block in inventory.values()
            if not block.get("measured")
        ]
    problems = inconsistencies(inventory, dead)
    return (1 if problems else 0), problems


def render(inventory: dict, dead: dict[str, str], orphans: list[str]) -> None:
    print(f"{'surface':<12}{'total':>7}{'dead':>6}{'live':>6}{'unit':>7}{'e2e':>7}{'holes':>7}")
    print("-" * 52)
    grand = {"total": 0, "dead": 0, "denominator": 0, "unit": 0, "e2e": 0, "holes": 0}
    for surface in SURFACES:
        block = inventory.get(surface)
        if block is None:
            continue
        if not block["measured"]:
            print(f"{surface:<12}{'NOTHING MEASURED':>40}")
            print(f"{'':<12}{block['reason']}")
            continue
        agg = totals(block, dead)
        cells = [
            f"{agg['total']:>7}",
            f"{agg['dead']:>6}",
            f"{agg['denominator']:>6}",
            f"{agg['unit']:>7}" if agg["unit"] is not None else f"{'?':>7}",
            f"{agg['e2e']:>7}" if agg["e2e"] is not None else f"{'?':>7}",
            f"{agg['holes']:>7}" if agg["holes"] is not None else f"{'?':>7}",
        ]
        print(f"{surface:<12}" + "".join(cells))
        for key in grand:
            value = agg[key]
            if value is not None:
                grand[key] += value
    print("-" * 52)
    print(
        f"{'TOTAL':<12}{grand['total']:>7}{grand['dead']:>6}{grand['denominator']:>6}"
        f"{grand['unit']:>7}{grand['e2e']:>7}{grand['holes']:>7}"
    )

    for surface in SURFACES:
        block = inventory.get(surface)
        if block is None or not block["measured"]:
            continue
        if not block["e2e_measured"]:
            print(f"\n{surface}: the end-to-end column is NOT measured. {block['e2e_instrument']}")
    print("\ndead capabilities inside these denominators:")
    if dead:
        for cap, builder in sorted(dead.items()):
            print(f"  {cap}  (declared unwired by {builder})")
    else:
        print("  none: every unwired builder of the baseline names a capability outside them")
    if orphans:
        print(
            f"\n{len(orphans)} unwired builder(s) name no enumerated capability, so the dead"
            "\nsurface they describe sits outside these five denominators:"
        )
        for builder in sorted(orphans):
            print(f"  {builder}")

    desktop = inventory.get("desktop")
    if desktop and desktop["measured"]:
        notes = desktop["notes"]
        residual = notes["acted_but_classified_marker"]
        wrapped = notes.get("reached_through_a_wrapper", [])
        print(
            f"\ndesktop cross-check: of {notes['acted_in_corpus']} anchors an automation step acts"
            f" on,\n  {notes['acted_in_corpus'] - len(residual)} are classified as gestures."
            f" {len(residual)} residual(s): {', '.join(residual) or 'none'}"
        )
        if wrapped:
            # Not residuals: the runner descends into a wrapper for `fill` and
            # `setChecked`. Named anyway, because an anchor that only works
            # through that descent is one refactor away from silence.
            print(
                f"  {len(wrapped)} more sit on a wrapper the runner descends into,"
                f" which is by design: {', '.join(wrapped)}"
            )
        promoted = notes.get("reconstructed_promoted_by_evidence", [])
        print(
            f"  {notes.get('reconstructed_unverified', 0)} reconstructed anchor(s) sit outside"
            " the denominator: their id is built from data and rebuilt from the file's"
            "\n  string literals, which over-generates; only a run can tell which render."
            f" {len(promoted)} such anchor(s) a step acts on are counted on that evidence."
        )


# ─── Selftest ────────────────────────────────────────────────────────────────


def selftest() -> int:
    failures = []

    def case(name: str, condition: bool) -> None:
        if condition:
            print(f"  ok    {name}")
        else:
            print(f"  FAIL  {name}")
            failures.append(name)

    missing = Path("/nonexistent-apollia-inventory")

    # Every enumerator must return "nothing measured", never an empty pass.
    case("cli on an absent binary measures nothing",
         extract_cli(missing / "apollia-os", missing, missing)["measured"] is False)
    case("desktop on an absent UI root measures nothing",
         extract_desktop(missing, missing)["measured"] is False)

    # A reconstructed anchor enters the denominator on evidence only.
    import tempfile
    with tempfile.TemporaryDirectory() as tmp:
        ui = Path(tmp) / "ui"
        (ui / "routes").mkdir(parents=True)
        (ui / "routes" / "Probe.svelte").write_text(
            '<script lang="ts">\n'
            '  const actions = [{ id: "alpha" }];\n'
            '  const cls = cn("rounded-xl");\n'
            '</script>\n'
            '<button data-testid="real-btn">x</button>\n'
            '{#each actions as action}\n'
            '  <button data-testid={`${action.id}-btn`} class={cls}>y</button>\n'
            '{/each}\n',
            encoding="utf-8",
        )
        scripts = Path(tmp) / "scripts"
        scripts.mkdir()
        (scripts / "probe.json").write_text(
            json.dumps({"name": "probe", "steps": [{"kind": "click", "testid": "alpha-btn"}]}),
            encoding="utf-8",
        )
        probe = extract_desktop(ui, scripts)
        ids = {c["id"]: c for c in probe["capabilities"]}
        case("a literal anchor is in the desktop denominator", "real-btn" in ids)
        case("a reconstructed anchor a step acts on is counted, and covered",
             "alpha-btn" in ids and ids["alpha-btn"]["e2e"] is True)
        case("a reconstructed anchor nothing acts on stays out of the denominator",
             "rounded-xl-btn" not in ids and probe["notes"]["reconstructed_unverified"] == 1)
    case("api on an absent document measures nothing",
         extract_api(missing / "openapi.json", missing, missing)["measured"] is False)
    case("tools on an absent dispatcher measures nothing",
         extract_tools(missing / "d.rs", missing, missing, missing)["measured"] is False)

    # A tool driven through its chat card needs the ask and the answer.
    with tempfile.TemporaryDirectory() as tmp:
        scripts = Path(tmp) / "scripts"
        scripts.mkdir()
        both = {"name": "p", "steps": [
            {"kind": "sendChat", "text": "Use the ask_user tool to ask my name."},
            {"kind": "click", "testid": "ask-user-skip"}]}
        ask_only = {"name": "q", "steps": [
            {"kind": "fill", "testid": "quickpicker-textarea", "text": "Use ask_user."},
            {"kind": "click", "testid": "quickpicker-submit"}]}
        answer_only = {"name": "r", "steps": [{"kind": "click", "testid": "ask-user-skip"}]}
        (scripts / "both.json").write_text(json.dumps(both), encoding="utf-8")
        case("a recipe that asks for ask_user and answers its card drives the tool",
             tools_driven_by_automation(scripts) == {"ask_user"})
        (scripts / "both.json").unlink()
        (scripts / "ask.json").write_text(json.dumps(ask_only), encoding="utf-8")
        (scripts / "answer.json").write_text(json.dumps(answer_only), encoding="utf-8")
        case("a prompt that names the tool, or a click with no ask, drives nothing",
             tools_driven_by_automation(scripts) == set())
    case("connectors on absent families measures nothing",
         extract_connectors((missing / "g.rs",), missing, missing, missing)["measured"] is False)

    # An unmeasured denominator is a 2, and a 2 is never a pass.
    blind = {"cli": unmeasured("cli", "fixture")}
    code, _ = verdict(blind, {})
    case("an unmeasured denominator yields exit 2", code == 2)

    clean = {
        "cli": {
            "surface": "cli", "measured": True, "unit_measured": True, "e2e_measured": True,
            "e2e_instrument": "fixture", "notes": {},
            "capabilities": [
                {"id": "agent list", "implemented_in": ["x.rs"], "unit": True, "e2e": True},
                {"id": "agent show", "implemented_in": ["x.rs"], "unit": False, "e2e": False},
            ],
        }
    }
    code, problems = verdict(clean, {})
    case("a consistent inventory yields exit 0", code == 0 and not problems)
    case("a hole is counted, not hidden", totals(clean["cli"], {})["holes"] == 1)

    duplicated = json.loads(json.dumps(clean))
    duplicated["cli"]["capabilities"].append(duplicated["cli"]["capabilities"][0])
    code, problems = verdict(duplicated, {})
    case("a duplicate id yields exit 1", code == 1 and any("duplicate" in p for p in problems))

    contradiction = json.loads(json.dumps(clean))
    code, problems = verdict(contradiction, {"agent list": "with_ghost@fixture"})
    case("a dead capability an instrument executes yields exit 1",
         code == 1 and any("declared dead" in p for p in problems))

    excluded = json.loads(json.dumps(clean))
    excluded["cli"]["capabilities"][0]["e2e"] = False
    agg = totals(excluded["cli"], {"agent list": "with_ghost@fixture"})
    case("a dead capability leaves the denominator",
         agg["total"] == 2 and agg["denominator"] == 1 and agg["dead"] == 1)

    # The dead cross must fire on a spelled-out name and stay silent on an
    # English word that happens to equal a capability id.
    named, orphans = dead_capabilities(
        clean, {"with_ghost@fixture": "defect, open: `agent list` has no caller."}
    )
    case("a verdict naming a capability marks it dead",
         named == {"agent list": "with_ghost@fixture"} and not orphans)
    named, orphans = dead_capabilities(
        clean, {"with_ghost@fixture": "defect, open: the default path leaves it None."}
    )
    case("a bare English word marks nothing dead",
         named == {} and orphans == ["with_ghost@fixture"])
    named, _ = dead_capabilities(
        clean, {"with_ghost@fixture": "held for v0.2: `agent list` is deferred."}
    )
    case("a builder held rather than unwired marks nothing dead", named == {})

    # The Rust splitter must not credit a production mention to a test.
    prod, tests = split_rust(
        'fn handler() { let x = "file_read"; }\n'
        '#[cfg(test)]\nmod t { fn a() { assert!("web_read".len() > 0); } }\n'
    )
    case("the cfg(test) split keeps production out of the test text",
         '"file_read"' in prod and '"file_read"' not in tests and '"web_read"' in tests)

    # A path template normalises to the shape the OpenAPI document declares,
    # and the query string never joins the comparison.
    case("a path template normalises to its OpenAPI shape",
         normalise_path("/api/v1/agents/{agent_id}/logs?last={last}") == "/api/v1/agents/{}/logs")

    # A doc comment is not a caller: the CLI names /api/v1/a2a/agents in a
    # `///` line only, and a hand count read that as a call.
    case("a path written only in a doc comment is not a call",
         "/api/v1/a2a/agents" not in cli_called_paths(REPO_ROOT / "crates/apollia-cli/src"))

    # An observation step does not exercise a gesture.
    case("only action steps count as an execution",
         "waitFor" not in ACTION_KINDS and "click" in ACTION_KINDS)

    if failures:
        print(f"\nselftest: {len(failures)} case(s) failed", file=sys.stderr)
        return 1
    print("\nselftest: every case holds")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--bin", default=str(DEFAULT_BIN))
    parser.add_argument("--surface", default="all", choices=("all",) + SURFACES)
    parser.add_argument(
        "--eval-suites",
        default=str(EVAL_SUITES),
        help="directory of apollia eval suites; the tools end-to-end column is "
             "blindness, not zero, when it holds none",
    )
    parser.add_argument("--json", action="store_true", help="machine-readable inventory")
    parser.add_argument("--list-holes", action="store_true", help="name every uncovered capability")
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args()

    if args.selftest:
        return selftest()

    inventory, dead, orphans = build(args)
    code, problems = verdict(inventory, dead)

    if args.json:
        payload = {
            "exit_code": code,
            "problems": problems,
            "dead": dead,
            "unwired_builders_outside_denominators": sorted(orphans),
            "surfaces": {
                name: {**block, "totals": totals(block, dead) if block["measured"] else None}
                for name, block in inventory.items()
            },
        }
        print(json.dumps(payload, indent=2, sort_keys=True))
        return code

    render(inventory, dead, orphans)
    if args.list_holes:
        for surface, block in inventory.items():
            if not block["measured"] or not block["e2e_measured"]:
                continue
            holes = [c["id"] for c in block["capabilities"] if not c["e2e"] and c["id"] not in dead]
            print(f"\n{surface}: {len(holes)} capability/capabilities no instrument executes")
            for hole in holes:
                print(f"  NONE  {hole}")
    if code == 2:
        print("\nNOTHING MEASURED on at least one denominator:", file=sys.stderr)
        for problem in problems:
            print(f"  {problem}", file=sys.stderr)
    elif code == 1:
        print(f"\n{len(problems)} inconsistency/inconsistencies:", file=sys.stderr)
        for problem in problems:
            print(f"  {problem}", file=sys.stderr)
    return code


if __name__ == "__main__":
    sys.exit(main())
