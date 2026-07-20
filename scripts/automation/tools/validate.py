#!/usr/bin/env python3
"""Static validator for the desktop automation scripts.
Checks: JSON parses, every step kind legal, every goto route legal, every
testid/testidPrefix resolvable against the UI source (static literals + dynamic
prefixes). Heuristic on dynamic ids, tuned to catch typos / removed testids."""
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
static_ids = set()   # fully-literal testids
prefixes = set()     # leading literal segment of a dynamic testid

# match data-testid / testid / testId / dataTestId attribute or prop values
attr = re.compile(r'(?:data-testid|dataTestId|testId|testid)\s*[=:]\s*(?P<q>["\'`{])')

def add_value(raw):
    """raw is the char sequence after the opening quote/brace up to close."""
    # dynamic if it contains { or ${ or + concatenation
    if '${' in raw or re.search(r'\{[^}]', raw) or '+' in raw:
        # leading literal before first { or $ or +
        m = re.match(r'^([A-Za-z0-9_\-./:]*)', raw)
        pre = m.group(1) if m else ''
        # strip trailing '-' kept (prefix like "provider-toggle-")
        if pre:
            prefixes.add(pre)
    else:
        v = raw.strip()
        if re.fullmatch(r'[A-Za-z0-9_\-./:]+', v):
            static_ids.add(v)

for path in glob.glob(f"{UI}/**/*.svelte", recursive=True) + \
            glob.glob(f"{UI}/**/*.ts", recursive=True):
    try:
        txt = open(path, encoding="utf-8").read()
    except Exception:
        continue
    for m in re.finditer(r'(?:data-testid|dataTestId|testId|testid)\s*[=:]\s*', txt):
        i = m.end()
        if i >= len(txt): continue
        c = txt[i]
        if c in '"\'':
            j = txt.find(c, i+1)
            if j > 0: add_value(txt[i+1:j])
        elif c == '`':
            j = txt.find('`', i+1)
            if j > 0: add_value(txt[i+1:j])
        elif c == '{':
            # attribute {expr}: read balanced-ish up to matching close on same-ish
            depth=0; j=i
            while j < len(txt):
                if txt[j]=='{': depth+=1
                elif txt[j]=='}':
                    depth-=1
                    if depth==0: break
                j+=1
            inner = txt[i+1:j]
            # inner may be `tpl`, "lit", 'lit', or var
            inner=inner.strip()
            if inner and inner[0] in '"\'`':
                add_value(inner[1:-1] if len(inner)>=2 else inner)
            else:
                # pure expression like {dataTestId} or {rowId} -> unknown, treat as wildcard prefix ''
                prefixes.add('')  # will make any testid "plausible"; noted separately
    # TabBar/FilterChipBar: a `testidPrefix="X"` prop yields child ids
    # `X-tabbar`, `X-tab-<key>` (TabBar) and `X-<key>` (FilterChipBar).
    # SplitLayout: sidebarTestid="X" / detailTestid="X" render as data-testid.
    for pm in re.finditer(r'(?:sidebarTestid|detailTestid)\s*=\s*["\']([A-Za-z0-9_\-./:]+)["\']', txt):
        static_ids.add(pm.group(1))
    for pm in re.finditer(r'testidPrefix\s*=\s*["\']([A-Za-z0-9_\-./:]+)["\']', txt):
        X = pm.group(1)
        static_ids.add(f"{X}-tabbar")
        prefixes.add(f"{X}-tab-")
        prefixes.add(f"{X}-")
    # also plain string literals that look like testids used in .ts (FirstConnectionTour etc.)

def testid_ok(t):
    if t in static_ids: return True
    for p in prefixes:
        if p and t.startswith(p): return True
        if p and p.startswith(t): return True
    # some static id is a longer form
    for s in static_ids:
        if s.startswith(t + '-') or t.startswith(s + '-'): return True
    return False

def prefix_ok(tp):
    for s in static_ids:
        if s.startswith(tp) or tp.startswith(s): return True
    for p in prefixes:
        if p and (p.startswith(tp) or tp.startswith(p)): return True
    return False

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

print(f"corpus: {len(static_ids)} static testids, {len(prefixes)} dynamic prefixes")
print(f"scripts: {len(glob.glob(f'{SCRIPTS}/*.json'))}, total steps: {total_steps}")
print(f"'' wildcard-prefix present: {'' in prefixes} (expression-built ids -> permissive)")
print()
if not problems:
    print("ALL CLEAN: 0 static issues")
else:
    for name, errs in problems.items():
        print(f"### {name} ({len(errs)})")
        for e in errs[:40]:
            print("  -", e)
    print(f"\nTOTAL scripts with issues: {len(problems)}")
