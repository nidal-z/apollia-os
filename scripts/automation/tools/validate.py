#!/usr/bin/env python3
"""Static validator for the desktop automation scripts.
Checks: JSON parses, every step kind legal, every goto route legal, every
testid/testidPrefix resolvable against the UI source.

Resolving a testid means reproducing, statically, what the UI renders at
runtime. Three shapes exist in the source:
  1. a plain literal            data-testid="chat-input"
  2. a leading-literal template  data-testid="transcript-card-{id}"
  3. an expression-led template  data-testid={`${action.id}-btn`}
Shape 3 carries no literal prefix, so it needs the value that fills the slot.
Two rules recover it: a shared component that composes `${dataTestId}-input`
contributes the suffix `-input` to every known base id, and a template built
inside one file is resolved against that file's own string literals (the loop
that feeds it lives there). Without those rules the redesign's move to
`{#each}` + `{#snippet}` reads as ~28 removed anchors that in fact still
render."""
import json, os, re, sys, glob

UI = "crates/apollia-desktop/ui/src"
SCRIPTS = "scripts/automation"

KINDS = {"goto","waitFor","waitGone","click","fill","sendChat","expect",
         "captureText","screenshot","sleep","awaitTurn","setChecked",
         "selectOption","press"}
ROUTES = {"dashboard","agents","tasks","chat","inbox","integrations","llm",
          "automations","projects","memory","transcriptions","notifications",
          "observability","settings","design","design-motion",
          "design-empty-states","design-dark-mode","onboarding"}

# ---- Build testid corpus from source ----
static_ids = set()   # fully-literal testids, plus the ones we resolve
prefixes = set()     # leading literal segment of a dynamic testid
suffixes = set()     # tails a shared component appends to the id it is given

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
        if j > 0: yield txt[i+1:j]
        return
    if c == '`':
        j = txt.find('`', i + 1)
        if j > 0: yield txt[i+1:j]
        return
    if c != '{':
        return
    depth = 0; j = i
    while j < len(txt):
        if txt[j] == '{': depth += 1
        elif txt[j] == '}':
            depth -= 1
            if depth == 0: break
        j += 1
    inner = txt[i+1:j].strip()
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

for path in glob.glob(f"{UI}/**/*.svelte", recursive=True) + \
            glob.glob(f"{UI}/**/*.ts", recursive=True):
    try:
        txt = open(path, encoding="utf-8").read()
    except Exception:
        continue
    pool = set(ARG_LITERAL.findall(txt))
    props = set()
    for pm in PROPS_BLOCK.finditer(txt):
        # `"data-testid": testid` binds the prop to a local name; both count.
        props.update(re.findall(r'([A-Za-z_$][\w$]*)\s*(?:[,}]|=)', pm.group(1)))
        props.update(re.findall(r':\s*([A-Za-z_$][\w$]*)', pm.group(1)))
    for m in re.finditer(r'(?:data-testid|dataTestId|testId|testid)\s*[=:]\s*', txt):
        i = m.end()
        if i >= len(txt): continue
        for raw in raw_values(txt, i):
            if raw is None:
                # `data-testid={testid}`: a snippet renders whatever its caller
                # passed, so every id-shaped argument literal of this file lands.
                static_ids.update(pool)
                continue
            if not ('${' in raw or re.search(r'\{[^}]', raw) or '+' in raw):
                v = raw.strip()
                if re.fullmatch(f'{ID}+', v):
                    static_ids.add(v)
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
                    suffixes.add(tail)          # `${dataTestId}-input`
                else:
                    static_ids.update(l + tail for l in pool)   # `${action.id}-btn`
            elif len(exprs) >= 2 and len(frags) > 1 and frags[1]:
                prefixes.update(l + frags[1] for l in pool)     # `{testid}-{opt.value}`
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

# A shared component's suffix applies to every id a caller can hand it. Which
# caller uses which component is not tracked, so this is a permissive fallback:
# it clears exact-id lookups only, never a prefix step (see prefix_ok).
composed_ids = {base + suf for base in static_ids for suf in suffixes}

def testid_ok(t):
    if t in static_ids or t in composed_ids: return True
    return any(p and t.startswith(p) for p in prefixes)

def prefix_ok(tp):
    """A prefix step selects `[data-testid^="tp"]`, so something must start with
    it. The old rule also accepted `tp.startswith(id)`, which let a stale
    `set-default-` pass against the plain `set-default` it can never match."""
    if any(s.startswith(tp) for s in static_ids): return True
    return any(p and (p.startswith(tp) or tp.startswith(p)) for p in prefixes)

# ---- Validate scripts ----
problems = {}
total_steps = 0
for f in sorted(glob.glob(f"{SCRIPTS}/*.json")):
    name = os.path.basename(f)
    errs = []
    try:
        d = json.load(open(f))
    except Exception as e:
        problems[name] = [f"JSON parse error: {e}"]; continue
    steps = d.get("steps", [])
    total_steps += len(steps)
    for idx, s in enumerate(steps):
        k = s.get("kind")
        if k not in KINDS:
            errs.append(f"step {idx}: illegal kind {k!r}")
            continue
        if k == "goto":
            r = s.get("route")
            if r not in ROUTES:
                errs.append(f"step {idx}: illegal route {r!r}")
        if "testid" in s and s["testid"] is not None:
            if not testid_ok(s["testid"]):
                errs.append(f"step {idx} ({k}): unknown testid {s['testid']!r}")
        if "testidPrefix" in s and s["testidPrefix"] is not None:
            if not prefix_ok(s["testidPrefix"]):
                errs.append(f"step {idx} ({k}): unknown testidPrefix {s['testidPrefix']!r}")
    if errs:
        problems[name] = errs

print(f"corpus: {len(static_ids)} resolved testids, {len(prefixes)} dynamic prefixes, "
      f"{len(suffixes)} composed suffixes ({len(composed_ids)} composed ids)")
print(f"scripts: {len(glob.glob(f'{SCRIPTS}/*.json'))}, total steps: {total_steps}")
print()
if not problems:
    print("ALL CLEAN: 0 static issues")
else:
    for name, errs in problems.items():
        print(f"### {name} ({len(errs)})")
        for e in errs[:40]:
            print("  -", e)
    print(f"\nTOTAL scripts with issues: {len(problems)}")
