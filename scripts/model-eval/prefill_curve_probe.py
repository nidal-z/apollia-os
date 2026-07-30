#!/usr/bin/env python3
"""Prefill rate as a function of prompt length, on cold requests only.

Prefill is what dominates a long agentic turn, and it was the one dimension the
harness never varied. A single-length speed figure says nothing about whether
the engine holds its rate at 16k, which is where a ReAct loop actually lives.

Every point is cold, obtained with `cache_prompt: false` and asserted from
`timings.cache_n` afterwards. A warm point would report fixed per-request cost
as though it were a rate, and the curve would flatten for a reason that has
nothing to do with prefill.

The x-axis is the **measured** `prompt_tok_total`, not the requested target. The
chat template contributes a constant prefix, measured at 535 tokens for
Ministral 3 on this machine, so the two differ by that offset.

Env:
  BASE_URL     (default http://127.0.0.1:8080/v1)
  MODEL        (default "local")
  LABEL        (default $MODEL)
  REPS         (default 5)
  LENGTHS      (default 512,1024,2048,4096,8192,16384,32768)
  MAX_TOKENS   (default 8)  -> short: this probe measures prefill, not decode
  MODEL_PATH, LLAMA_BIN, LAUNCH_ARGS, N_CTX, CAMPAIGN_ID, RUN_INDEX, OUT
  RUN_ORDER, PAGE_CACHE, ENGINE_EXTRA, MODEL_SHA256  -> set by sweep.py

Prints one JSON campaign container on stdout. Stdlib only.
"""

import json
import os
import sys

import harness

BASE_URL = os.environ.get("BASE_URL", "http://127.0.0.1:8080/v1").rstrip("/")
MODEL = os.environ.get("MODEL", "local")
LABEL = os.environ.get("LABEL", MODEL)
REPS = int(os.environ.get("REPS", "5"))
LENGTHS = [
    int(part)
    for part in os.environ.get(
        "LENGTHS", "512,1024,2048,4096,8192,16384,32768"
    ).split(",")
    if part.strip()
]
MAX_TOKENS = int(os.environ.get("MAX_TOKENS", "8"))
MODEL_PATH = os.environ.get("MODEL_PATH", "")
LLAMA_BIN = os.environ.get("LLAMA_BIN") or None
LAUNCH_ARGS = json.loads(os.environ.get("LAUNCH_ARGS", "[]"))
N_CTX = int(os.environ.get("N_CTX", "0")) or None
CAMPAIGN_ID = os.environ.get("CAMPAIGN_ID") or None
RUN_INDEX = int(os.environ.get("RUN_INDEX", "0"))
OUT = os.environ.get("OUT") or None
OVERLAY = harness.env_overlay()

# Room for the template prefix and the generation, so the longest point does not
# silently overflow the slot and get truncated into a shorter measurement.
SLOT_MARGIN_TOK = 1024


def describe_shape(points):
    """Say what the curve does. A plot nobody reads is a plot nobody checks."""
    usable = [p for p in points if p["prefill_tps"] is not None]
    if len(usable) < 2:
        return "too few points to characterise the shape"
    first = usable[0]
    best = max(usable, key=lambda p: p["prefill_tps"])
    last = usable[-1]
    drop_pct = 100.0 * (1.0 - last["prefill_tps"] / best["prefill_tps"])
    rise_pct = 100.0 * (best["prefill_tps"] / first["prefill_tps"] - 1.0)
    parts = []
    if rise_pct > 5.0:
        parts.append(
            "rises %.0f percent from %d to %d tokens, where fixed per-request "
            "cost stops dominating"
            % (rise_pct, first["prompt_tok_total"], best["prompt_tok_total"])
        )
    else:
        parts.append("is already at its peak by %d tokens" % first["prompt_tok_total"])
    if drop_pct > 5.0:
        parts.append(
            "then falls %.0f percent by %d tokens, which is the quadratic "
            "attention term the roofline deliberately does not model"
            % (drop_pct, last["prompt_tok_total"])
        )
    else:
        parts.append(
            "then holds flat within %.0f percent out to %d tokens"
            % (abs(drop_pct), last["prompt_tok_total"])
        )
    return "The curve " + ", ".join(parts) + "."


def plot(points):
    """Text plot. Width scaled to the fastest point."""
    usable = [p for p in points if p["prefill_tps"] is not None]
    if not usable:
        return ["no usable point"]
    peak = max(p["prefill_tps"] for p in usable)
    lines = ["%9s  %9s  %s" % ("prompt", "tok/s", "")]
    for point in points:
        rate = point["prefill_tps"]
        if rate is None:
            lines.append("%9d  %9s  %s" % (point["prompt_tok_total"], "n/a", ""))
            continue
        bars = int(round(48.0 * rate / peak))
        lines.append(
            "%9d  %9.0f  %s" % (point["prompt_tok_total"], rate, "#" * max(1, bars))
        )
    return lines


def main():
    started = harness.now_rfc3339()
    props = harness.server_props(BASE_URL)
    slot_tok = props["n_ctx_slot_tok"] or 0
    engine = harness.engine_block(
        N_CTX or slot_tok,
        slot_tok,
        n_parallel=props.get("total_slots"),
        **OVERLAY["engine_extra"]
    )
    prov = harness.provenance(
        MODEL_PATH, LAUNCH_ARGS, LLAMA_BIN, model_sha256=OVERLAY["model_sha256"]
    )

    slot_reset = harness.reset_slot(BASE_URL)

    targets = [n for n in LENGTHS if n + SLOT_MARGIN_TOK + MAX_TOKENS <= slot_tok]
    skipped = [n for n in LENGTHS if n not in targets]

    all_samples = []
    curve = []
    for index, target in enumerate(targets):
        samples = []
        for rep in range(REPS):
            prompt = harness.filler_for_tokens(
                BASE_URL, target, salt=index * 97 + rep + 1
            )
            samples.append(
                harness.measure(
                    BASE_URL,
                    MODEL,
                    prompt,
                    max_tokens=MAX_TOKENS,
                    cache_prompt=False,
                    n_ctx_slot_tok=slot_tok,
                )
            )
        all_samples.extend(samples)
        stats = harness.stats_for(samples)
        curve.append(
            {
                "prompt_tok_total": int(stats["prompt_tok_total"]["median"]),
                "prefill_tps": (
                    stats["prefill_tps"]["median"] if "prefill_tps" in stats else None
                ),
                "stats": stats,
            }
        )

    notes = (
        "every point cold via cache_prompt=false, asserted from timings.cache_n; "
        "x-axis is measured prompt_tok_total, which exceeds the requested target "
        "by the chat template prefix"
    )
    if skipped:
        notes += "; lengths %s skipped, they do not fit a %d token slot with a %d token margin" % (
            ",".join(str(n) for n in skipped),
            slot_tok,
            SLOT_MARGIN_TOK,
        )

    record = harness.build_record(
        probe="prefill_curve",
        record_id="prefill-curve-%s" % LABEL,
        label=LABEL,
        provenance_block=prov,
        conditions_block=harness.conditions(
            run_index=RUN_INDEX,
            run_order=OVERLAY["run_order"],
            page_cache=OVERLAY["page_cache"],
            slot_reset_before=slot_reset,
            notes=notes,
        ),
        engine=engine,
        campaign_id=CAMPAIGN_ID,
        samples=all_samples,
        stats=harness.stats_for(all_samples),
        extra={
            "prefill_curve": [
                {"prompt_tok_total": p["prompt_tok_total"], "stats": p["stats"]}
                for p in curve
            ]
        },
    )

    sys.stderr.write("\n".join(plot(curve)) + "\n\n" + describe_shape(curve) + "\n")

    container = {
        "schema_version": harness.SCHEMA_VERSION,
        "campaign_id": CAMPAIGN_ID or ("prefill-curve-%s" % LABEL),
        "started_at": started,
        "finished_at": harness.now_rfc3339(),
        "records": [record],
        "records_excluded": [],
        "prefill_curve_plot": plot(curve),
        "prefill_curve_shape": describe_shape(curve),
    }
    if OUT:
        harness.write_campaign(OUT, container["campaign_id"], [record], started)
    return container


if __name__ == "__main__":
    try:
        sys.stdout.write(json.dumps(main(), ensure_ascii=False) + "\n")
    except (harness.ProbeError, OSError, ValueError) as exc:
        sys.stderr.write("prefill_curve_probe: %s\n" % exc)
        raise SystemExit(1)
