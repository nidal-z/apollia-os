#!/usr/bin/env python3
"""Report the desktop gestures no automation recipe ever plays, by page.

`validate.py` answers "does every step resolve against the UI?". This answers
the opposite question: "which anchors of the UI does no step ever touch?", and
it separates the two things the corpus mixes.

  A GESTURE is an anchor a human can act on: the id sits on a native control
  (button, input, select, textarea, a, summary), on an element carrying an
  event handler or an interactive `role`, or on a component whose own root is
  one of those (Button, Input, Toggle, Checkbox, Select, Textarea, ListRow...).
  A MARKER is an anchor that only exists to be read: a card, a banner, a
  section, a dialog shell, an empty state. A marker is covered when a step
  waits for it or asserts on it; a gesture is only covered when a step ACTS on
  it (click, fill, setChecked, selectOption, press, or the implicit
  chat-input / chat-send-button of `sendChat`).

Counting the two together is what makes a coverage figure meaningless: a page
whose every banner is asserted and whose every button is untouched reads as
half covered.

The `prefix` column is kept apart from `played` for the same reason: a
`testidPrefix` step selects `[data-testid^="..."]` and the runner acts on the
FIRST match, so a prefix that matches eight anchors proves one of them was
played and says nothing about the other seven. Counting those eight as covered
is exactly the mistake that reported connector operations as exercised when the
true answer was zero.

Three origins are reported apart, because they are not equally certain:

  literal   the id is written as a literal in the source. It renders. This is
            the only bucket the headline counts.
  derived   the id is built from data inside its own file (`${action.id}-btn`)
            and is reconstructed from that file's string literals, exactly as
            `validate.py` does. The reconstruction over-generates: a class name
            caught in the same literal pool produces an id nothing renders.
  composed  a shared component appends a suffix to the id it is handed
            (`${dataTestId}-input`). Reported per call site rather than over
            the whole file, which is why this tool counts far fewer of them
            than the validation corpus does.

Exit codes: 0 measured and within budget, 1 a defect (uncovered gestures above
`--max-uncovered`, or an anchor this tool sees that the validation corpus does
not), 2 nothing measured (no site, no script: an empty measurement is not a
pass).
"""
import argparse
import glob
import json
import os
import re
import sys
from collections import defaultdict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from validate import UI, SCRIPTS, build_corpus, corpus_files, raw_values, slots_of  # noqa: E402

# Anchors carried by a native control are gestures without further analysis.
NATIVE_INTERACTIVE = {"button", "input", "textarea", "select", "a", "summary", "option"}
# An ARIA role that names a control the operator drives.
ROLE_INTERACTIVE = re.compile(
    r'role\s*=\s*["\'](button|link|menuitem|menuitemcheckbox|menuitemradio|tab|switch'
    r'|checkbox|radio|option|combobox|slider|textbox|searchbox)["\']'
)
# A container role: the element wraps a surface, the handler on it is a
# backdrop dismissal rather than the gesture the anchor names.
ROLE_CONTAINER = re.compile(
    r'role\s*=\s*["\'](presentation|none|dialog|alertdialog|alert|note|status|list'
    r'|listitem|group|region|log|toolbar|tablist|radiogroup|menu)["\']'
)
# A modal shell forwards its caller's role and traps the keyboard: the handlers
# on it dismiss the surface, they are not the gesture the anchor names.
CONTAINER_HINT = re.compile(r'aria-modal|\{\s*role\s*\}|role\s*=\s*\{\s*role\s*\}')
HANDLER = re.compile(r'\bon(click|change|input|keydown|keyup|submit|pointerdown|mousedown)\s*=')
# `onclick={onclick}` / `role={onclick ? "button" : undefined}`: the element is
# a control only when its caller hands it a handler, so the call site decides.
HANDLER_FORWARDED = re.compile(r'\bon(?:click|change|input|keydown|submit)\s*=\s*\{\s*(?:on\w+|\w+\s*\?)')
ROLE_FORWARDED = re.compile(r'role\s*=\s*\{[^}]*\?')
SPREAD = re.compile(r'\{\s*\.\.\.\s*(?:restProps|rest|props)\b')
ATTR = re.compile(r'(?:data-testid|dataTestId|testId|testid)\s*[=:]\s*')
PREFIX_ID = r'[A-Za-z0-9_\-./:]+'
BARE_FORWARD = re.compile(r'\{\s*[A-Za-z_$][\w$.?]*\s*\}')

# Kinds that ACT on their target. `sendChat` acts without naming a target: the
# runner types into `chat-input` and clicks `chat-send-button`.
ACTING_KINDS = {"click", "fill", "setChecked", "selectOption", "press"}
SENDCHAT_TARGETS = ("chat-input", "chat-send-button")
# `awaitTurn` accepts whatever approval card is on screen, by prefix.
AWAITTURN_PREFIXES = ("approval-accept-", "operator-approval-accept-")
OBSERVING_KINDS = {"waitFor", "waitGone", "expect", "captureText"}

INTERACTIVE, MARKER, CONDITIONAL = "gesture", "marker", "conditional"


def tag_extent(txt, start):
    """The opening tag beginning at `start`, quote and brace aware.

    A naive scan to the next '>' stops inside `class={cn("a > b")}` and inside
    every ternary, which is most of this codebase.
    """
    k, depth, n = start, 0, len(txt)
    while k < n:
        c = txt[k]
        if c in "\"'`":
            quote, k = c, k + 1
            while k < n and txt[k] != quote:
                if txt[k] == "\\":
                    k += 1
                k += 1
        elif c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
        elif c == ">" and depth == 0:
            return txt[start:k + 1]
        k += 1
    return txt[start:start + 600]


BLOCK_COMMENT = re.compile(r'/\*.*?\*/|<!--.*?-->', re.S)
LINE_COMMENT = re.compile(r'^[ \t]*(?://|\*).*$', re.M)


def blank_comments(txt):
    """Same text, comment bodies replaced by spaces (offsets preserved).

    A doc-comment that shows how to call a component (`<Disclosure
    testid="details">`) renders nothing, and counting it invented an anchor.
    Line comments are only blanked when the line IS a comment, so a `//` inside
    a URL literal is left alone.
    """
    out = list(txt)
    for rx in (BLOCK_COMMENT, LINE_COMMENT):
        for m in rx.finditer(txt):
            for k in range(m.start(), m.end()):
                if out[k] != "\n":
                    out[k] = " "
    return "".join(out)


def classify_element(tag, tag_txt):
    """Tri-state verdict for one element carrying an anchor."""
    if tag in NATIVE_INTERACTIVE:
        return INTERACTIVE
    if ROLE_INTERACTIVE.search(tag_txt):
        return INTERACTIVE
    if ROLE_FORWARDED.search(tag_txt) or HANDLER_FORWARDED.search(tag_txt):
        return CONDITIONAL
    if HANDLER.search(tag_txt):
        container = ROLE_CONTAINER.search(tag_txt) or CONTAINER_HINT.search(tag_txt)
        return MARKER if container else INTERACTIVE
    return None  # not decided here: a component tag, or a plain container


def scan_sites(ui_root=UI):
    """Every place the UI writes an anchor, with the element that carries it.

    Returns (sites, carriers). A site is a dict; carriers maps a component
    basename to the elements that receive the id it is handed (a forwarded
    `data-testid={testid}` or a `{...restProps}` spread).
    """
    sites = []
    carriers = defaultdict(list)
    for path in corpus_files(ui_root):
        try:
            txt = blank_comments(open(path, encoding="utf-8").read())
        except OSError:
            continue
        component = os.path.splitext(os.path.basename(path))[0]
        pool = set(re.findall(r'[(,:]\s*["\']([A-Za-z0-9_\-]{2,60})["\']', txt))
        props = set()
        for pm in re.finditer(r'let\s*\{(.*?)\}\s*(?::\s*\w+\s*)?=\s*\$props\(\)', txt, re.S):
            props.update(re.findall(r'([A-Za-z_$][\w$]*)\s*(?:[,}]|=)', pm.group(1)))
            props.update(re.findall(r':\s*([A-Za-z_$][\w$]*)', pm.group(1)))
        for m in re.finditer(SPREAD, txt):
            j = txt.rfind("<", 0, m.start())
            if j < 0:
                continue
            tag_txt = tag_extent(txt, j)
            tm = re.match(r'<\s*([A-Za-z][\w.\-]*)', tag_txt)
            if tm:
                carriers[component].append((tm.group(1), tag_txt))
        for m in ATTR.finditer(txt):
            j = txt.rfind("<", 0, m.start())
            if j < 0:
                continue
            tag_txt = tag_extent(txt, j)
            tm = re.match(r'<\s*([A-Za-z][\w.\-]*)', tag_txt)
            tag = tm.group(1) if tm else "?"
            line = txt.count("\n", 0, m.start()) + 1
            value_txt = txt[m.end():m.end() + 80]
            if BARE_FORWARD.match(value_txt) and re.match(r'\{\s*([A-Za-z_$][\w$]*)\s*\}', value_txt) \
                    and re.match(r'\{\s*([A-Za-z_$][\w$]*)\s*\}', value_txt).group(1) in props:
                carriers[component].append((tag, tag_txt))
            for raw in raw_values(txt, m.end()):
                sites.append({
                    "path": path, "line": line, "tag": tag, "tag_txt": tag_txt,
                    "raw": raw, "component": component, "pool": pool, "props": props,
                })
        # A tab bar and a filter chip bar name their children off a
        # `testidPrefix` prop, so the children are anchors no site of this file
        # spells out: TabBar renders `X-tabbar` (the tablist) and one
        # `X-tab-<key>` per tab, FilterChipBar one `X-<key>` per chip.
        for pm in re.finditer(f'testidPrefix\\s*=\\s*["\\\']({PREFIX_ID})["\\\']', txt):
            j = txt.rfind("<", 0, pm.start())
            tm = re.match(r'<\s*([A-Za-z][\w.\-]*)', txt[j:j + 60]) if j >= 0 else None
            owner = tm.group(1) if tm else "?"
            line = txt.count("\n", 0, pm.start()) + 1
            sites.append({
                "path": path, "line": line, "tag": owner, "tag_txt": "",
                "raw": None, "component": component, "pool": pool, "props": props,
                "prefix_prop": pm.group(1),
            })
    return sites, carriers


def resolve_components(carriers):
    """Verdict per component: interactive, marker, or decided by the call site."""
    verdict = {}

    def resolve(name, seen=()):
        if name in verdict:
            return verdict[name]
        if name in seen:
            return MARKER
        found = set()
        for tag, tag_txt in carriers.get(name, ()):
            v = classify_element(tag, tag_txt)
            if v is None and tag[:1].isupper():
                v = resolve(tag.split(".")[0], seen + (name,))
            found.add(v or MARKER)
        if not found:
            return None
        if found == {INTERACTIVE}:
            out = INTERACTIVE
        elif INTERACTIVE in found or CONDITIONAL in found:
            out = CONDITIONAL
        else:
            out = MARKER
        verdict[name] = out
        return out

    for name in list(carriers):
        resolve(name)
    return verdict


def site_kind(site, components):
    """Gesture or marker, for one anchor site."""
    tag, tag_txt = site["tag"], site["tag_txt"]
    direct = classify_element(tag, tag_txt)
    if direct in (INTERACTIVE, MARKER):
        return INTERACTIVE if direct == INTERACTIVE else MARKER
    if tag[:1].isupper():
        v = components.get(tag.split(".")[0])
        if v == INTERACTIVE:
            return INTERACTIVE
        if v == CONDITIONAL:
            # The caller decides: a Card or a ListRow is a control only when it
            # is handed a handler.
            return INTERACTIVE if HANDLER.search(tag_txt) else MARKER
        if v == MARKER:
            return MARKER
        # A component this tool never saw render an anchor (a third-party
        # primitive). Its name is the only evidence left.
        if re.search(r'(Item|Trigger|Button|Link|Option|Tab|Row|Toggle|Input)$', tag):
            return INTERACTIVE
        return MARKER
    if direct == CONDITIONAL:
        return MARKER
    return MARKER


def page_of(path):
    """The page a file's anchors belong to, from the tree's own layout."""
    rel = path.replace(os.sep, "/").split("/src/", 1)[-1]
    m = re.match(r'routes/settings/([\w\-]+)\.svelte', rel)
    if m:
        return f"settings/{m.group(1).lower()}"
    m = re.match(r'routes/([\w\-]+)\.svelte', rel)
    if m:
        return m.group(1).lower()
    m = re.match(r'components/([\w\-]+)/', rel)
    if m:
        return m.group(1).lower()
    m = re.match(r'lib/components/([\w\-]+)/', rel)
    if m:
        return f"shared/{m.group(1).lower()}"
    m = re.match(r'lib/([\w\-]+)/', rel)
    if m:
        return f"shared/{m.group(1).lower()}"
    return "shared/app"


def anchors_from_sites(sites, components):
    """Turn sites into anchors: literal ids, derived ids, prefix families.

    Composed ids (a shared component appending `-input` to the id of THIS site)
    are attached to the site that hands the id over, never to the whole file.
    """
    suffixes = defaultdict(set)   # component -> tails it appends, with a kind
    suffix_kind = {}
    for site in sites:
        raw = site["raw"]
        if raw is None or not ("${" in raw or re.search(r'\{[^}]', raw)):
            continue
        frags, exprs = slots_of(raw)
        if frags and frags[0]:
            continue
        if len(exprs) == 1 and exprs[0] in site["props"]:
            tail = "".join(f for f in frags[1:] if f)
            if tail:
                suffixes[site["component"]].add(tail)
                suffix_kind[(site["component"], tail)] = site_kind(site, components)

    anchors = {}      # id -> record
    families = []     # dynamic prefixes, one record each
    unresolved = []   # a site whose id this tool cannot name at all

    def add(anchor_id, site, origin, kind=None):
        rec = anchors.get(anchor_id)
        kind = kind or site_kind(site, components)
        if rec is None:
            anchors[anchor_id] = {
                "id": anchor_id, "kind": kind, "origin": origin,
                "page": page_of(site["path"]), "file": site["path"], "line": site["line"],
            }
            return
        # A gesture wins over a marker: the same id rendered twice, once on a
        # button, is reachable as a gesture.
        if rec["kind"] == MARKER and kind == INTERACTIVE:
            rec.update(kind=kind, page=page_of(site["path"]), file=site["path"], line=site["line"])
        if rec["origin"] != "literal" and origin == "literal":
            rec["origin"] = "literal"

    for site in sites:
        if site.get("prefix_prop"):
            base = site["prefix_prop"]
            if site["tag"] == "TabBar":
                anchors.setdefault(f"{base}-tabbar", {
                    "id": f"{base}-tabbar", "kind": MARKER, "origin": "literal",
                    "page": page_of(site["path"]), "file": site["path"], "line": site["line"],
                })
                families.append({"prefix": f"{base}-tab-", "kind": INTERACTIVE,
                                 "page": page_of(site["path"]), "file": site["path"],
                                 "line": site["line"]})
            else:
                families.append({"prefix": f"{base}-", "kind": INTERACTIVE,
                                 "page": page_of(site["path"]), "file": site["path"],
                                 "line": site["line"]})
            continue
        raw = site["raw"]
        if raw is None:
            unresolved.append(site)
            continue
        templated = "${" in raw or re.search(r'\{[^}]', raw) or "+" in raw
        if not templated:
            value = raw.strip()
            if not re.fullmatch(r'[A-Za-z0-9_\-./:]+', value):
                continue
            add(value, site, "literal")
            for tail in suffixes.get(site["tag"].split(".")[0], ()):
                add(value + tail, site, "composed",
                    suffix_kind.get((site["tag"].split(".")[0], tail)))
            continue
        frags, exprs = slots_of(raw)
        if frags and frags[0]:
            families.append({
                "prefix": frags[0], "kind": site_kind(site, components),
                "page": page_of(site["path"]), "file": site["path"], "line": site["line"],
            })
            continue
        if not exprs:
            continue
        tail = "".join(f for f in frags[1:] if f)
        if len(exprs) == 1 and exprs[0] in site["props"]:
            continue  # a component suffix, already collected
        if len(exprs) == 1 and tail:
            for lit in site["pool"]:
                add(lit + tail, site, "derived")
                for stail in suffixes.get(site["tag"].split(".")[0], ()):
                    add(lit + tail + stail, site, "composed",
                        suffix_kind.get((site["tag"].split(".")[0], stail)))
        else:
            unresolved.append(site)
    return anchors, families, unresolved


def read_scripts(script_dir=SCRIPTS):
    """What the recipes act on, and what they merely look at."""
    acted, observed, acted_prefix, observed_prefix = set(), set(), set(), set()
    files = sorted(glob.glob(f"{script_dir}/*.json"))
    parsed = 0
    for path in files:
        try:
            data = json.load(open(path, encoding="utf-8"))
        except (OSError, ValueError):
            continue
        parsed += 1
        for step in data.get("steps", []):
            if not isinstance(step, dict):
                continue
            kind = step.get("kind")
            if kind == "sendChat":
                acted.update(SENDCHAT_TARGETS)
                continue
            if kind == "awaitTurn":
                acted_prefix.update(AWAITTURN_PREFIXES)
                continue
            tid, tprefix = step.get("testid"), step.get("testidPrefix")
            if kind in ACTING_KINDS:
                bucket, pbucket = acted, acted_prefix
            elif kind in OBSERVING_KINDS:
                bucket, pbucket = observed, observed_prefix
            else:
                continue
            if tid:
                bucket.add(tid)
            if tprefix:
                pbucket.add(tprefix)
    return acted, observed, acted_prefix, observed_prefix, files, parsed


def status_of(anchor_id, acted, observed, acted_prefix, observed_prefix):
    """played | prefix | seen | never, for one anchor."""
    if anchor_id in acted:
        return "played"
    if any(anchor_id.startswith(p) for p in acted_prefix):
        return "prefix"
    if anchor_id in observed or any(anchor_id.startswith(p) for p in observed_prefix):
        return "seen"
    return "never"


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--ui", default=UI, help="UI source root to read anchors from")
    ap.add_argument("--scripts", default=SCRIPTS, help="directory of automation recipes")
    ap.add_argument("--origin", default="literal",
                    choices=["literal", "literal+composed", "all"],
                    help="which anchors the headline counts (default: literal)")
    ap.add_argument("--page", help="restrict the listing to one page")
    ap.add_argument("--list", action="store_true", help="list the uncovered gestures")
    ap.add_argument("--markers", action="store_true", help="report markers too")
    ap.add_argument("--families", action="store_true",
                    help="report the dynamic prefix families and their coverage")
    ap.add_argument("--by-family", action="store_true",
                    help="group by id family (first segment) instead of by page")
    ap.add_argument("--json", action="store_true", help="machine-readable output")
    ap.add_argument("--max-uncovered", type=int, default=None,
                    help="guard mode: exit 1 when more gestures than this are never played")
    args = ap.parse_args()

    static_ids, prefixes, composed_ids, _ = build_corpus(args.ui)
    corpus_universe = static_ids | composed_ids
    sites, carriers = scan_sites(args.ui)
    components = resolve_components(carriers)
    anchors, families, unresolved = anchors_from_sites(sites, components)
    acted, observed, acted_prefix, observed_prefix, files, parsed = read_scripts(args.scripts)

    if not sites or not anchors or not files or not parsed:
        print("NOTHING MEASURED: no anchor site or no script "
              "(run from the repository root)")
        return 2

    keep = {
        "literal": {"literal"},
        "literal+composed": {"literal", "composed"},
        "all": {"literal", "derived", "composed"},
    }[args.origin]
    selected = [a for a in anchors.values() if a["origin"] in keep]
    for a in selected:
        a["status"] = status_of(a["id"], acted, observed, acted_prefix, observed_prefix)

    # A marker a recipe drives with an acting step is a gesture whatever the
    # static reading said: the corpus is the evidence. This only ever moves an
    # anchor into the played column, so it flatters the ratio and never hides a
    # hole; the count is printed so the reader can take it back out.
    promoted = [a for a in selected
                if a["kind"] == MARKER and a["status"] in ("played", "prefix")]
    for a in promoted:
        a["kind"] = INTERACTIVE
    gestures = [a for a in selected if a["kind"] == INTERACTIVE]
    markers = [a for a in selected if a["kind"] == MARKER]
    outside = sorted(a["id"] for a in selected if a["id"] not in corpus_universe)

    key = (lambda a: a["id"].split("-")[0]) if args.by_family else (lambda a: a["page"])
    groups = defaultdict(lambda: defaultdict(int))
    for a in gestures:
        groups[key(a)][a["status"]] += 1
        groups[key(a)]["total"] += 1

    never = [a for a in gestures if a["status"] == "never"]
    if args.json:
        print(json.dumps({
            "origin": args.origin,
            "sites": len(sites), "anchors": len(anchors), "selected": len(selected),
            "gestures": len(gestures), "markers": len(markers),
            "promoted_by_evidence": len(promoted),
            "played": sum(1 for a in gestures if a["status"] == "played"),
            "prefix": sum(1 for a in gestures if a["status"] == "prefix"),
            "seen": sum(1 for a in gestures if a["status"] == "seen"),
            "never": len(never),
            "families": len(families), "unresolved_sites": len(unresolved),
            "outside_validation_corpus": outside,
            "groups": {g: dict(v) for g, v in groups.items()},
            "never_ids": [{"id": a["id"], "page": a["page"],
                           "file": a["file"].split("/src/")[-1], "line": a["line"]}
                          for a in sorted(never, key=lambda a: (a["page"], a["id"]))],
        }, indent=2))
    else:
        print(f"anchor sites: {len(sites)}   anchors named: {len(anchors)}   "
              f"origin kept: {args.origin} -> {len(selected)}")
        print(f"validation corpus: {len(static_ids)} ids + {len(composed_ids)} composed "
              f"= {len(corpus_universe)} addressable")
        print(f"scripts read: {parsed}/{len(files)}")
        print(f"gestures {len(gestures)} (of which {len(promoted)} promoted by evidence: "
              f"a step acts on them)   markers {len(markers)}")
        print(f"dynamic families {len(families)}   "
              f"sites this tool cannot name {len(unresolved)}")
        print()
        head = "family" if args.by_family else "page"
        print(f"{head:<26} {'gestures':>8} {'played':>7} {'prefix':>7} {'seen':>6} {'never':>6}")
        for g in sorted(groups, key=lambda g: (-groups[g]["never"], g)):
            row = groups[g]
            if args.page and g != args.page:
                continue
            print(f"{g:<26} {row['total']:>8} {row['played']:>7} {row['prefix']:>7} "
                  f"{row['seen']:>6} {row['never']:>6}")
        total = {k: sum(v[k] for v in groups.values())
                 for k in ("total", "played", "prefix", "seen", "never")}
        print(f"{'TOTAL':<26} {total['total']:>8} {total['played']:>7} {total['prefix']:>7} "
              f"{total['seen']:>6} {total['never']:>6}")
        if args.list:
            print("\nnever played:")
            for a in sorted(never, key=lambda a: (a["page"], a["id"])):
                if args.page and a["page"] != args.page:
                    continue
                print(f"  {a['page']:<24} {a['id']:<44} "
                      f"{a['file'].split('/src/')[-1]}:{a['line']}")
        if args.markers:
            never_markers = [a for a in markers if a["status"] == "never"]
            print(f"\nmarkers: {len(markers)}, never asserted: {len(never_markers)}")
        if args.families:
            print("\ndynamic families (an id built from data, covered by a prefix step "
                  "or a dynamicTestids declaration):")
            seen_fam = set()
            for f in sorted(families, key=lambda f: (f["page"], f["prefix"])):
                if f["prefix"] in seen_fam or (args.page and f["page"] != args.page):
                    continue
                seen_fam.add(f["prefix"])
                covered = any(p.startswith(f["prefix"]) or f["prefix"].startswith(p)
                              for p in acted_prefix) or \
                    any(i.startswith(f["prefix"]) for i in acted)
                print(f"  {'played' if covered else 'never ':<7} {f['kind']:<8} "
                      f"{f['prefix']:<40} {f['page']}")
        if outside:
            print(f"\nDEFECT: {len(outside)} anchor(s) this tool resolves and the "
                  f"validation corpus does not: {', '.join(outside[:10])}")

    if outside:
        return 1
    if args.max_uncovered is not None and len(never) > args.max_uncovered:
        if not args.json:
            print(f"\nDEFECT: {len(never)} gestures never played, budget is "
                  f"{args.max_uncovered}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
