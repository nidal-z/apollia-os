#!/usr/bin/env python3
"""Gather the model cards of `registry/models/` into the index the catalogue reads.

Each card is one JSON file under `registry/models/<group>/`, written for this
project (description and explanations in French and in English) with every
value it repeats from a model repository citing the address it was read from.
The release pipeline runs this script, signs its output in the minisign format
with the distribution key (`cargo tauri signer sign`), and publishes the index
and its signature side by side. The application accepts no other key.

Three keys of a card file belong to the repository and never reach the index:

  schema_version   the version of the card file format
  template         the recorded chat template the guard checks the declared
                   capabilities against
  selection_files  the download addresses a dictation card answers for

The index is deterministic: cards sorted by identifier, keys in file order, one
trailing newline. Two runs over the same tree give the same bytes, which is
what lets a signature made once be checked by anyone who rebuilds it.

Usage:
    python3 scripts/build_model_registry_index.py [--out PATH]
    python3 scripts/build_model_registry_index.py --test-fixture

`--test-fixture` rewrites the fixture the Rust tests verify: the index built
from the tree, signed by a throwaway key made on the spot with
`cargo tauri signer generate`, whose private half is deleted with its
temporary directory. Only the public half is kept beside the fixture.
"""

import argparse
import json
import pathlib
import subprocess
import sys
import tempfile
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
REGISTRY = ROOT / "registry" / "models"
PUBLISHERS = ROOT / "crates" / "apollia-llm" / "src" / "recommend" / "publishers.toml"
FIXTURES = ROOT / "crates" / "apollia-llm" / "src" / "model_cards" / "fixtures"
FIXTURE_INDEX = FIXTURES / "registry-index.json"
FIXTURE_SIGNATURE = FIXTURES / "registry-index.json.minisig"
FIXTURE_KEY = FIXTURES / "registry-test.key.pub"

INDEX_SCHEMA_VERSION = 1
CARD_SCHEMA_VERSION = 1
REPOSITORY_ONLY_KEYS = ("schema_version", "template", "selection_files")


def card_files(registry: pathlib.Path) -> list[pathlib.Path]:
    """Every card file under `registry`, in a stable order."""
    return sorted(p for p in registry.rglob("*.json") if p.is_file())


def load_card(path: pathlib.Path) -> dict:
    """One card file, as written."""
    return json.loads(path.read_text(encoding="utf-8"))


def index_card(card: dict) -> dict:
    """A card as the index carries it, without the repository-only keys."""
    return {k: v for k, v in card.items() if k not in REPOSITORY_ONLY_KEYS}


def publisher_lists(publishers: pathlib.Path) -> dict:
    """The official publishers and the recognised converters."""
    data = tomllib.loads(publishers.read_text(encoding="utf-8"))
    return {
        "official": [
            {"publisher": entry["publisher"], "orgs": list(entry.get("orgs", []))}
            for entry in data.get("official", [])
        ],
        "converters": list(data.get("converters", [])),
    }


def build(registry: pathlib.Path = REGISTRY, publishers: pathlib.Path = PUBLISHERS) -> str:
    """The index text for the cards under `registry`."""
    cards = [index_card(load_card(p)) for p in card_files(registry)]
    cards.sort(key=lambda c: c.get("id", ""))
    index = {
        "schema_version": INDEX_SCHEMA_VERSION,
        "cards": cards,
        "publishers": publisher_lists(publishers),
    }
    return json.dumps(index, ensure_ascii=False, indent=2) + "\n"


def write_test_fixture() -> int:
    """Rebuild the fixture index and sign it with a key made for the occasion."""
    text = build()
    FIXTURE_INDEX.write_text(text, encoding="utf-8")
    with tempfile.TemporaryDirectory() as tmp:
        key = pathlib.Path(tmp) / "registry-test.key"
        password = "registry-test"
        subprocess.run(
            ["cargo", "tauri", "signer", "generate", "--ci", "-p", password, "-w", str(key)],
            check=True,
            capture_output=True,
        )
        subprocess.run(
            ["cargo", "tauri", "signer", "sign", "-f", str(key), "-p", password, str(FIXTURE_INDEX)],
            check=True,
            capture_output=True,
        )
        signature = FIXTURE_INDEX.with_name(FIXTURE_INDEX.name + ".sig")
        signature.replace(FIXTURE_SIGNATURE)
        public = pathlib.Path(str(key) + ".pub").read_text(encoding="utf-8")
        FIXTURE_KEY.write_text(public.strip() + "\n", encoding="utf-8")
    sys.stdout.write(f"build_model_registry_index: fixture rewritten under {FIXTURES.relative_to(ROOT)}\n")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--out", type=pathlib.Path)
    parser.add_argument("--test-fixture", action="store_true")
    args = parser.parse_args()
    if args.test_fixture:
        return write_test_fixture()
    text = build()
    if args.out:
        args.out.write_text(text, encoding="utf-8")
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
