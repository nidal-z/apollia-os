#!/usr/bin/env python3
"""Hold every persisted serde type to zero fragility: a file written under
`~/.apollia` must survive the addition of a field.

A struct is evolution-safe when the container carries `#[serde(default)]`,
when every field carries `#[serde(default ...)]`, is `Option<_>` or is
flattened, or when the type carries an explicit version field
(`schema_version` / `format_version`; a bare `version` is the version of the
thing described, not of the format) the reader can branch on. Otherwise a file
written by an older binary fails to parse the day a required field is added,
and the failure lands on the operator's profile, not in a test.

Scope is the MANIFEST below: the structured files the product writes under
`~/.apollia`, each mapped to the module and the structs that shape it. The
manifest is the inventory nobody kept before this guard existed; a new
persisted file starts by adding its row here. Caches the product rebuilds on
its own are listed with `cache=True` and reported separately (a fragile cache
costs one refetch, not a broken profile).

Held at zero: before this guard, 9 of these structs were fragile, every one a
bare `#[derive(Deserialize)]` with required fields and no version.

Exit codes:
  0  every persisted struct is evolution-safe
  1  at least one non-cache struct is fragile
  2  nothing measured (a manifest module is missing)

Usage:
    python3 scripts/check_serde_persisted_defaults.py [--json] [--selftest]
"""

import argparse
import glob
import json
import os
import re
import sys

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# file under ~/.apollia, module that shapes it, structs (None = all), rebuildable cache?
MANIFEST = [
    ("apollia.toml", "crates/apollia-core/src/config/*.rs", None, False),
    ("agents/community/registry.json", "crates/apollia-runtime/src/agents/registry_remote.rs", ["RegistryEntry"], False),
    ("mcp-overrides.json", "crates/apollia-desktop/src/mcp/overrides.rs", ["McpOverrides"], False),
    ("mcp.toml", "crates/apollia-mcp/src/config.rs", ["McpConfig", "McpServerConfig"], False),
    ("models/sampling-defaults.json", "crates/apollia-llm/src/model_defaults/mod.rs", ["UserOverrides", "ModelDefaults"], False),
    ("drive-prefs.toml", "crates/apollia-auth/src/drive_prefs.rs", ["DrivePrefsFile", "AccountDrivePref", "PickedFolder"], False),
    ("oauth-clients.toml", "crates/apollia-auth/src/oauth_clients_file.rs", ["OAuthClientsFile", "OAuthClientEntry"], False),
    ("connectors-index.json", "crates/apollia-auth/src/multi_account.rs", ["AccountIndex"], False),
    ("agents/<name>/manifest.toml", "crates/apollia-core/src/manifest.rs", ["AgentManifest"], False),
    ("memory export (apollia memory export)", "crates/apollia-aip/src/memory.rs", None, False),
    ("mcp-registry.json", "crates/apollia-desktop/src/mcp/registry_client.rs", ["RegistryListResponse", "RegistryServer"], True),
    ("link_previews.json", "crates/apollia-desktop/src/commands/link_preview.rs", ["CachedPreview", "LinkPreview"], True),
]

DERIVE_RE = re.compile(r"#\[derive\(([^)]*)\)\]")
VERSION_FIELD_RE = re.compile(r"^\s*(pub(\([a-z]+\))?\s+)?(schema_version|format_version)\s*:", re.M)


def production_text_of(text):
    text = re.sub(r"//[^\n]*", lambda m: " " * len(m.group(0)), text)
    # Cut only an inline test module (`mod x {`): a `mod x;` declaration near
    # the top of a file must not drop the production code below it.
    m = re.search(r"#\[cfg\(test\)\]\s*(pub\s+)?mod\s+\w+\s*\{", text)
    return text[: m.start()] if m else text


def structs(text):
    for m in DERIVE_RE.finditer(text):
        if "Deserialize" not in m.group(1):
            continue
        k = m.end()
        attrs = ""
        while True:
            n = re.match(r"\s*(#\[[^\]]*\])", text[k:])
            if not n:
                break
            attrs += n.group(1)
            k += n.end()
        item = re.match(r"\s*(pub(\([a-z]+\))?\s+)?(struct|enum)\s+([A-Za-z0-9_]+)", text[k:])
        if not item or item.group(3) != "struct":
            continue
        name = item.group(4)
        b = text.find("{", k + item.end())
        semi = text.find(";", k + item.end())
        if b < 0 or (0 <= semi < b):
            continue
        depth = 0
        for i in range(b, len(text)):
            if text[i] == "{":
                depth += 1
            elif text[i] == "}":
                depth -= 1
                if depth == 0:
                    yield name, attrs, text[b + 1 : i], text.count("\n", 0, m.start()) + 1
                    break


def required_fields(body):
    out = []
    pending = False
    for line in body.split("\n"):
        s = line.strip()
        if s.startswith("#["):
            if "serde(" in s and ("default" in s or "flatten" in s):
                pending = True
            continue
        fm = re.match(r"(pub(\([a-z]+\))?\s+)?([a-z_][a-z0-9_]*)\s*:\s*(.+)", s)
        if fm:
            if not pending and not fm.group(4).startswith("Option<"):
                out.append(fm.group(3))
            pending = False
    return out


def judge_struct(name, attrs, body):
    container_default = bool(re.search(r"serde\([^)]*\bdefault\b", attrs))
    versioned = bool(VERSION_FIELD_RE.search(body))
    req = required_fields(body)
    return {
        "struct": name,
        "container_default": container_default,
        "versioned": versioned,
        "required": req,
        "safe": container_default or versioned or not req,
    }


def run(as_json):
    rows = []
    for file_name, module_glob, wanted, cache in MANIFEST:
        paths = sorted(
            p
            for p in glob.glob(os.path.join(REPO_ROOT, module_glob))
            if not p.endswith(("tests.rs", "_tests.rs"))
        )
        if not paths:
            print(f"manifest module missing: {module_glob}", file=sys.stderr)
            return 2
        for p in paths:
            text = production_text_of(open(p, encoding="utf-8", errors="replace").read())
            for name, attrs, body, line in structs(text):
                if wanted is not None and name not in wanted:
                    continue
                row = judge_struct(name, attrs, body)
                row.update({"file": file_name, "module": os.path.relpath(p, REPO_ROOT), "line": line, "cache": cache})
                rows.append(row)
    if not rows:
        print("nothing measured", file=sys.stderr)
        return 2
    bad = [r for r in rows if not r["safe"] and not r["cache"]]
    bad_cache = [r for r in rows if not r["safe"] and r["cache"]]
    if as_json:
        print(json.dumps(rows, indent=1))
    else:
        print(f"persisted structs inspected: {len(rows)} across {len(MANIFEST)} files")
        for r in rows:
            if not r["safe"]:
                mark = "fragile (cache)" if r["cache"] else "FRAGILE"
                print(f"  {r['file']:40s} {r['struct']:26s} required: {','.join(r['required'])}  <- {mark}")
        print(f"fragile persisted structs: {len(bad)} (plus {len(bad_cache)} in rebuildable caches)")
    print(f"verdict: {'RED' if bad else 'GREEN'} ({len(bad)} fragile struct(s))", file=sys.stderr if as_json else sys.stdout)
    return 1 if bad else 0


# ── Selftest ─────────────────────────────────────────────────────────────────

FRAGILE_SAMPLE = """\
#[derive(Debug, Serialize, Deserialize)]
pub struct Entry {
    pub name: String,
    pub value: String,
}
"""

VERSIONED_SAMPLE = """\
#[derive(Debug, Serialize, Deserialize)]
pub struct Entry {
    #[serde(default = "default_format_version")]
    pub format_version: u32,
    pub name: String,
}
"""

DEFAULTED_SAMPLE = """\
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Entry {
    pub name: String,
}
"""


def selftest():
    failures = []

    frag = [judge_struct(n, a, b) for n, a, b, _l in structs(FRAGILE_SAMPLE)]
    if not frag or frag[0]["safe"]:
        failures.append(f"fragile sample judged safe: {frag}")

    ver = [judge_struct(n, a, b) for n, a, b, _l in structs(VERSIONED_SAMPLE)]
    if not ver or not ver[0]["safe"] or not ver[0]["versioned"]:
        failures.append(f"format_version sample not judged versioned-safe: {ver}")

    dflt = [judge_struct(n, a, b) for n, a, b, _l in structs(DEFAULTED_SAMPLE)]
    if not dflt or not dflt[0]["safe"] or not dflt[0]["container_default"]:
        failures.append(f"container-default sample not judged safe: {dflt}")

    if failures:
        for msg in failures:
            print(f"  FAIL  {msg}")
        print("selftest verdict: RED")
        return 1
    print("  ok    fragile sample fires, versioned and container-default samples pass")
    print("selftest verdict: GREEN")
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--selftest", action="store_true", help="drive the rule on fixtures, red first")
    ns = ap.parse_args()
    if ns.selftest:
        return selftest()
    return run(ns.json)


if __name__ == "__main__":
    sys.exit(main())
