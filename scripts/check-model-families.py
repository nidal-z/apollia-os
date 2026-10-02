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

Rules, each reported with the entry it names:

  families.toml
    * the file parses and declares `schema_version = 1`
    * every family has at least one variant, and every variant a `source`
    * every `released` is an ISO date, not later than today
    * every variant carries a repository: a `repo_patterns` entry on its family
      or an `mlx_repos` entry of its own
    * every `mlx_repos` entry is `owner/name`, and its owner is on one of the
      two lists of publishers.toml
  publishers.toml
    * the file parses and declares `schema_version = 1`
    * both lists are non-empty, and no handle sits on both
    * every `publisher` of families.toml is an official publisher

Exit codes, read by `just guards`:

  0  both files measured, every rule holds
  1  at least one rule broken
  2  a file is absent, so nothing was measured

`--families PATH` and `--publishers PATH` point the guard at other files, which
is how `--selftest` checks the guard itself against two faulty fixtures.
"""

import argparse
import datetime
import pathlib
import sys
import tempfile
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
RECOMMEND = ROOT / "crates" / "apollia-llm" / "src" / "recommend"
FAMILIES = RECOMMEND / "families.toml"
PUBLISHERS = RECOMMEND / "publishers.toml"
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


def run(families: pathlib.Path, publishers: pathlib.Path, today: datetime.date) -> int:
    for path in (families, publishers):
        if not path.is_file():
            print(f"check-model-families: {path} is absent, nothing measured")
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
    families_count = len(fam.get("family", []))
    for fault in faults:
        print(f"  {fault}")
    if faults:
        print(f"check-model-families: RED, {len(faults)} fault(s) over {measured} variant(s)")
        return 1
    print(
        f"check-model-families: ok, {families_count} families, {measured} variants, "
        f"{len(names)} official publishers, {len(conv)} converters"
    )
    return 0


def selftest() -> int:
    """The guard answers 0 on the tree and 1 on each of two faulty tables."""
    today = datetime.date.today()
    future = (today + datetime.timedelta(days=30)).isoformat()
    tree = FAMILIES.read_text(encoding="utf-8")
    first_source = next(line for line in tree.splitlines() if line.startswith("source = "))
    first_released = next(line for line in tree.splitlines() if line.startswith("released = "))
    faulty = {
        "a variant without source": tree.replace(first_source + "\n", "", 1),
        "a release dated in the future": tree.replace(
            first_released, f'released = "{future}"', 1
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
    print("check-model-families selftest:", "ok" if not failures else f"RED, {failures} miss(es)")
    return 0 if not failures else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--families", type=pathlib.Path, default=FAMILIES)
    parser.add_argument("--publishers", type=pathlib.Path, default=PUBLISHERS)
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args()
    if args.selftest:
        return selftest()
    return run(args.families, args.publishers, datetime.date.today())


if __name__ == "__main__":
    sys.exit(main())
