#!/usr/bin/env python3
"""Hold the curated model table and the publisher lists to what they promise.

`crates/apollia-llm/src/recommend/families.toml` ranks model generations with
scores it says were read from a cited source, and dates it says are the
publisher's own. Its header announced this guard long before it existed: an
entry whose `source` is missing, or whose `released` date lies in the future,
is a number nobody can trace, and it ranks models with the authority of a
measurement.

`crates/apollia-llm/src/recommend/publishers.toml` holds the two lists that
classify a HuggingFace owner: the official publishers and the recognised
converters. A family whose `publisher` is on neither list, or an owner on both,
is a classification the runtime cannot make.

`registry/models/` holds the model cards the catalogue reads once the
release pipeline has gathered and signed them
(`scripts/build_model_registry_index.py`). A card is the text a business user
reads before choosing a model, so a card with a missing field, a missing
language or an em-dash is a defect the user sees, and a card whose declared
capabilities contradict what the runtime computes from the chat template is a
promise the product does not keep.

Rules, each reported with the entry it names:

  families.toml
    * the file parses and declares `schema_version = 1`
    * every family has at least one variant, and every variant a `source`
    * every `released` is an ISO date, not later than today
    * every variant carries a repository: a `repo_patterns` entry on its family
      or an `mlx_repos` entry of its own
    * every `mlx_repos` entry is `owner/name`, and its owner is on one of the
      two lists of publishers.toml
    * every `[[embedding]]` entry has a unique id, a `source`, an ISO
      `released` not later than today, a `repo` that is `owner/name`, a `.gguf`
      `file`, a positive `size_bytes`, `dimensions` and `context_length`, a
      `pooling` among mean, cls and last, both prefixes, and a `sha256` of 64
      lowercase hexadecimal digits: the digest is what the download is checked
      against, so an entry without one could not be verified. Its publisher is
      not held to publishers.toml: the pinned digest, not the owner, is what
      vouches for a community conversion
    * every `[[dictation]]` entry has a unique id, a `source`, an ISO
      `released` not later than today, a `license`, a `repo` that is
      `owner/name`, a `.bin` `file`, a positive `size_bytes`, a `sha256` of 64
      lowercase hexadecimal digits and a `card`, for the same reason
  publishers.toml
    * the file parses and declares `schema_version = 1`
    * both lists are non-empty, and no handle sits on both
    * every `publisher` of families.toml is an official publisher

  registry/models
    * every card file parses, declares `schema_version = 1`, and has a unique
      `id` and a `kind` among language, embeddings and dictation
    * every field is present: label, publisher, licence and its source, a
      `source_url` on huggingface.co, a description in `fr` and in `en`, the
      three capabilities and their source, a positive minimum memory and its
      source, the defaults (with a source as soon as one value is given),
      the author fields (name, type `liste` or `booleen`, values for a list,
      default, explanation in `fr` and in `en`, source) and its repositories
    * no card carries an em-dash
    * a language card states an ISO `released` date not later than today, a
      positive `params_b` and a `usage` among general, code and specialised:
      the first-launch suggestion orders the registry on the first two and
      suggests only the first usage, so a card without them is a model the
      suggestion cannot place
    * a card marked `draft`, a model published to draft for a larger one in
      speculative decoding, is a language card and cites the address that
      shows it: the mark files the model as a draft on the user's machine
    * a card naming a recorded `template` declares the tools and reasoning
      capabilities the runtime computes from it: the markers are read from
      `crates/apollia-llm/src/capabilities.rs`, so the guard and the runtime
      cannot drift. A recorded copy may replace an em-dash inside a template
      comment, the one character the prose guard refuses everywhere, and no
      marker contains one
    * every variant and every `[[embedding]]` entry of families.toml has its
      card
    * every `[[dictation]]` entry names, in `card`, a dictation card whose
      `selection_files` holds the address the entry is downloaded from

Exit codes, read by `just guards`:

  0  every file measured, every rule holds
  1  at least one rule broken
  2  a file is absent, so nothing was measured

`--families PATH`, `--publishers PATH` and `--registry DIR` point the guard at
other files, which is how `--selftest` checks the guard itself against its
faulty fixtures.
"""

import argparse
import datetime
import json
import pathlib
import re
import shutil
import sys
import tempfile
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
RECOMMEND = ROOT / "crates" / "apollia-llm" / "src" / "recommend"
FAMILIES = RECOMMEND / "families.toml"
PUBLISHERS = RECOMMEND / "publishers.toml"
REGISTRY = ROOT / "registry" / "models"
CAPABILITIES = ROOT / "crates" / "apollia-llm" / "src" / "capabilities.rs"
SCHEMA_VERSION = 1


def check_publishers(data: dict) -> tuple[list[str], set[str], set[str], set[str]]:
    """Return the faults, the official names, the official orgs, the converters."""
    faults: list[str] = []
    if data.get("schema_version") != SCHEMA_VERSION:
        faults.append(f"publishers.toml: schema_version is not {SCHEMA_VERSION}")
    official = data.get("official", [])
    converters = data.get("converters", [])
    if not official:
        faults.append("publishers.toml: the official list is empty")
    if not converters:
        faults.append("publishers.toml: the converter list is empty")
    names: set[str] = set()
    orgs: set[str] = set()
    for entry in official:
        name = entry.get("publisher", "")
        if not name:
            faults.append("publishers.toml: an official entry has no publisher")
        names.add(name)
        if not entry.get("orgs"):
            faults.append(f"publishers.toml: {name} lists no organisation")
        orgs.update(o.lower() for o in entry.get("orgs", []))
    conv = {c.lower() for c in converters}
    for both in sorted(orgs & conv):
        faults.append(f"publishers.toml: {both} is both official and converter")
    return faults, names, orgs, conv


def check_families(
    data: dict, names: set[str], known_owners: set[str], today: datetime.date
) -> tuple[list[str], int]:
    """Return the faults and the number of variants measured."""
    faults: list[str] = []
    measured = 0
    if data.get("schema_version") != SCHEMA_VERSION:
        faults.append(f"families.toml: schema_version is not {SCHEMA_VERSION}")
    families = data.get("family", [])
    if not families:
        faults.append("families.toml: no family")
    for family in families:
        fid = family.get("id", "?")
        if family.get("publisher") not in names:
            faults.append(
                f"families.toml: {fid} names publisher {family.get('publisher')!r}, "
                "absent from the official list of publishers.toml"
            )
        released = family.get("released", "")
        try:
            date = datetime.date.fromisoformat(str(released))
        except ValueError:
            faults.append(f"families.toml: {fid} released {released!r} is not an ISO date")
        else:
            if date > today:
                faults.append(f"families.toml: {fid} released {released} is in the future")
        variants = family.get("variant", [])
        if not variants:
            faults.append(f"families.toml: {fid} has no variant")
        for variant in variants:
            measured += 1
            name = variant.get("hf_name", "?")
            if not variant.get("source"):
                faults.append(f"families.toml: {fid} / {name} cites no source")
            mlx = variant.get("mlx_repos", [])
            if not family.get("repo_patterns") and not mlx:
                faults.append(f"families.toml: {fid} / {name} carries no repository")
            for repo in mlx:
                owner, _, rest = repo.partition("/")
                if not owner or not rest or "/" in rest:
                    faults.append(f"families.toml: {fid} / {name} mlx repo {repo!r} is not owner/name")
                elif owner.lower() not in known_owners:
                    faults.append(
                        f"families.toml: {fid} / {name} mlx repo {repo} is owned by "
                        f"{owner}, on neither list of publishers.toml"
                    )
    return faults, measured


EMBEDDING_FIELDS = (
    "id", "label", "publisher", "released", "license", "repo", "file",
    "quantization", "size_bytes", "sha256", "architecture", "dimensions",
    "context_length", "pooling", "query_prefix", "passage_prefix", "source",
)
POOLINGS = {"mean", "cls", "last"}
HEX = set("0123456789abcdef")


def check_embeddings(data: dict, today: datetime.date) -> tuple[list[str], int]:
    """Return the faults of the `[[embedding]]` entries and how many there are."""
    faults: list[str] = []
    entries = data.get("embedding", [])
    seen: set[str] = set()
    for entry in entries:
        eid = entry.get("id", "?")
        if eid in seen:
            faults.append(f"families.toml: embedding {eid} is defined twice")
        seen.add(eid)
        for field in EMBEDDING_FIELDS:
            if field not in entry:
                faults.append(f"families.toml: embedding {eid} has no {field}")
        if not entry.get("source"):
            faults.append(f"families.toml: embedding {eid} cites no source")
        released = entry.get("released", "")
        try:
            date = datetime.date.fromisoformat(str(released))
        except ValueError:
            faults.append(f"families.toml: embedding {eid} released {released!r} is not an ISO date")
        else:
            if date > today:
                faults.append(f"families.toml: embedding {eid} released {released} is in the future")
        owner, _, rest = str(entry.get("repo", "")).partition("/")
        if not owner or not rest or "/" in rest:
            faults.append(f"families.toml: embedding {eid} repo {entry.get('repo')!r} is not owner/name")
        if not str(entry.get("file", "")).endswith(".gguf"):
            faults.append(f"families.toml: embedding {eid} file is not a .gguf")
        for field in ("size_bytes", "dimensions", "context_length"):
            value = entry.get(field)
            if not isinstance(value, int) or value <= 0:
                faults.append(f"families.toml: embedding {eid} {field} is not a positive integer")
        if entry.get("pooling") not in POOLINGS:
            faults.append(f"families.toml: embedding {eid} pooling {entry.get('pooling')!r} is not one of {sorted(POOLINGS)}")
        digest = str(entry.get("sha256", ""))
        if len(digest) != 64 or not set(digest) <= HEX:
            faults.append(f"families.toml: embedding {eid} sha256 is not 64 lowercase hexadecimal digits")
    return faults, len(entries)


DICTATION_FIELDS = (
    "id", "label", "publisher", "released", "license", "repo", "file",
    "quantization", "size_bytes", "sha256", "card", "source",
)


def dictation_url(entry: dict) -> str:
    """The address a `[[dictation]]` entry is downloaded from."""
    return f"https://huggingface.co/{entry.get('repo')}/resolve/main/{entry.get('file')}"


def check_dictation(data: dict, today: datetime.date) -> tuple[list[str], int]:
    """Return the faults of the `[[dictation]]` entries and how many there are."""
    faults: list[str] = []
    entries = data.get("dictation", [])
    seen: set[str] = set()
    for entry in entries:
        did = entry.get("id", "?")
        if did in seen:
            faults.append(f"families.toml: dictation {did} is defined twice")
        seen.add(did)
        for field in DICTATION_FIELDS:
            if not entry.get(field):
                faults.append(f"families.toml: dictation {did} has no {field}")
        released = entry.get("released", "")
        try:
            date = datetime.date.fromisoformat(str(released))
        except ValueError:
            faults.append(f"families.toml: dictation {did} released {released!r} is not an ISO date")
        else:
            if date > today:
                faults.append(f"families.toml: dictation {did} released {released} is in the future")
        owner, _, rest = str(entry.get("repo", "")).partition("/")
        if not owner or not rest or "/" in rest:
            faults.append(f"families.toml: dictation {did} repo {entry.get('repo')!r} is not owner/name")
        if not str(entry.get("file", "")).endswith(".bin"):
            faults.append(f"families.toml: dictation {did} file is not a whisper.cpp .bin")
        size = entry.get("size_bytes")
        if not isinstance(size, int) or size <= 0:
            faults.append(f"families.toml: dictation {did} size_bytes is not a positive integer")
        digest = str(entry.get("sha256", ""))
        if len(digest) != 64 or not set(digest) <= HEX:
            faults.append(f"families.toml: dictation {did} sha256 is not 64 lowercase hexadecimal digits")
    return faults, len(entries)


EM_DASH = chr(0x2014)
CARD_KINDS = {"language", "embeddings", "dictation"}
AUTHOR_TYPES = {"liste", "booleen"}
CARD_USAGES = {"general", "code", "specialised"}
DEFAULT_KEYS = ("temperature", "top_p", "top_k", "min_p", "repetition_penalty", "presence_penalty")


def markers(name: str) -> list[str]:
    """The marker list `name` of capabilities.rs, as the runtime reads it."""
    source = CAPABILITIES.read_text(encoding="utf-8")
    block = re.search(rf"const {name}: &\[&str\] = &\[(.*?)\];", source, re.S)
    if block is None:
        return []
    return [m.lower() for m in re.findall(r'"((?:[^"\\]|\\.)*)"', block.group(1))]


def localized(value: object) -> bool:
    return (
        isinstance(value, dict)
        and isinstance(value.get("fr"), str) and value["fr"].strip() != ""
        and isinstance(value.get("en"), str) and value["en"].strip() != ""
    )


def check_card(path: pathlib.Path, card: dict, tools_m: list[str], reason_m: list[str]) -> list[str]:
    """The faults of one card file."""
    cid = card.get("id") or path.name
    out: list[str] = []
    def fault(msg: str) -> None:
        out.append(f"registry: {cid}: {msg}")
    if card.get("schema_version") != SCHEMA_VERSION:
        fault(f"schema_version is not {SCHEMA_VERSION}")
    if card.get("kind") not in CARD_KINDS:
        fault(f"kind {card.get('kind')!r} is not one of {sorted(CARD_KINDS)}")
    for field in ("id", "label", "publisher"):
        if not str(card.get(field, "")).strip():
            fault(f"has no {field}")
    lic = card.get("license") or {}
    if not lic.get("id") or not lic.get("source"):
        fault("has no licence or no licence source")
    if not str(card.get("source_url", "")).startswith("https://huggingface.co/"):
        fault("source_url is not a huggingface.co page")
    if not localized(card.get("description")):
        fault("description lacks a language (fr and en are both required)")
    caps = card.get("capabilities") or {}
    for flag in ("tools", "reasoning", "vision"):
        if not isinstance(caps.get(flag), bool):
            fault(f"capability {flag} is not declared")
    if not caps.get("source"):
        fault("capabilities cite no source")
    memory = card.get("memory") or {}
    if not isinstance(memory.get("min_gb"), (int, float)) or memory.get("min_gb", 0) <= 0:
        fault("minimum memory is not a positive number")
    if not memory.get("source"):
        fault("minimum memory cites no source")
    defaults = card.get("defaults")
    if not isinstance(defaults, dict) or "source" not in defaults:
        fault("has no defaults block")
    else:
        given = [k for k in DEFAULT_KEYS if k in defaults]
        for key in given:
            if not isinstance(defaults[key], (int, float)):
                fault(f"default {key} is not a number")
        if given and not defaults.get("source"):
            fault("defaults cite no source")
    fields = card.get("author_fields")
    if not isinstance(fields, list):
        fault("has no author_fields list")
        fields = []
    for field in fields:
        name = field.get("name", "?")
        if not field.get("name"):
            fault("an author field has no name")
        if field.get("type") not in AUTHOR_TYPES:
            fault(f"author field {name} type is not one of {sorted(AUTHOR_TYPES)}")
        if field.get("type") == "liste" and not field.get("values"):
            fault(f"author field {name} is a list without values")
        if "default" not in field:
            fault(f"author field {name} has no default")
        if not localized(field.get("explanation")):
            fault(f"author field {name} explanation lacks a language")
        if not field.get("source"):
            fault(f"author field {name} cites no source")
    repos = card.get("repos")
    if not isinstance(repos, list) or not repos:
        fault("lists no repository")
    else:
        for repo in repos:
            owner, _, rest = str(repo).partition("/")
            if not owner or not rest or "/" in rest:
                fault(f"repository {repo!r} is not owner/name")
    if card.get("kind") == "language":
        released = str(card.get("released", ""))
        try:
            day = datetime.date.fromisoformat(released)
        except ValueError:
            fault("a language card has no ISO release date")
        else:
            if day > datetime.date.today():
                fault(f"release date {released} is in the future")
        params = card.get("params_b")
        if not isinstance(params, (int, float)) or isinstance(params, bool) or params <= 0:
            fault("a language card has no positive parameter count")
        if card.get("usage") not in CARD_USAGES:
            fault(f"usage {card.get('usage')!r} is not one of {sorted(CARD_USAGES)}")
    if "draft" in card:
        mark = card["draft"]
        if card.get("kind") != "language":
            fault("is marked as a draft model but is not a language card")
        if not isinstance(mark, dict) or not str(mark.get("source", "")).startswith("https://"):
            fault("is marked as a draft model without the address that shows it")
    template = card.get("template")
    if template is not None:
        tpl = ROOT / template
        if not tpl.is_file():
            fault(f"recorded template {template} is absent")
        else:
            text = tpl.read_text(encoding="utf-8").lower()
            tools = any(m in text for m in tools_m)
            reasoning = any(m in text for m in reason_m)
            if caps.get("tools") is not tools:
                fault(f"declares tools={caps.get('tools')} but the recorded template computes {tools}")
            if caps.get("reasoning") is not reasoning:
                fault(f"declares reasoning={caps.get('reasoning')} but the recorded template computes {reasoning}")
    return out


def check_registry(registry: pathlib.Path, fam: dict) -> tuple[list[str], int, int]:
    """Return the faults of the card files, how many there are, and how many
    were checked against a recorded template."""
    faults: list[str] = []
    tools_m, reason_m = markers("TOOL_MARKERS"), markers("REASONING_MARKERS")
    if not tools_m or not reason_m:
        faults.append("registry: the capability markers of capabilities.rs could not be read")
    ids: set[str] = set()
    kinds: dict[str, str] = {}
    selection_of: dict[str, set[str]] = {}
    files = sorted(registry.rglob("*.json"))
    templated = 0
    for path in files:
        text = path.read_text(encoding="utf-8")
        rel = path.relative_to(registry)
        if EM_DASH in text:
            faults.append(f"registry: {rel} carries an em-dash")
        try:
            card = json.loads(text)
        except json.JSONDecodeError as err:
            faults.append(f"registry: {rel} does not parse: {err}")
            continue
        cid = card.get("id", "")
        if cid in ids:
            faults.append(f"registry: {cid} is defined twice")
        ids.add(cid)
        kinds[cid] = card.get("kind", "")
        selection_of[cid] = set(card.get("selection_files", []))
        if "template" in card:
            templated += 1
        faults += check_card(path, card, tools_m, reason_m)
    for family in fam.get("family", []):
        for variant in family.get("variant", []):
            vid = f"{family.get('id')}/{variant.get('hf_name')}"
            if kinds.get(vid) != "language":
                faults.append(f"registry: variant {vid} has no language card")
    for entry in fam.get("embedding", []):
        if kinds.get(entry.get("id")) != "embeddings":
            faults.append(f"registry: embeddings model {entry.get('id')} has no embeddings card")
    for entry in fam.get("dictation", []):
        did, card = entry.get("id"), entry.get("card")
        if kinds.get(card) != "dictation":
            faults.append(f"registry: dictation model {did} names {card!r}, which is no dictation card")
        elif dictation_url(entry) not in selection_of.get(card, set()):
            faults.append(f"registry: dictation model {did} is not among the selection_files of {card}")
    return faults, len(files), templated


def check_shipped_index() -> list[str]:
    """The index compiled into the application is the one the cards build."""
    sys.path.insert(0, str(ROOT / "scripts"))
    import build_model_registry_index as builder

    shipped = builder.SHIPPED_INDEX
    if not shipped.is_file():
        return [f"{shipped.relative_to(ROOT)} is absent: run build_model_registry_index.py --shipped"]
    if shipped.read_text(encoding="utf-8") != builder.build():
        return [
            f"{shipped.relative_to(ROOT)} no longer matches the cards: "
            "run build_model_registry_index.py --shipped"
        ]
    return []


def run(
    families: pathlib.Path,
    publishers: pathlib.Path,
    today: datetime.date,
    registry: pathlib.Path = REGISTRY,
) -> int:
    for path in (families, publishers):
        if not path.is_file():
            print(f"check-model-families: {path} is absent, nothing measured")
            return 2
    if not registry.is_dir():
        print(f"check-model-families: {registry} is absent, nothing measured")
        return 2
    try:
        pub = tomllib.loads(publishers.read_text(encoding="utf-8"))
        fam = tomllib.loads(families.read_text(encoding="utf-8"))
    except tomllib.TOMLDecodeError as err:
        print(f"check-model-families: RED, a file does not parse: {err}")
        return 1
    faults, names, orgs, conv = check_publishers(pub)
    fam_faults, measured = check_families(fam, names, orgs | conv, today)
    faults += fam_faults
    emb_faults, embeddings = check_embeddings(fam, today)
    faults += emb_faults
    dict_faults, dictations = check_dictation(fam, today)
    faults += dict_faults
    card_faults, cards, templated = check_registry(registry, fam)
    faults += card_faults
    if registry == REGISTRY and publishers == PUBLISHERS:
        faults += check_shipped_index()
    families_count = len(fam.get("family", []))
    for fault in faults:
        print(f"  {fault}")
    if faults:
        print(f"check-model-families: RED, {len(faults)} fault(s) over {measured} variant(s)")
        return 1
    print(
        f"check-model-families: ok, {families_count} families, {measured} variants, "
        f"{embeddings} embeddings models, {dictations} dictation models, "
        f"{len(names)} official publishers, {len(conv)} converters, "
        f"{cards} cards ({templated} checked against a recorded template)"
    )
    return 0


def selftest() -> int:
    """The guard answers 0 on the tree and 1 on each of nine faulty fixtures:
    five faulty tables, and four faulty cards (a missing language, an
    em-dash, a capability the recorded template contradicts, a draft mark
    that cites no address)."""
    today = datetime.date.today()
    future = (today + datetime.timedelta(days=30)).isoformat()
    tree = FAMILIES.read_text(encoding="utf-8")
    first_source = next(line for line in tree.splitlines() if line.startswith("source = "))
    first_released = next(line for line in tree.splitlines() if line.startswith("released = "))
    first_digest = next(line for line in tree.splitlines() if line.startswith("sha256 = "))
    first_card = next(line for line in tree.splitlines() if line.startswith("card = "))
    dictation_part = tree[tree.index("\n[[dictation]]\n"):]
    first_dictation_file = next(
        line for line in dictation_part.splitlines() if line.startswith("file = ")
    )
    faulty = {
        "a variant without source": tree.replace(first_source + "\n", "", 1),
        "a release dated in the future": tree.replace(
            first_released, f'released = "{future}"', 1
        ),
        "an embeddings entry without a full digest": tree.replace(
            first_digest, 'sha256 = "abc"', 1
        ),
        "a dictation entry naming no card": tree.replace(
            first_card, 'card = "no-such-card"', 1
        ),
        "a dictation entry whose file is not a .bin": tree.replace(
            first_dictation_file, 'file = "ggml-tiny.gguf"', 1
        ),
    }
    failures = 0
    if run(FAMILIES, PUBLISHERS, today) != 0:
        print("selftest: the tree itself is red")
        failures += 1
    with tempfile.TemporaryDirectory() as tmp:
        for label, text in faulty.items():
            path = pathlib.Path(tmp) / "families.toml"
            path.write_text(text, encoding="utf-8")
            code = run(path, PUBLISHERS, today)
            if code != 1:
                print(f"selftest: {label} answered {code}, expected 1")
                failures += 1
        templated = next(
            p for p in sorted(REGISTRY.rglob("*.json"))
            if "template" in json.loads(p.read_text(encoding="utf-8"))
        )
        def no_french(card: dict) -> None:
            del card["description"]["fr"]
        def em_dash(card: dict) -> None:
            card["description"]["en"] += " " + EM_DASH + " fast"
        def contradicted(card: dict) -> None:
            card["capabilities"]["tools"] = not card["capabilities"]["tools"]
        def unsourced_draft(card: dict) -> None:
            card["draft"] = {"source": ""}
        def undated(card: dict) -> None:
            del card["released"]
        def unsized(card: dict) -> None:
            card["params_b"] = 0
        def unknown_usage(card: dict) -> None:
            card["usage"] = "everything"
        faulty_cards = {
            "a language card without its release date": undated,
            "a language card without a parameter count": unsized,
            "a language card of an unknown usage": unknown_usage,
            "a card marked as a draft model with no address": unsourced_draft,
            "a card without its French description": no_french,
            "a card carrying an em-dash": em_dash,
            "a card whose tools capability the template contradicts": contradicted,
        }
        for label, spoil in faulty_cards.items():
            copy = pathlib.Path(tmp) / "registry"
            if copy.exists():
                shutil.rmtree(copy)
            shutil.copytree(REGISTRY, copy)
            target = copy / templated.relative_to(REGISTRY)
            card = json.loads(target.read_text(encoding="utf-8"))
            spoil(card)
            target.write_text(json.dumps(card, ensure_ascii=False, indent=2), encoding="utf-8")
            code = run(FAMILIES, PUBLISHERS, today, copy)
            if code != 1:
                print(f"selftest: {label} answered {code}, expected 1")
                failures += 1
    print("check-model-families selftest:", "ok" if not failures else f"RED, {failures} miss(es)")
    return 0 if not failures else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--families", type=pathlib.Path, default=FAMILIES)
    parser.add_argument("--publishers", type=pathlib.Path, default=PUBLISHERS)
    parser.add_argument("--registry", type=pathlib.Path, default=REGISTRY)
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args()
    if args.selftest:
        return selftest()
    return run(args.families, args.publishers, datetime.date.today(), args.registry)


if __name__ == "__main__":
    sys.exit(main())
