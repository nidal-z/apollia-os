#!/usr/bin/env python3
"""Regression cases for the sweep analysis, each one a defect that was real.

Every case here reconstructs a way the report was once willing to state a
conclusion the data did not support. They are written as datasets rather than as
unit tests because the defects lived in how records combine, not in any single
function, and a test that called the function directly would have missed all of
them.

Each case fails loudly if the guard it covers is removed. Run before touching
`analyse`, `verdict`, `curve_analysis` or `render_report`.

Usage: python3 sweep_selftest.py     (exit 0 when every case holds)

No server, no model, no network. Stdlib only.
"""

import json
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = tempfile.mkdtemp(prefix="sweep-selftest-")
sys.path.insert(0, HERE)
os.makedirs(OUT, exist_ok=True)
import harness

PROV = {
    "git_sha": "x", "git_dirty": False, "llama_server_version": "v", "llama_server_path": "p",
    "model_path": "/m.gguf", "model_sha256": "s", "model_sha256_scope": "whole_file",
    "launch_args": [], "machine_id": "m", "machine_chip": "c",
    "machine_memory_bytes": 1, "os_version": "1",
}


def sample(tps, state="cold"):
    return {"streamed": True, "cache_state": state, "prompt_tok_total": 1000,
            "prompt_tok_computed": 1000 if state == "cold" else 100,
            "prompt_tok_cached": 0 if state == "cold" else 900,
            "decode_tok": 100, "decode_ms": 100000.0 / tps, "decode_tps": tps,
            "prefill_ms": 500.0, "prefill_tps": 2000.0, "ttft_ms": 510.0,
            "ttft_cold_ms": 510.0 if state == "cold" else None,
            "ttft_warm_ms": None if state == "cold" else 510.0,
            "prompt_cache_hit_ratio": 0.0 if state == "cold" else 0.9,
            "request_wall_ms": 610.0, "degenerate": False, "empty": False}


def rec(probe, rid, idx, role, factor, level, samples=None, extra=None, invalid=None, conds=None):
    r = harness.build_record(
        probe=probe, record_id=rid, label="m", provenance_block=PROV,
        conditions_block=conds or harness.conditions(run_index=idx, run_order="randomised"),
        engine={"n_ctx": 32768, "n_ctx_slot_tok": 32768, "n_parallel": 1},
        campaign_id="c", samples=samples,
        stats=harness.stats_for(samples) if samples else None, extra=extra)
    r["sweep"] = {"plan": "c", "role": role, "factor": factor, "level": level}
    if invalid is not None:
        r["invalid"] = invalid
    return r


def container(records, factors=None):
    return {"schema_version": 1, "campaign_id": "c", "started_at": "x", "finished_at": "x",
            "records": records, "records_excluded": [],
            "sweep": {"plan": "c", "model_label": "m", "run_order": "randomised",
                      "shuffle_seed": 1, "runs_planned": len(records), "runs_completed": len(records),
                      "factors": factors or [{"name": "f", "baseline_level": "default"}]}}


def report(name, cont):
    path = os.path.join(OUT, name + ".json")
    json.dump(cont, open(path, "w"))
    r = subprocess.run([sys.executable, "sweep.py", "--report", path],
                       cwd=HERE, capture_output=True, text=True)
    return r.stdout + r.stderr


def check(label, text, must_contain=None, must_not_contain=None):
    ok = True
    for needle in must_contain or []:
        if needle not in text:
            ok = False
            print("  MISSING: %r" % needle)
    for needle in must_not_contain or []:
        if needle in text:
            ok = False
            print("  PRESENT BUT SHOULD NOT BE: %r" % needle)
    print("%s  %s" % ("PASS" if ok else "FAIL", label))
    if not ok:
        print("---- output ----\n%s\n----" % text)
    return ok

results = []

# A2: prefill curve must never be compared as a pooled record-level metric.
base = [rec("prefill_curve", "prefill-curve-m#%d" % i, i, "baseline", None, None,
            samples=[sample(60) for _ in range(5)],
            extra={"prefill_curve": [
                {"prompt_tok_total": n + 534, "stats": harness.stats_for(
                    [{"prefill_tps": 700.0} for _ in range(5)])}
                for n in (512, 1024, 2048, 4096, 8192, 16384)]}) for i in range(5)]
lvl = rec("prefill_curve", "prefill-curve-m#9", 9, "level", "n_ctx", "8192",
          samples=[sample(60) for _ in range(5)],
          extra={"prefill_curve": [
              {"prompt_tok_total": n + 534, "stats": harness.stats_for(
                  [{"prefill_tps": 700.0} for _ in range(5)])}
              for n in (512, 1024, 2048, 4096)]})
text = report("a2", container(base + [lvl], [{"name": "n_ctx", "baseline_level": "32768"}]))
results.append(check("A2 pooled prefill metric gone, per-length shows +0.0 %",
                     text, must_contain=["PREFILL CURVE, PER LENGTH", "+0.0 %",
                                         "lengths the baseline measured and this level did not"],
                     must_not_contain=["prefill_tps curve"]))

# A3: an unstable level must not receive an effect verdict.
base = [rec("speed", "speed-m-cold#%d" % i, i, "baseline", None, None,
            samples=[sample(60 + 0.1 * k) for k in range(5)]) for i in range(5)]
lvl = rec("speed", "speed-m-cold#9", 9, "level", "f", "1024",
          samples=[sample(t) for t in (30, 45, 66, 90, 105)])
text = report("a3", container(base + [lvl]))
results.append(check("A3 unstable level withheld", text,
                     must_contain=["unstable, within-run cv", "0.10 of 1.4.8"],
                     must_not_contain=["better, beyond"]))

# A5: a short concurrency aggregate must be provisional even when stats clears I8.
cbase = [rec("concurrency", "concurrency-m#%d" % i, i, "baseline", None, None,
             samples=[sample(60) for _ in range(6)],
             extra={"concurrency": {"n_requests": 1, "rounds": 3,
                                    "aggregate_decode_tps_wall": harness.aggregate([100.0, 101.0, 99.0])}})
         for i in range(5)]
clvl = rec("concurrency", "concurrency-m#9", 9, "level", "f", "4",
           samples=[sample(60) for _ in range(6)],
           extra={"concurrency": {"n_requests": 4, "rounds": 3,
                                  "aggregate_decode_tps_wall": harness.aggregate([400.0, 401.0, 399.0])}})
text = report("a5", container(cbase + [clvl]))
results.append(check("A5 concurrency aggregate n=3 marked provisional", text,
                     must_contain=["PROVISIONAL UNDER I8", "aggregate_decode_tps_wall"],
                     must_not_contain=["better, beyond"]))

# A6: a record violating I1/I9/I13 must be disqualified, not compared.
base = [rec("speed", "speed-m-cold#%d" % i, i, "baseline", None, None,
            samples=[sample(60) for _ in range(5)]) for i in range(5)]
bad = rec("speed", "speed-m-cold#9", 9, "level", "f", "1024",
          samples=[sample(72) for _ in range(5)], invalid=["I1", "I9", "I13"])
text = report("a6", container(base + [bad]))
results.append(check("A6 invariant violations disqualify", text,
                     must_contain=["INVARIANT VIOLATIONS ON COMPARED RECORDS", "disqualified, violates I1, I9, I13"],
                     must_not_contain=["better, beyond"]))

# A7: a mixed cache-state record must be named, not silently dropped.
base = [rec("speed", "speed-m-cold#%d" % i, i, "baseline", None, None,
            samples=[sample(60) for _ in range(5)]) for i in range(5)]
mixed = rec("speed", "speed-m-cold#9", 9, "level", "f", "1024",
            samples=[sample(60), sample(60), sample(60), sample(60), sample(60, "warm")])
text = report("a7", container(base + [mixed]))
results.append(check("A7 mixed-state record surfaced with its mismatch", text,
                     must_contain=["REQUESTED CACHE STATE NOT ACHIEVED",
                                   "cold requested, 1 of 5 sample(s) came back cold/warm"],
                     must_not_contain=["better, beyond"]))

# A8 case A: one level point must not satisfy two baseline lengths.
base = [rec("prefill_curve", "prefill-curve-m#%d" % i, i, "baseline", None, None,
            samples=[sample(60) for _ in range(5)],
            extra={"prefill_curve": [
                {"prompt_tok_total": n, "stats": harness.stats_for(
                    [{"prefill_tps": 1000.0} for _ in range(5)])} for n in (1024, 1200)]})
        for i in range(5)]
lvl = rec("prefill_curve", "prefill-curve-m#9", 9, "level", "f", "x",
          samples=[sample(60) for _ in range(5)],
          extra={"prefill_curve": [{"prompt_tok_total": 1100, "stats": harness.stats_for(
              [{"prefill_tps": 2000.0} for _ in range(5)])}]})
text = report("a8a", container(base + [lvl]))
rows = text.count("tok  baseline")
results.append(check("A8a one level point produces one row, no double count (got %d)" % rows, text))
if rows != 1:
    print("  FAIL detail: %d rows from a single level point" % rows)
    results[-1] = False

# A8 case B: a level point with no baseline counterpart must be named.
base = [rec("prefill_curve", "prefill-curve-m#%d" % i, i, "baseline", None, None,
            samples=[sample(60) for _ in range(5)],
            extra={"prefill_curve": [
                {"prompt_tok_total": n + 534, "stats": harness.stats_for(
                    [{"prefill_tps": 700.0} for _ in range(5)])}
                for n in (512, 1024, 2048)]}) for i in range(5)]
lvl = rec("prefill_curve", "prefill-curve-m#9", 9, "level", "f", "65536",
          samples=[sample(60) for _ in range(5)],
          extra={"prefill_curve": [
              {"prompt_tok_total": n + 534, "stats": harness.stats_for(
                  [{"prefill_tps": 700.0} for _ in range(5)])}
              for n in (512, 1024, 2048, 4096)]})
text = report("a8b", container(base + [lvl]))
results.append(check("A8b level point absent from baseline is named", text,
                     must_contain=["lengths this level measured and the baseline did not"]))

# A9: an unstable baseline curve point withholds that length at every level, and
# only that length. The guard lived in the metric table alone, and the curve
# table went on printing seventy verdicts off a contaminated baseline, two of
# them the reverse of what the clean grid says. The mutation is the adversarial
# pass's own: spread one baseline point's samples around its own median, which
# moves the within-run cv and leaves the median, the between-run dispersion and
# every threshold untouched. Anything that changes here is the guard, not the
# arithmetic.
def curve_pts(rates_by_length):
    return {"prefill_curve": [
        {"prompt_tok_total": n + 534, "stats": harness.stats_for(
            [{"prefill_tps": r} for r in rates])}
        for n, rates in rates_by_length]}


steady = [700.0] * 5
jittered = [400.0, 700.0, 1000.0, 700.0, 700.0]      # same median, cv about 0.30
base = [rec("prefill_curve", "prefill-curve-m#%d" % i, i, "baseline", None, None,
            samples=[sample(60) for _ in range(5)],
            extra=curve_pts([(512, jittered if i == 2 else steady), (4096, steady)]))
        for i in range(5)]
lvl = rec("prefill_curve", "prefill-curve-m#9", 9, "level", "f", "x",
          samples=[sample(60) for _ in range(5)],
          extra=curve_pts([(512, [900.0] * 5), (4096, [900.0] * 5)]))
text = report("a9", container(base + [lvl]))
results.append(check("A9 unstable baseline curve point withholds that length", text,
                     must_contain=["Lengths 1046 are withheld at every level",
                                   "withheld, baseline run(s) 2 unstable at cv 0.30"]))
# The other length must still be compared. A guard that withholds everything is
# not a guard, and attack 3c refuted over-firing for the metric table.
stable_row = [line for line in text.split("\n")
              if line.strip().startswith("4630 tok")]
if len(stable_row) != 1 or "beyond the" not in stable_row[0]:
    print("  FAIL detail: stable length not compared: %s" % (stable_row or "absent"))
    results[-1] = False

# B3: a declared gate that could not run must be announced.
base = [rec("speed", "speed-m-cold#%d" % i, i, "baseline", None, None,
            samples=[sample(60) for _ in range(5)]) for i in range(5)]
lvl = rec("speed", "speed-m-cold#9", 9, "level", "kv", "q4_0",
          samples=[sample(72) for _ in range(5)])
text = report("b3", container(base + [lvl],
                              [{"name": "kv", "baseline_level": "f16", "levels": ["q4_0"],
                                "quality_gate": "toolcall"}]))
results.append(check("B3 declared-but-unapplied gate announced", text,
                     must_contain=["QUALITY GATE DECLARED BUT NOT APPLIED", "kv = q4_0"]))

# A4: a concurrency record with warm samples must not be reported as a gain.
cbase = [rec("concurrency", "concurrency-m#%d" % i, i, "baseline", None, None,
             samples=[sample(60) for _ in range(5)],
             extra={"concurrency": {"n_requests": 1, "rounds": 5, "samples_not_cold": 0,
                                    "aggregate_decode_tps_wall": harness.aggregate([100.0, 101.0, 99.0, 100.5, 99.5])}})
         for i in range(5)]
cwarm = rec("concurrency", "concurrency-m#9", 9, "level", "f", "4",
            samples=[sample(60), sample(60), sample(60, "warm"), sample(60, "warm"), sample(60)],
            extra={"concurrency": {"n_requests": 4, "rounds": 5, "samples_not_cold": 2,
                                   "aggregate_decode_tps_wall": harness.aggregate([130.0, 131.0, 129.0, 130.5, 129.5])}})
text = report("a4", container(cbase + [cwarm]))
results.append(check("A4 warm samples in a cold probe withhold the verdict", text,
                     must_contain=["cold requested, 2 of 5 sample(s) came back cold/warm",
                                   "not a cold measurement",
                                   "REQUESTED CACHE STATE NOT ACHIEVED"],
                     must_not_contain=["better, beyond"]))

# --- wave 2: two verdict axes, conflict detection, interactions ----------------

def csmp(tps, prefill_tps, prefill_ms, ttft, state="cold", computed=4142, cached=0):
    return {"streamed": True, "cache_state": state, "prompt_tok_total": computed + cached,
            "prompt_tok_computed": computed, "prompt_tok_cached": cached,
            "prompt_cache_hit_ratio": cached / float(computed + cached),
            "decode_tok": 100, "decode_ms": 100000.0 / tps, "decode_tps": tps,
            "prefill_ms": prefill_ms, "prefill_tps": prefill_tps, "ttft_ms": ttft,
            "ttft_cold_ms": ttft if state == "cold" else None,
            "ttft_warm_ms": None if state == "cold" else ttft,
            "request_wall_ms": ttft + 500, "degenerate": False, "empty": False}


def axis_container():
    recs = []
    for i in range(5):
        j = i * 0.4
        recs.append(rec("speed", "speed-m-cold#%d" % i, i, "baseline", None, None,
                        [csmp(77, 1700 + j + k, 1000, 1220 + j + k) for k in range(5)]))
        recs.append(rec("cache_reuse", "cache-reuse-m-continuation#%d" % i, i, "baseline", None, None,
                        [csmp(77, 1470, 400 + j, 460 + j + k * 0.2, "warm", 587, 3627) for k in range(5)]))
    # -ub 2048 buys cold prefill and sells the warm continuation.
    recs.append(rec("speed", "speed-m-cold#9", 9, "level", "n_ubatch", "2048",
                    [csmp(77, 2060 + k, 820, 1010 + k) for k in range(5)]))
    recs.append(rec("cache_reuse", "cache-reuse-m-continuation#9", 9, "level", "n_ubatch", "2048",
                    [csmp(77, 2230, 950, 1012 + k * 0.2, "warm", 2119, 2095) for k in range(5)]))
    # At -ctxcp 0 the continuation is served entirely cold. It must still be read
    # as the continuation and not silently swapped for the seed.
    recs.append(rec("speed", "speed-m-cold#11", 11, "level", "ctx_checkpoints", "0",
                    [csmp(77, 1700 + k, 1000, 1220 + k) for k in range(5)]))
    recs.append(rec("cache_reuse", "cache-reuse-m-continuation#11", 11, "level", "ctx_checkpoints", "0",
                    [csmp(77, 1700, 2480, 2541 + k * 0.2, "cold", 4214, 0) for k in range(5)]))
    recs.append(rec("speed", "speed-m-cold#13", 13, "interaction", "ub x ctxcp", "2048/0",
                    [csmp(77, 2060 + k, 820, 1010 + k) for k in range(5)]))
    recs.append(rec("cache_reuse", "cache-reuse-m-continuation#13", 13, "interaction", "ub x ctxcp", "2048/0",
                    [csmp(77, 1700, 2480, 2541 + k * 0.2, "cold", 4214, 0) for k in range(5)]))
    c = container(recs, [{"name": "n_ubatch", "baseline_level": "default"},
                         {"name": "ctx_checkpoints", "baseline_level": "default",
                          "fields": ["ctx_checkpoints"]}])
    c["sweep"]["interactions"] = [{"name": "ub x ctxcp", "fields": ["n_ubatch", "ctx_checkpoints"],
                                   "baseline_cell": "default/default", "cells": ["2048/0"],
                                   "fields_swept_individually": ["ctx_checkpoints", "n_ubatch"]}]
    return c


text = report("axes", axis_container())
results.append(check("W1 every factor reported on both verdict axes", text,
                     must_contain=["COLD PREFILL", "WARM CONTINUATION",
                                   "continuation hit ratio", "continuation recompute tok"]))
results.append(check("W2 opposing verdicts flagged and never merged", text,
                     must_contain=["FACTORS WHOSE TWO VERDICTS DISAGREE",
                                   "DISAGREEMENT at 2048: cold better, warm worse",
                                   "These are not averaged"]))
results.append(check("W3 continuation read correctly when served cold at ctxcp 0", text,
                     must_contain=["-100.0 %", "+617.9 %"]))
results.append(check("W4 interactions reported apart from single factors", text,
                     must_contain=["INTERACTIONS", "not one-factor-at-a-time"]))
results.append(check("W5 a flag with no Rust field says so", text,
                     must_contain=["No field in LlamaServerConfig"]))

print("\n%d/%d passed" % (sum(1 for r in results if r), len(results)))
sys.exit(0 if all(results) else 1)
