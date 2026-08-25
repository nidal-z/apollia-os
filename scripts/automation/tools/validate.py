#!/usr/bin/env python3
"""Static validator for the desktop automation scripts.

Checks: JSON parses, every step matches the schema of its kind (types.ts is
the contract: required keys, allowed keys, exactly one target), every goto
route exists in the app's Route type, every testid/testidPrefix resolvable
against the UI source.

Resolving a testid means reproducing, statically, what the UI renders at
runtime. Three shapes exist in the source:
  1. a plain literal            data-testid="chat-input"
  2. a leading-literal template  data-testid="transcript-card-{id}"
  3. an expression-led template  data-testid={`${action.id}-btn`}
Shape 3 carries no literal prefix, so it needs the value that fills the slot.
Two rules recover it: a shared component that composes `${dataTestId}-input`
contributes its suffixes to the testid literals of the files that import it,
and a template built inside one file is resolved against that file's own
string literals (the loop that feeds it lives there).

Resolution of a step's exact `testid` is by equality: the id is a literal,
a composed id, or is named by the script's own `dynamicTestids` declaration.
A dynamic prefix (shape 2) never resolves an exact step on its own: that rule
let 57 literal anchors disappear from the source unnoticed, each shadowed by
a template sharing its first characters. A script that targets an instance id
built from data (`automation-row-<seed id>`) declares it in a top-level
`dynamicTestids` list; the declaration is itself validated (the id must sit
under a strictly shorter dynamic prefix, must be used by a step, and must not
shadow an id that resolves exactly).

The corpus deliberately excludes `src/lib/automation/` and `*.test.ts`: the
harness and the unit tests reference anchors, they do not render any, and
feeding them to the corpus masked the removal of every anchor they name.

Exit codes: 0 all scripts clean, 1 at least one problem, 2 nothing measured
(no corpus or no script: the verdict of an empty measurement is not a pass).
"""
import glob
import json
import os
import re
import sys

UI = "crates/apollia-desktop/ui/src"
SCRIPTS = "scripts/automation"
NAVIGATION = f"{UI}/lib/stores/navigation.ts"

# Step schemas, mirrored from src/lib/automation/types.ts. `target` says how
# many of testid/testidPrefix a step takes: "one" exactly one, "opt" at most
# one, None none. `nth` rides along whenever a target is allowed.
TARGET_KEYS = {"testid", "testidPrefix", "nth"}
SCHEMAS = {
    # kind: (required, optional, target)
    "goto": ({"route"}, set(), None),
    "waitFor": (set(), {"timeoutMs"}, "one"),
    "waitGone": (set(), {"timeoutMs"}, "one"),
    "click": (set(), {"timeoutMs"}, "one"),
    "fill": ({"text"}, {"timeoutMs"}, "one"),
    "sendChat": ({"text"}, set(), None),
    "expect": (set(), {"contains"}, "one"),
    "captureText": ({"as"}, set(), "one"),
    "screenshot": ({"label"}, set(), None),
    "sleep": ({"ms"}, set(), None),
    "awaitTurn": (set(), {"approve", "timeoutMs", "label", "maxApprovals"}, None),
    "setChecked": ({"checked"}, set(), "one"),
    "selectOption": (set(), {"timeoutMs", "value", "labelText", "index"}, "one"),
    "press": ({"key"}, {"meta", "ctrl", "shift", "alt", "timeoutMs"}, "opt"),
}
SCRIPT_KEYS = {"name", "stopOnError", "destructive", "notes", "dynamicTestids", "steps"}

ID = r'[A-Za-z0-9_\-./:]'
# A component receiving the caller's id composes children off it
# (`${dataTestId}-input`, `${testid}-toggle`). What makes it a component-wide
# suffix rather than a route-local id is that the slot holds a declared PROP:
# a snippet parameter of the same name only ever takes that one file's
# literals, and is resolved through the literal pool instead.
PROPS_BLOCK = re.compile(r'let\s*\{(.*?)\}\s*(?::\s*\w+\s*)?=\s*\$props\(\)', re.S)
# Literals in an argument or object-value position feed the loops and snippet
# calls that build ids. Attribute values (`for="..."`) are excluded on purpose:
# they name form controls, not testids, and would mask real removals.
ARG_LITERAL = re.compile(r'[(,:]\s*["\']([A-Za-z0-9_\-]{2,60})["\']')
IMPORT = re.compile(r'^\s*import\s+(.+?)\s+from\s+["\']([^"\']+)["\']', re.M)


def slots_of(raw):
    """Literal fragments and slot expressions of a `${x}-y` / `{x}-y` template."""
    frags = re.split(r'\$\{[^}]*\}|\{[^}]*\}', raw)
    exprs = [(a or b).strip() for a, b in re.findall(r'\$\{([^}]*)\}|(?<!\$)\{([^}]+)\}', raw)]
    return frags, exprs


def raw_values(txt, i):
    """Yield every id expression written after a testid attribute at offset i."""
    c = txt[i]
    if c in '"\'':
        j = txt.find(c, i + 1)
        if j > 0:
            yield txt[i + 1:j]
        return
    if c == '`':
        j = txt.find('`', i + 1)
        if j > 0:
            yield txt[i + 1:j]
        return
    if c != '{':
        return
    depth = 0
    j = i
    while j < len(txt):
        if txt[j] == '{':
            depth += 1
        elif txt[j] == '}':
            depth -= 1
            if depth == 0:
                break
        j += 1
    inner = txt[i + 1:j].strip()
    if inner and inner[0] in '"\'`':
        yield inner[1:-1] if len(inner) >= 2 else inner
        return
    # Ternary guard, the near-universal `dataTestId ? `${dataTestId}-x` : undefined`.
    quoted = re.findall(r'`([^`]*)`|"([^"]*)"|\'([^\']*)\'', inner)
    if quoted:
        for a, b, c2 in quoted:
            yield a or b or c2
        return
    yield None  # bare expression, the id comes from a prop or a snippet argument


def corpus_files(ui_root):
    """UI sources that render anchors. The automation harness and the unit
    tests reference anchors without rendering any; including them lets the
    harness vouch for the corpus it is supposed to check (it did)."""
    out = []
    for path in glob.glob(f"{ui_root}/**/*.svelte", recursive=True) + \
            glob.glob(f"{ui_root}/**/*.ts", recursive=True):
        rel = path.replace(os.sep, "/")
        if f"{ui_root}/lib/automation/" in rel or rel.endswith(".test.ts"):
            continue
        out.append(path)
    return out


def imported_components(txt):
    """Local names a file imports: the default import of a `.svelte` module
    and every braced name (barrel re-exports keep the component's name)."""
    names = set()
    for m in IMPORT.finditer(txt):
        clause, module = m.group(1), m.group(2)
        braced = re.search(r'\{([^}]*)\}', clause)
        if braced:
            for part in braced.group(1).split(","):
                part = part.split(" as ")[-1].strip()
                if part:
                    names.add(part)
        default = re.match(r'([A-Za-z_$][\w$]*)', clause)
        if default:
            names.add(default.group(1))
        if module.endswith(".svelte"):
            names.add(os.path.splitext(os.path.basename(module))[0])
    return names


def build_corpus(ui_root=UI):
    """Resolve every anchor the UI can render.

    Returns (static_ids, prefixes, composed_ids, n_suffixes).
    Composed ids are restricted to real usage: a shared component's suffixes
    apply to the testid literals of the files that import that component,
    never to the whole tree (the cartesian product resolved 30 970 ids of
    which the UI renders a few dozen, and that surplus masked removals).
    """
    static_ids = set()
    prefixes = set()
    component_suffixes = {}   # component basename -> set of tails it appends
    file_contrib = {}         # path -> ids that file feeds a testid-ish slot
    file_imports = {}         # path -> component names imported by that file

    for path in corpus_files(ui_root):
        try:
            txt = open(path, encoding="utf-8").read()
        except OSError:
            continue
        component = os.path.splitext(os.path.basename(path))[0]
        contrib = set()
        pool = set(ARG_LITERAL.findall(txt))
        props = set()
        for pm in PROPS_BLOCK.finditer(txt):
            # `"data-testid": testid` binds the prop to a local name; both count.
            props.update(re.findall(r'([A-Za-z_$][\w$]*)\s*(?:[,}]|=)', pm.group(1)))
            props.update(re.findall(r':\s*([A-Za-z_$][\w$]*)', pm.group(1)))
        for m in re.finditer(r'(?:data-testid|dataTestId|testId|testid)\s*[=:]\s*', txt):
            i = m.end()
            if i >= len(txt):
                continue
            for raw in raw_values(txt, i):
                if raw is None:
                    # `data-testid={testid}`: a snippet renders whatever its caller
                    # passed, so every id-shaped argument literal of this file lands.
                    static_ids.update(pool)
                    contrib.update(pool)
                    continue
                if not ('${' in raw or re.search(r'\{[^}]', raw) or '+' in raw):
                    v = raw.strip()
                    if re.fullmatch(f'{ID}+', v):
                        static_ids.add(v)
                        contrib.add(v)
                    continue
                frags, exprs = slots_of(raw)
                if frags and frags[0]:
                    prefixes.add(frags[0])
                    continue
                if not exprs:
                    continue
                tail = ''.join(f for f in frags[1:] if f)
                if len(exprs) == 1 and tail:
                    if exprs[0] in props:
                        # `${dataTestId}-input`: a suffix this component appends
                        # to whatever id its importers hand it.
                        component_suffixes.setdefault(component, set()).add(tail)
                    else:
                        # `${action.id}-btn`: the loop feeding the slot lives in
                        # this file, so its literal pool fills it.
                        static_ids.update(lit + tail for lit in pool)
                        contrib.update(lit + tail for lit in pool)
                elif len(exprs) >= 2 and len(frags) > 1 and frags[1]:
                    prefixes.update(lit + frags[1] for lit in pool)  # `{testid}-{opt.value}`
        # TabBar/FilterChipBar: a `testidPrefix="X"` prop yields child ids
        # `X-tabbar`, `X-tab-<key>` (TabBar) and `X-<key>` (FilterChipBar).
        # SplitLayout: sidebarTestid="X" / detailTestid="X" render as data-testid.
        for pm in re.finditer(f'(?:sidebarTestid|detailTestid)\\s*=\\s*["\\\']({ID}+)["\\\']', txt):
            static_ids.add(pm.group(1))
        for pm in re.finditer(f'testidPrefix\\s*=\\s*["\\\']({ID}+)["\\\']', txt):
            X = pm.group(1)
            static_ids.add(f"{X}-tabbar")
            prefixes.add(f"{X}-tab-")
            prefixes.add(f"{X}-")
        file_contrib[path] = contrib
        file_imports[path] = imported_components(txt)

    composed_ids = set()
    for path, imports in file_imports.items():
        for component in imports & component_suffixes.keys():
            composed_ids.update(
                base + suffix
                for base in file_contrib[path]
                for suffix in component_suffixes[component]
            )
    n_suffixes = sum(len(s) for s in component_suffixes.values())
    return static_ids, prefixes, composed_ids, n_suffixes


def load_routes(path=NAVIGATION):
    """The Route union of navigation.ts, the only routes `goto` can reach."""
    try:
        txt = open(path, encoding="utf-8").read()
    except OSError:
        return set()
    m = re.search(r'export\s+type\s+Route\s*=\s*([^;]+);', txt)
    if not m:
        return set()
    return set(re.findall(r'"([^"]+)"', m.group(1)))


def prefix_ok(tp, static_ids, prefixes):
    """A prefix step selects `[data-testid^="tp"]`, so something must start with
    it. The old rule also accepted `tp.startswith(id)`, which let a stale
    `set-default-` pass against the plain `set-default` it can never match."""
    if any(s.startswith(tp) for s in static_ids):
        return True
    return any(p and (p.startswith(tp) or tp.startswith(p)) for p in prefixes)


def check_step(idx, step, routes, resolve_testid, static_ids, prefixes):
    """Schema and resolution errors of one step, as a list of messages."""
    errs = []
    kind = step.get("kind")
    if kind not in SCHEMAS:
        return [f"step {idx}: illegal kind {kind!r}"]
    required, optional, target = SCHEMAS[kind]
    allowed = {"kind"} | required | optional | (TARGET_KEYS if target else set())
    for key in step:
        if key not in allowed:
            errs.append(f"step {idx} ({kind}): unknown key {key!r}")
    for key in required:
        if key not in step:
            errs.append(f"step {idx} ({kind}): missing required key {key!r}")
    if target:
        named = sum(1 for key in ("testid", "testidPrefix") if step.get(key) is not None)
        if target == "one" and named != 1:
            errs.append(f"step {idx} ({kind}): exactly one of testid/testidPrefix required")
        if target == "opt" and named > 1:
            errs.append(f"step {idx} ({kind}): at most one of testid/testidPrefix")
    if kind == "selectOption":
        selectors = sum(1 for key in ("value", "labelText", "index") if key in step)
        if selectors != 1:
            errs.append(f"step {idx} (selectOption): exactly one of value/labelText/index required")
    if kind == "goto" and step.get("route") not in routes:
        errs.append(f"step {idx}: illegal route {step.get('route')!r}")
    t = step.get("testid")
    if t is not None and not resolve_testid(t):
        errs.append(f"step {idx} ({kind}): unknown testid {t!r}")
    tp = step.get("testidPrefix")
    if tp is not None and not prefix_ok(tp, static_ids, prefixes):
        errs.append(f"step {idx} ({kind}): unknown testidPrefix {tp!r}")
    return errs


def check_script(data, static_ids, prefixes, composed_ids, routes):
    """All errors of one parsed script file."""
    errs = []
    for key in data:
        if key not in SCRIPT_KEYS:
            errs.append(f"script: unknown top-level key {key!r}")
    if "name" not in data:
        errs.append("script: missing top-level key 'name'")
    dynamics = data.get("dynamicTestids", [])
    if not isinstance(dynamics, list) or not all(isinstance(d, str) for d in dynamics):
        errs.append("script: dynamicTestids must be a list of strings")
        dynamics = []
    steps = data.get("steps", [])
    used = {s.get("testid") for s in steps if isinstance(s, dict)}
    for d in sorted(dynamics):
        if d in static_ids or d in composed_ids:
            errs.append(f"dynamicTestids: {d!r} resolves exactly, remove the declaration "
                        f"(a declared-dynamic literal is an anchor whose removal nothing sees)")
        elif not any(p and d.startswith(p) and len(p) < len(d) for p in prefixes):
            errs.append(f"dynamicTestids: {d!r} sits under no dynamic prefix of the source")
        if d not in used:
            errs.append(f"dynamicTestids: {d!r} is referenced by no step of this script")
    declared = set(dynamics)

    def resolve(t):
        return t in static_ids or t in composed_ids or t in declared

    for idx, step in enumerate(steps):
        if not isinstance(step, dict):
            errs.append(f"step {idx}: not an object")
            continue
        errs.extend(check_step(idx, step, routes, resolve, static_ids, prefixes))
    return errs


def main():
    static_ids, prefixes, composed_ids, n_suffixes = build_corpus()
    routes = load_routes()
    script_files = sorted(glob.glob(f"{SCRIPTS}/*.json"))

    print(f"corpus: {len(static_ids)} resolved testids, {len(prefixes)} dynamic prefixes, "
          f"{n_suffixes} composed suffixes ({len(composed_ids)} composed ids)")

    problems = {}
    total_steps = 0
    for f in script_files:
        name = os.path.basename(f)
        try:
            data = json.load(open(f))
        except (OSError, ValueError) as e:
            problems[name] = [f"JSON parse error: {e}"]
            continue
        total_steps += len(data.get("steps", []))
        errs = check_script(data, static_ids, prefixes, composed_ids, routes)
        if errs:
            problems[name] = errs

    print(f"scripts: {len(script_files)}, total steps: {total_steps}")
    print()
    if not static_ids or not script_files:
        print("NOTHING MEASURED: empty corpus or no script "
              "(run from the repository root)")
        sys.exit(2)
    if not problems:
        print("ALL CLEAN: 0 static issues")
        sys.exit(0)
    for name, errs in problems.items():
        print(f"### {name} ({len(errs)})")
        for e in errs[:40]:
            print("  -", e)
    print(f"\nTOTAL scripts with issues: {len(problems)}")
    sys.exit(1)


if __name__ == "__main__":
    main()
