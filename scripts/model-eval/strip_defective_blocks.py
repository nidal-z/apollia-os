#!/usr/bin/env python3
"""Remove the two defective blocks from the pre-contract results files.

The six `results/<label>.json` files predate the contract. Each holds four
independent blocks, and they do not share a fate because they do not share the
defect.

  speed    deleted. Its time to first token is a cache-hit latency: the probe
           ran the same prompt twice and reported the second. Its throughput
           counts server-sent events, not tokens.
  batch    deleted. Same probe lineage, and measured at -np 8 while the product
           runs -np 1, so the figure does not describe what a user gets.
  toolcall retained. Scored, not timed. Neither defect touches it.
  esrs     retained. Same.

A number known to be wrong is not a record of anything. It is a trap for
whoever cites it in six months without reading this file, which is why the two
blocks go even for the four labels whose GGUF no longer exists and which
therefore cannot be re-measured.

Run only after the replacement campaign exists and has been verified.

Usage: strip_defective_blocks.py [--apply]   (default is a dry run)
"""

import glob
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
RESULTS = os.path.join(HERE, "results")
DEFECTIVE = ("speed", "batch")

# Written into each stripped file so the deletion explains itself in place.
TOMBSTONE = (
    "speed and batch blocks removed: the probe that produced them reported a "
    "warm cache-hit latency as a time to first token, counted server-sent "
    "events as tokens, and launched with -np 8 -c 16384 rather than the "
    "product's -np 1 -c 32768. toolcall and esrs are retained: they are scored "
    "rather than timed, so neither defect applies, but they are single-sample "
    "and carry no provenance, so they are provisional under I8 and excluded "
    "from comparisons."
)


def legacy_files():
    """The pre-contract files: a top-level `label`, no `records` array."""
    found = []
    for path in sorted(glob.glob(os.path.join(RESULTS, "*.json"))):
        with open(path, "r", encoding="utf-8") as handle:
            payload = json.load(handle)
        if isinstance(payload, dict) and "records" not in payload and "label" in payload:
            found.append((path, payload))
    return found


def main(argv):
    apply_changes = "--apply" in argv
    files = legacy_files()
    if not files:
        sys.stdout.write("no pre-contract results file found\n")
        return 0

    for path, payload in files:
        present = [b for b in DEFECTIVE if b in payload]
        kept = [b for b in ("toolcall", "esrs") if b in payload]
        sys.stdout.write(
            "%-28s remove %-16s keep %s\n"
            % (payload.get("label", "?"), ",".join(present) or "-", ",".join(kept) or "-")
        )
        if not apply_changes:
            continue
        for block in present:
            payload.pop(block)
        payload["blocks_removed"] = list(present)
        payload["blocks_removed_reason"] = TOMBSTONE
        with open(path, "w", encoding="utf-8") as handle:
            json.dump(payload, handle, ensure_ascii=False, indent=1)

    if not apply_changes:
        sys.stdout.write("\ndry run. Pass --apply to write.\n")
    else:
        sys.stdout.write("\nstripped %d file(s)\n" % len(files))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
