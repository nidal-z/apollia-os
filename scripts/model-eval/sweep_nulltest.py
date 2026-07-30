#!/usr/bin/env python3
"""Measured false-positive rate and power of the sweep detection threshold.

`sweep.detection_threshold_pct` replaced a bare-cv comparison with
`t(0.975, n-1) * cv * sqrt(1 + 1/n)`, on the argument that reading the cv as the
bar is a one-sigma test that a third of null comparisons clear by chance. That
argument was never measured, and neither was what the replacement misses. This
file measures both, against the shipped code path.

Two suites:

  null   baseline runs and the level run drawn from one distribution, so no
         effect exists by construction. Anything the report calls an effect is
         a false positive. The bare-cv rule is scored on the same replicates,
         so the claim that motivated the replacement is checked rather than
         repeated.

  power  the same generator with a known multiplicative effect injected. A test
         that never false-positives by never detecting anything is the opposite
         failure and is just as wrong.

Records are built once through `harness.build_record`, checked for an empty
`invalid` array, then patched per replicate on `decode_tps` and the `decode_ms`
that I5 ties to it. Rebuilding every replicate through `build_record` costs
seventeen aggregate blocks per record and would put the run in the hours; the
patched records are re-checked through `build_record` on a sample, and the
sample rate and outcome are printed, so the shortcut is visible rather than
assumed.

Nothing here writes to the repository and nothing here modifies `sweep.py`. The
calibration endpoints swap `sweep.T95` in memory, in this process only.

Usage: python3 sweep_nulltest.py [replicates]     (default 10000)

No server, no model, no network. Stdlib only.
"""

import math
import os
import random
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import harness
import sweep

PROV = {
    "git_sha": "x", "git_dirty": False, "llama_server_version": "v",
    "llama_server_path": "p", "model_path": "/m.gguf", "model_sha256": "s",
    "model_sha256_scope": "whole_file", "launch_args": [], "machine_id": "m",
    "machine_chip": "c", "machine_memory_bytes": 1, "os_version": "1",
}

DECODE_TOK = 128


def sample(tps):
    """One cold speed sample whose decode_ms is consistent with its rate.

    I5 recomputes `decode_tok / (decode_ms / 1000)` and compares it to
    `decode_tps`, so the two move together or the record is disqualified before
    it reaches a comparison and the suite measures the disqualifier instead of
    the threshold.
    """
    return {
        "streamed": True, "cache_state": "cold",
        "prompt_tok_total": 1000, "prompt_tok_computed": 1000, "prompt_tok_cached": 0,
        "decode_tok": DECODE_TOK, "decode_ms": 1000.0 * DECODE_TOK / tps,
        "decode_tps": tps, "prefill_ms": 500.0, "prefill_tps": 2000.0,
        "ttft_ms": 510.0, "ttft_cold_ms": 510.0, "ttft_warm_ms": None,
        "prompt_cache_hit_ratio": 0.0, "request_wall_ms": 610.0,
        "degenerate": False, "empty": False,
    }


def build(record_id, run_index, role, factor, level, rates):
    record = harness.build_record(
        probe="speed", record_id=record_id, label="m", provenance_block=PROV,
        conditions_block=harness.conditions(run_index=run_index, run_order="randomised"),
        engine={"n_ctx": 32768, "n_ctx_slot_tok": 32768, "n_parallel": 1},
        campaign_id="c", samples=[sample(r) for r in rates],
        stats=harness.stats_for([sample(r) for r in rates]))
    record["sweep"] = {"plan": "c", "role": role, "factor": factor, "level": level}
    return record


def patch(record, rates):
    """Overwrite one record's decode rates without rebuilding its stats block."""
    for spl, rate in zip(record["samples"], rates):
        spl["decode_tps"] = rate
        spl["decode_ms"] = 1000.0 * DECODE_TOK / rate
    record["stats"]["decode_tps"] = harness.aggregate(rates)
    record["stats"]["decode_ms"] = harness.aggregate(
        [s["decode_ms"] for s in record["samples"]])
    return record


def container(records, n_baseline):
    return {
        "schema_version": 1, "campaign_id": "c", "started_at": "x", "finished_at": "x",
        "records": records, "records_excluded": [],
        "sweep": {"plan": "c", "model_label": "m", "run_order": "randomised",
                  "shuffle_seed": 1, "runs_planned": n_baseline + 1,
                  "runs_completed": n_baseline + 1,
                  "factors": [{"name": "f", "baseline_level": "default"}]},
    }


# ---------------------------------------------------------------------------
# Generation
# ---------------------------------------------------------------------------


def jitter(rng, spread, shape):
    """A multiplier with mean one and the requested relative dispersion."""
    if spread <= 0.0:
        return 1.0
    if shape == "lognormal":
        sigma = math.sqrt(math.log(1.0 + spread * spread))
        return math.exp(rng.gauss(0.0, sigma) - 0.5 * sigma * sigma)
    return 1.0 + rng.gauss(0.0, spread)


def draw_run(rng, mean, between, within, n_samples, shape):
    run_mean = mean * jitter(rng, between, shape)
    return [max(1e-6, run_mean * jitter(rng, within, shape)) for _ in range(n_samples)]


BASE_RATE = 77.15  # the clean qwen grid's cold decode median


def replicate(rng, cfg, effect):
    """One synthetic campaign. `effect` is the multiplier on the level's mean."""
    baseline = [
        draw_run(rng, BASE_RATE, cfg["between"], cfg["within"], cfg["n_samples"], cfg["shape"])
        for _ in range(cfg["n_baseline"])
    ]
    level = draw_run(
        rng, BASE_RATE * effect,
        cfg["between"] * cfg["level_spread"], cfg["within"] * cfg["level_spread"],
        cfg["n_samples"], cfg["shape"])
    return baseline, level


# ---------------------------------------------------------------------------
# Scoring
# ---------------------------------------------------------------------------

EFFECT = "effect"
NONE = "no detectable effect"
WITHHELD_PROVISIONAL = "withheld, provisional"
WITHHELD_UNSTABLE = "withheld, unstable"
OTHER = "other"


def classify(verdict_text):
    if verdict_text.startswith(("better", "worse")):
        return EFFECT
    if verdict_text.startswith("no detectable"):
        return NONE
    if verdict_text.startswith("provisional"):
        return WITHHELD_PROVISIONAL
    if verdict_text.startswith("unstable"):
        return WITHHELD_UNSTABLE
    return OTHER


def run_suite(cfg, replicates, effect, seed, verify_every=500):
    """Drive `sweep.analyse` over synthetic campaigns and tally the verdicts."""
    rng = random.Random(seed)
    template_base = [
        # The `-cold` suffix is load-bearing: `METRICS` selects a speed record
        # by `id_contains`, so an id without it matches no metric and `analyse`
        # returns an empty analysis. That is how this suite came to report a
        # zero effect rate at a zero critical value, which the calibration
        # endpoint correctly refused to accept.
        build("speed-m-cold#%d" % i, i, "baseline", None, None,
              [BASE_RATE] * cfg["n_samples"])
        for i in range(cfg["n_baseline"])
    ]
    template_level = build("speed-m-cold#9", cfg["n_baseline"], "level", "f", "x",
                           [BASE_RATE] * cfg["n_samples"])
    for record in template_base + [template_level]:
        if record["invalid"]:
            raise SystemExit("template record carries invalid %s; the suite would "
                             "measure the disqualifier" % record["invalid"])
    cont = container(template_base + [template_level], cfg["n_baseline"])

    tally = {}
    onesigma_effect = 0
    deltas = []
    between_cvs = []
    thresholds = []
    verified = 0
    verify_failures = 0
    signed_correct = 0

    for index in range(replicates):
        baseline, level = replicate(rng, cfg, effect)
        for record, rates in zip(template_base, baseline):
            patch(record, rates)
        patch(template_level, level)

        if index % verify_every == 0:
            rebuilt = harness.build_record(
                probe="speed", record_id="v", label="m", provenance_block=PROV,
                conditions_block=harness.conditions(run_index=0, run_order="randomised"),
                engine={"n_ctx": 32768, "n_ctx_slot_tok": 32768, "n_parallel": 1},
                campaign_id="c", samples=list(template_level["samples"]),
                stats=harness.stats_for(template_level["samples"]))
            verified += 1
            if rebuilt["invalid"]:
                verify_failures += 1

        analysis, _unmatched, _mismatches = sweep.analyse(cont)
        block = analysis.get("decode_tps cold")
        if not block or not block["rows"]:
            tally[OTHER] = tally.get(OTHER, 0) + 1
            continue
        row = block["rows"][0]
        outcome = classify(row["verdict"])
        tally[outcome] = tally.get(outcome, 0) + 1
        delta = row["delta_pct"]
        cv = block["baseline"]["between_run_cv"]
        if delta is not None:
            deltas.append(delta)
        if cv is not None:
            between_cvs.append(cv)
            # The rule the threshold replaced, scored on the same replicate:
            # I11 read literally, delta against the bare between-run cv.
            if delta is not None and abs(delta) > 100.0 * cv:
                onesigma_effect += 1
        if block["threshold_pct"] is not None:
            thresholds.append(block["threshold_pct"])
        if outcome == EFFECT:
            better = delta > 0
            if (effect > 1.0 and better) or (effect < 1.0 and not better):
                signed_correct += 1

    return {
        "tally": tally,
        "replicates": replicates,
        "effect_rate": tally.get(EFFECT, 0) / float(replicates),
        "onesigma_rate": onesigma_effect / float(replicates),
        "signed_correct": signed_correct,
        "median_delta_abs": (sorted(abs(d) for d in deltas)[len(deltas) // 2]
                             if deltas else None),
        "realised_between_cv": (sorted(between_cvs)[len(between_cvs) // 2]
                                if between_cvs else None),
        "realised_threshold_pct": (sorted(thresholds)[len(thresholds) // 2]
                                   if thresholds else None),
        "verified": verified,
        "verify_failures": verify_failures,
    }


def cfg(n_baseline=5, between=0.0139, within=0.0068, n_samples=5,
        shape="normal", level_spread=1.0):
    return {"n_baseline": n_baseline, "between": between, "within": within,
            "n_samples": n_samples, "shape": shape, "level_spread": level_spread}


# ---------------------------------------------------------------------------
# Calibration: the instrument before the measurement
# ---------------------------------------------------------------------------


def calibrate(replicates):
    """The endpoints a working instrument must reach.

    At an absurd critical value every comparison must land on no detectable
    effect; at zero every comparison must land on an effect. A harness that
    cannot produce both endpoints is measuring something other than the verdict,
    and its middle numbers mean nothing.
    """
    print("CALIBRATION")
    saved_t95 = dict(sweep.T95)
    saved_large = sweep.T95_LARGE
    n = min(400, replicates)
    try:
        sweep.T95 = {k: 1e9 for k in saved_t95}
        sweep.T95_LARGE = 1e9
        high = run_suite(cfg(), n, 1.0, seed=11)
        sweep.T95 = {k: 0.0 for k in saved_t95}
        sweep.T95_LARGE = 0.0
        low = run_suite(cfg(), n, 1.0, seed=11)
    finally:
        sweep.T95 = saved_t95
        sweep.T95_LARGE = saved_large
    print("  t critical 1e9   effect rate %6.2f %%   expected 0" % (100 * high["effect_rate"]))
    print("  t critical 0     effect rate %6.2f %%   expected 100" % (100 * low["effect_rate"]))
    ok = high["effect_rate"] == 0.0 and low["effect_rate"] > 0.99
    print("  %s\n" % ("instrument reaches both endpoints"
                      if ok else "INSTRUMENT NOT CALIBRATED, numbers below are void"))
    return ok


# ---------------------------------------------------------------------------
# Suites
# ---------------------------------------------------------------------------

# Within-run dispersion is zero on the configurations that reproduce a real
# grid. The two grids print a between-run cv smaller than the within-run noise
# on a five-sample median would produce on its own, so an independent-noise
# generator cannot hit both figures at once. The between-run cv is the one the
# threshold is derived from, so that is the one held to the measured value; the
# within-run term is exercised separately in the rows that name it.
NULL_CONFIGS = [
    ("n=5  between 0.0012, qwen prefill cold", cfg(5, 0.0012, 0.0)),
    ("n=5  between 0.0139, qwen decode cold", cfg(5, 0.0139, 0.0)),
    ("n=5  between 0.0155, dense decode cold", cfg(5, 0.0155, 0.0)),
    ("n=5  between 0.0368, dense warm ttft", cfg(5, 0.0368, 0.0)),
    ("n=3  between 0.0139", cfg(3, 0.0139, 0.0)),
    ("n=8  between 0.0139", cfg(8, 0.0139, 0.0)),
    ("n=12 between 0.0139", cfg(12, 0.0139, 0.0)),
    ("n=20 between 0.0139", cfg(20, 0.0139, 0.0)),
    ("n=5  between 0.0139, lognormal", cfg(5, 0.0139, 0.0, shape="lognormal")),
    ("n=5  between 0.0139, within 0.007", cfg(5, 0.0139, 0.0068)),
    ("n=5  between 0.0139, within 0.05", cfg(5, 0.0139, 0.05)),
    ("n=5  between 0.0139, level 2x dispersion", cfg(5, 0.0139, 0.0, level_spread=2.0)),
    ("n=5  between 0.0139, level 3x dispersion", cfg(5, 0.0139, 0.0, level_spread=3.0)),
    ("n=5  between 0.0012, level 2x dispersion", cfg(5, 0.0012, 0.0, level_spread=2.0)),
]

POWER_CONFIGS = [
    ("qwen prefill cold,  between 0.0012, grid threshold 0.4 %", cfg(5, 0.0012, 0.0)),
    ("qwen decode cold,   between 0.0139, grid threshold 4.2 %", cfg(5, 0.0139, 0.0)),
    ("dense decode cold,  between 0.0155, grid threshold 4.7 %", cfg(5, 0.0155, 0.0)),
    ("dense warm ttft,    between 0.0368, grid threshold 11.2 %", cfg(5, 0.0368, 0.0)),
    ("qwen aggregate,     between 0.0042, grid threshold 1.3 %", cfg(5, 0.0042, 0.0)),
]

EFFECT_SIZES = (0.005, 0.01, 0.02, 0.04, 0.08, 0.16)


def render_null(replicates):
    print("NULL SUITE, %d replicates per configuration" % replicates)
    print("  no effect exists by construction; anything called an effect is a "
          "false positive\n")
    print("  %-42s %9s %9s %9s %9s %9s"
          % ("configuration", "effect", "1-sigma", "withheld", "cv seen", "thr seen"))
    rows = []
    for index, (label, config) in enumerate(NULL_CONFIGS):
        result = run_suite(config, replicates, 1.0, seed=1000 + index)
        withheld = (result["tally"].get(WITHHELD_PROVISIONAL, 0)
                    + result["tally"].get(WITHHELD_UNSTABLE, 0)
                    + result["tally"].get(OTHER, 0))
        print("  %-42s %8.2f %% %8.2f %% %8.2f %% %9.4f %8.2f %%"
              % (label, 100 * result["effect_rate"], 100 * result["onesigma_rate"],
                 100.0 * withheld / replicates,
                 result["realised_between_cv"] or float("nan"),
                 result["realised_threshold_pct"] or float("nan")))
        rows.append((label, result))
    failures = sum(r["verify_failures"] for _, r in rows)
    checked = sum(r["verified"] for _, r in rows)
    print("\n  patched records re-checked through build_record: %d, "
          "invariant failures: %d" % (checked, failures))
    print("  nominal false-positive rate of a two-sided 95 percent interval: 5.00 %\n")
    return rows


def render_power(replicates):
    print("POWER SUITE, %d replicates per cell" % replicates)
    print("  a known effect injected; the cell is the fraction detected, with "
          "the right sign\n")
    header = "".join("%9s" % ("%.1f %%" % (100 * d)) for d in EFFECT_SIZES)
    print("  %-58s%s" % ("configuration", header))
    rows = []
    for index, (label, config) in enumerate(POWER_CONFIGS):
        cells = []
        for offset, delta in enumerate(EFFECT_SIZES):
            result = run_suite(config, replicates, 1.0 + delta,
                               seed=2000 + index * 100 + offset)
            cells.append(result["signed_correct"] / float(replicates))
        print("  %-58s%s" % (label, "".join("%8.1f%%" % (100 * c) for c in cells)))
        rows.append((label, cells))
    print("\n  50 percent power and 80 percent power, interpolated:")
    for label, cells in rows:
        print("    %-58s %s" % (label, interpolate_power(cells)))
    print()
    return rows


def interpolate_power(cells):
    """Effect size at 50 and 80 percent power, linear between measured points."""
    out = []
    for target in (0.5, 0.8):
        found = None
        for i in range(1, len(cells)):
            if cells[i - 1] < target <= cells[i]:
                span = cells[i] - cells[i - 1]
                frac = (target - cells[i - 1]) / span if span else 0.0
                found = EFFECT_SIZES[i - 1] + frac * (EFFECT_SIZES[i] - EFFECT_SIZES[i - 1])
                break
        if found is None:
            found = 0.0 if cells and cells[0] >= target else None
        out.append("%s power at %s"
                   % ("%.0f %%" % (100 * target),
                      "below %.1f %%" % (100 * EFFECT_SIZES[0]) if found == 0.0
                      else ("above %.1f %%" % (100 * EFFECT_SIZES[-1]) if found is None
                            else "%.2f %%" % (100 * found))))
    return ", ".join(out)


def main(argv):
    replicates = int(argv[0]) if argv else 10000
    if not calibrate(replicates):
        return 1
    render_null(replicates)
    render_power(replicates)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
