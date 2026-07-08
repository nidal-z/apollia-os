#!/usr/bin/env python3
"""Aggregate results/<label>.json into a comparison matrix (speed x tool-calling x ESRS)."""

import glob
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))


def _f(x):
    return "-" if x is None else (f"{x:g}" if isinstance(x, (int, float)) else str(x))


rows = []
for p in sorted(glob.glob(os.path.join(HERE, "results", "*.json"))):
    rows.append(json.load(open(p, encoding="utf-8")))

cols = [
    ("model", 24),
    ("TTFT ms", 9),
    ("dec tok/s", 10),
    ("batch x", 8),
    ("map tok/s", 10),
    ("tool", 6),
    ("ESRS F1", 8),
    ("flags", 22),
]
hdr = "".join(h.ljust(w) for h, w in cols)
print(hdr)
print("-" * len(hdr))


def g(d, *keys, default=None):
    for k in keys:
        if not isinstance(d, dict):
            return default
        d = d.get(k)
    return d if d is not None else default


for r in rows:
    sp, tl, es, ba = r.get("speed", {}), r.get("toolcall", {}), r.get("esrs", {}), r.get("batch", {})
    flags = []
    if r.get("error"):
        flags.append(r["error"])
    if g(sp, "degenerate"):
        flags.append("degenerate")
    if g(sp, "empty"):
        flags.append("empty")
    if isinstance(tl, dict) and tl.get("skipped"):
        flags.append("quality-skipped")
    vals = [
        (r.get("label", "?"))[:23],
        _f(g(sp, "ttft_ms")),
        _f(g(sp, "decode_tps")),
        _f(g(ba, "speedup")),
        _f(g(ba, "map_tps")),
        _f(g(tl, "score")),
        _f(g(es, "f1")),
        ", ".join(flags)[:21],
    ]
    print("".join(str(v).ljust(w) for v, (_, w) in zip(vals, cols)))
