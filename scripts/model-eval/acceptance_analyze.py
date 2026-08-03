#!/usr/bin/env python3
"""Acceptance-campaign analysis under the harness gates. Stdlib only.

The committed caller of `harness.require_cell_reps` and
`harness.require_informative_samples`: the adversarial pass on D1 cycle 2
found those gates had no caller, which is how a one-rep cell and a
five-identical-reps screening both entered comparisons. This script is the
single analysis path for acceptance campaigns; ad-hoc analysis of the same
files is the drift Part 1 of the contract exists to prevent.

Usage:
  python3 acceptance_analyze.py results/d1mtp-agentic-{mtp,nospec}-r{1..5}.json
  (pass every cell file of one campaign; cells are grouped by the
   `<campaign>-<cell>-r<rep>.json` naming convention)

Reports, per cell and pooled: drafting incidence before any ratio, pooled
acceptance ratio, accepted per verification pass (approximate at ~2 percent
when the drafter emits more than `n_draft` per pass), engine decode rate,
and the crossed length x reasoning-share x health grid. Comparisons are
refused, never annotated, when a cell is under 5 informative repetitions.
Capped and twin-chars iterations are degenerate: excluded from healthy
cells, counted apart, never silently dropped.
"""

import json
import re
import statistics
import sys

import harness

CAP_TOK = 1190
LENGTH_BINS = ((0, 200), (200, 800), (800, 100000))
SHARE_BINS = ((0.0, 0.6), (0.6, 0.9), (0.9, 1.01))


def load(paths):
    cells = {}
    for p in paths:
        m = re.match(r".*/(.+)-r(\d+)\.json$", p)
        if not m:
            raise harness.ProbeError("unrecognised cell file name: %s" % p)
        cell, rep = m.group(1), int(m.group(2))
        with open(p) as f:
            d = json.load(f)
        its = []
        for r in d["records"]:
            for i in r["turn"].get("iteration_records", []):
                if i.get("decode_tok") and i.get("decode_ms"):
                    i["_rep"] = rep
                    i["_task"] = r["conditions"].get("run_index")
                    its.append(i)
        cells.setdefault(cell, {})[rep] = its
    return cells


def share(i):
    return (i.get("decode_tok_reasoning") or 0) / i["decode_tok"]


def degenerate(i):
    if i["decode_tok"] >= CAP_TOK:
        return True
    rc, cc = i.get("reasoning_chars") or 0, i.get("content_chars") or 0
    return rc > 0 and cc > 0 and abs(rc - cc) <= 16 and i["decode_tok"] >= 800


def pooled(its):
    dec = sum(i["decode_tok"] for i in its)
    ms = sum(i["decode_ms"] for i in its)
    da = sum(i.get("draft_tok_accepted") or 0 for i in its)
    dt = sum(i.get("draft_tok") or 0 for i in its)
    return {
        "n": len(its),
        "drafting": sum(1 for i in its if i.get("draft_tok")),
        "ratio": da / dt if dt else None,
        "acc_per_pass": da / (dec - da) if dec > da and da else None,
        "decode_tps": dec / (ms / 1000.0) if ms else None,
        "decode_tok": dec,
    }


def main(paths):
    cells = load(paths)
    harness.require_cell_reps({c: len(reps) for c, reps in cells.items()})
    # The informative-samples gate runs on the acceptance evidence: per-rep
    # accepted totals. Byte-identical replays collapse to one repetition.
    harness.require_informative_samples(
        {
            c: [
                sum(i.get("draft_tok_accepted") or 0 for i in its)
                for its in reps.values()
            ]
            for c, reps in cells.items()
            if any(i.get("draft_tok") for its in reps.values() for i in its)
        }
    )

    for cell, reps in sorted(cells.items()):
        allits = [i for its in reps.values() for i in its]
        healthy = [i for i in allits if not degenerate(i)]
        degen = [i for i in allits if degenerate(i)]
        p = pooled(allits)
        h = pooled(healthy)
        print("== %s" % cell)
        print(
            "   all: n=%d drafting=%d/%d ratio=%s acc/pass=%s tps=%.2f" % (
                p["n"], p["drafting"], p["n"],
                "%.3f" % p["ratio"] if p["ratio"] is not None else "-",
                "%.2f" % p["acc_per_pass"] if p["acc_per_pass"] else "-",
                p["decode_tps"],
            )
        )
        print(
            "   healthy: n=%d tps=%.2f | degenerate: n=%d (%d tok, apart)" % (
                h["n"], h["decode_tps"] or 0.0, len(degen),
                sum(i["decode_tok"] for i in degen),
            )
        )
        per_rep = [
            pooled(its)["decode_tps"] for _, its in sorted(reps.items())
        ]
        cv = (
            statistics.stdev(per_rep) / statistics.mean(per_rep)
            if len(per_rep) > 1 else None
        )
        print(
            "   per-rep tps: %s cv=%s" % (
                ["%.2f" % t for t in per_rep],
                "%.4f" % cv if cv is not None else "-",
            )
        )
        print("   crossed healthy cells (length x reasoning share):")
        for lo, hi in LENGTH_BINS:
            row = "     %5d-%-6d" % (lo, hi)
            for rlo, rhi in SHARE_BINS:
                c = [
                    i for i in healthy
                    if lo <= i["decode_tok"] < hi and rlo <= share(i) < rhi
                ]
                if not c:
                    row += "%16s" % "-"
                    continue
                q = pooled(c)
                app = (
                    "%.2f" % q["acc_per_pass"] if q["acc_per_pass"] else "-"
                )
                flag = "" if q["n"] >= 5 else "(n<5)"
                row += "%16s" % ("n=%d %s%s" % (q["n"], app, flag))
            print(row)


if __name__ == "__main__":
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    main(sys.argv[1:])
