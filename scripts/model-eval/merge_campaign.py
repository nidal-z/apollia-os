#!/usr/bin/env python3
"""Merge per-probe campaign files into one campaign, per contract 1.9.

Each probe writes its own file so a failing probe does not cost the others their
results. This joins them into the single container the analysis scripts read.
`records_excluded` is carried through, never dropped: a campaign that loses a
run without saying so reports a cleaner result than it measured.

Usage: merge_campaign.py <out.json> <part.json> [<part.json> ...]
"""

import json
import os
import sys

import harness


def main(argv):
    if len(argv) < 2:
        sys.stderr.write("usage: merge_campaign.py <out.json> <part.json> ...\n")
        return 2
    out = argv[0]
    records = []
    excluded = []
    started = None
    campaign_id = os.environ.get("CAMPAIGN_ID") or os.path.splitext(
        os.path.basename(out)
    )[0]
    missing = []
    for part in argv[1:]:
        if not os.path.isfile(part):
            missing.append(part)
            continue
        with open(part, "r", encoding="utf-8") as handle:
            payload = json.load(handle)
        records.extend(payload.get("records", []))
        excluded.extend(payload.get("records_excluded", []))
        if started is None:
            started = payload.get("started_at")
    for part in missing:
        # A probe that produced no file is a probe that did not run. Recorded,
        # because a merged campaign that silently omits it looks complete.
        excluded.append(
            {"record_id": os.path.basename(part), "reason": "probe produced no file"}
        )
    harness.write_campaign(
        out, campaign_id, records, started or harness.now_rfc3339(), excluded
    )
    flagged = sum(1 for r in records if r.get("invalid"))
    sys.stderr.write(
        "merged %d record(s) into %s, %d carrying invariant violations, "
        "%d excluded\n" % (len(records), out, flagged, len(excluded))
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
