#!/usr/bin/env python3
"""Render campaign files as a comparison matrix.

Reads the contract's campaign container: `records[]`, each carrying `probe`,
`stats` and `invalid`. It reports the cold and warm columns separately because
they are different quantities, and it prints the invariant violations rather
than hiding them behind a clean-looking number, which is what the pre-contract
version of this script did by construction.

Usage: aggregate.py [<campaign.json> ...]   (no args = every file in results/)
"""

import glob
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))

COLUMNS = (
    ("model", 22),
    ("cold TTFT", 11),
    ("prefill", 10),
    ("warm TTFT", 11),
    ("hit %", 8),
    ("decode", 10),
    ("cv", 7),
    ("tool", 6),
    ("F1", 6),
    ("batch x", 8),
    ("invalid", 16),
)


def fmt(value, spec="%.1f"):
    return "-" if value is None else spec % value


def median_of(record, field):
    block = (record.get("stats") or {}).get(field)
    return block.get("median") if block else None


def load(paths):
    campaigns = []
    for path in paths:
        with open(path, "r", encoding="utf-8") as handle:
            payload = json.load(handle)
        if "records" in payload:
            campaigns.append((path, payload))
    return campaigns


def summarise(records):
    """One row's worth of figures, keyed by label."""
    rows = {}
    for record in records:
        label = record.get("label", "?")
        row = rows.setdefault(
            label, {"invalid": set(), "excluded": 0, "provisional": False}
        )
        row["invalid"].update(record.get("invalid") or [])
        probe = record.get("probe")
        if probe == "speed":
            # Cold and warm live in separate records. Which is which is read off
            # the samples' observed cache_state, never off the record id.
            states = {s.get("cache_state") for s in record.get("samples") or []}
            if states == {"cold"}:
                row["cold_ttft"] = median_of(record, "ttft_cold_ms")
                row["prefill"] = median_of(record, "prefill_tps")
                row["decode"] = median_of(record, "decode_tps")
                block = (record.get("stats") or {}).get("decode_tps")
                row["cv"] = block.get("cv") if block else None
            elif states == {"warm"}:
                row["warm_ttft"] = median_of(record, "ttft_warm_ms")
                hit = median_of(record, "prompt_cache_hit_ratio")
                row["hit"] = 100.0 * hit if hit is not None else None
        elif probe == "cache_reuse":
            if row.get("hit") is None:
                hit = median_of(record, "prompt_cache_hit_ratio")
                if hit:
                    row["hit"] = 100.0 * hit
        elif probe == "toolcall":
            row["tool"] = (record.get("toolcall") or {}).get("score")
        elif probe == "esrs":
            row["f1"] = (record.get("esrs") or {}).get("f1")
        elif probe == "batch":
            row["batch"] = (record.get("batch") or {}).get("speedup")
        if "I8" in (record.get("invalid") or []):
            row["provisional"] = True
    return rows


def main(argv):
    paths = argv or sorted(glob.glob(os.path.join(HERE, "results", "*.json")))
    if not paths:
        sys.stdout.write("no campaign file in results/\n")
        return 0
    campaigns = load(paths)
    if not campaigns:
        sys.stdout.write(
            "no file carries a contract campaign container. The pre-contract "
            "results/<label>.json shape is not readable here.\n"
        )
        return 1

    records = []
    excluded_total = 0
    for _, payload in campaigns:
        records.extend(payload.get("records", []))
        excluded_total += len(payload.get("records_excluded") or [])

    header = "".join(name.ljust(width) for name, width in COLUMNS)
    sys.stdout.write(header + "\n" + "-" * len(header) + "\n")
    for label, row in sorted(summarise(records).items()):
        values = (
            label[:21],
            fmt(row.get("cold_ttft")),
            fmt(row.get("prefill"), "%.0f"),
            fmt(row.get("warm_ttft")),
            fmt(row.get("hit"), "%.1f"),
            fmt(row.get("decode")),
            fmt(row.get("cv"), "%.3f"),
            fmt(row.get("tool"), "%.2f"),
            fmt(row.get("f1"), "%.2f"),
            fmt(row.get("batch"), "%.2f"),
            ",".join(sorted(row["invalid"]))[:15] or "-",
        )
        sys.stdout.write(
            "".join(str(v).ljust(w) for v, (_, w) in zip(values, COLUMNS)) + "\n"
        )

    sys.stdout.write(
        "\n%d record(s) across %d campaign file(s), %d excluded.\n"
        % (len(records), len(campaigns), excluded_total)
    )
    sys.stdout.write(
        "Cold and warm are separate columns because they are separate "
        "quantities. A warm hit ratio below 100 percent means the resend "
        "recomputed part of the prompt.\n"
    )
    if any(r.get("invalid") for r in records):
        sys.stdout.write(
            "I8 marks an aggregate below 5 repetitions: provisional, excluded "
            "from comparisons, not wrong.\n"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
