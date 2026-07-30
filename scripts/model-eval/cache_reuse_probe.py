#!/usr/bin/env python3
"""How much of a ReAct turn's prompt the engine serves from cache.

The shape being measured is the one an agentic loop actually produces: a long
prompt, then the same prompt again with a short suffix appended, which is what
happens when a tool result is added to the conversation and the model is asked
to continue. If the engine recomputed the whole prefix each iteration, prefill
would dominate every turn; this probe says how much it actually avoids.

Two records per run, so neither figure can be read as the other:
  - the seeding request, cold, which pays for the whole prefix
  - the continuation, warm, reported with the `prompt_cache_hit_ratio` that
    1.4.4 requires alongside any warm figure

Env:
  BASE_URL     (default http://127.0.0.1:8080/v1)
  MODEL        (default "local")
  LABEL        (default $MODEL)
  REPS         (default 5)
  PROMPT_TOK   (default 4096)   -> the long prefix
  SUFFIX_TOK   (default 64)     -> the appended tool result
  MAX_TOKENS   (default 32)
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
PROMPT_TOK = int(os.environ.get("PROMPT_TOK", "4096"))
SUFFIX_TOK = int(os.environ.get("SUFFIX_TOK", "64"))
MAX_TOKENS = int(os.environ.get("MAX_TOKENS", "32"))
MODEL_PATH = os.environ.get("MODEL_PATH", "")
LLAMA_BIN = os.environ.get("LLAMA_BIN") or None
LAUNCH_ARGS = json.loads(os.environ.get("LAUNCH_ARGS", "[]"))
N_CTX = int(os.environ.get("N_CTX", "0")) or None
CAMPAIGN_ID = os.environ.get("CAMPAIGN_ID") or None
RUN_INDEX = int(os.environ.get("RUN_INDEX", "0"))
OUT = os.environ.get("OUT") or None
OVERLAY = harness.env_overlay()


def main():
    started = harness.now_rfc3339()
    props = harness.server_props(BASE_URL)
    slot_tok = props["n_ctx_slot_tok"]
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

    seed_samples = []
    continuation_samples = []
    for rep in range(REPS):
        prefix = harness.filler_for_tokens(BASE_URL, PROMPT_TOK, salt=rep + 101)
        suffix = harness.filler_for_tokens(BASE_URL, SUFFIX_TOK, salt=rep + 811)

        # Iteration one: the whole prefix is new. Cold by construction and
        # asserted afterwards, never assumed.
        seed_samples.append(
            harness.measure(
                BASE_URL,
                MODEL,
                prefix,
                max_tokens=MAX_TOKENS,
                cache_prompt=False,
                n_ctx_slot_tok=slot_tok,
            )
        )
        # Iteration two: same prefix, plus what a tool would have returned.
        continuation_samples.append(
            harness.measure(
                BASE_URL,
                MODEL,
                prefix + "\n\nOutil execute. Resultat:\n" + suffix,
                max_tokens=MAX_TOKENS,
                cache_prompt=True,
                n_ctx_slot_tok=slot_tok,
            )
        )

    records = []
    for name, samples, note in (
        (
            "seed",
            seed_samples,
            "first iteration of the pair, whole prefix computed",
        ),
        (
            "continuation",
            continuation_samples,
            "second iteration: identical prefix plus a short appended result, "
            "the exact shape of a ReAct step after a tool returns",
        ),
    ):
        records.append(
            harness.build_record(
                probe="cache_reuse",
                record_id="cache-reuse-%s-%s" % (LABEL, name),
                label=LABEL,
                provenance_block=prov,
                conditions_block=harness.conditions(
                    run_index=RUN_INDEX,
                    run_order=OVERLAY["run_order"],
                    page_cache=OVERLAY["page_cache"],
                    slot_reset_before=slot_reset,
                    notes=note,
                ),
                engine=engine,
                campaign_id=CAMPAIGN_ID,
                samples=samples,
                stats=harness.stats_for(samples),
            )
        )

    hit = harness.aggregate([s["prompt_cache_hit_ratio"] for s in continuation_samples])
    cached = harness.aggregate([s["prompt_tok_cached"] for s in continuation_samples])
    computed = harness.aggregate(
        [s["prompt_tok_computed"] for s in continuation_samples]
    )
    summary = (
        "continuation served %d of %d prompt tokens from cache, %.2f percent, "
        "recomputing %d"
        % (
            int(cached["median"]),
            int(cached["median"] + computed["median"]),
            100.0 * hit["median"],
            int(computed["median"]),
        )
    )
    sys.stderr.write(summary + "\n")

    container = {
        "schema_version": harness.SCHEMA_VERSION,
        "campaign_id": CAMPAIGN_ID or ("cache-reuse-%s" % LABEL),
        "started_at": started,
        "finished_at": harness.now_rfc3339(),
        "records": records,
        "records_excluded": [],
        "cache_reuse_summary": summary,
    }
    if OUT:
        harness.write_campaign(OUT, container["campaign_id"], records, started)
    return container


if __name__ == "__main__":
    try:
        sys.stdout.write(json.dumps(main(), ensure_ascii=False) + "\n")
    except (harness.ProbeError, OSError, ValueError) as exc:
        sys.stderr.write("cache_reuse_probe: %s\n" % exc)
        raise SystemExit(1)
