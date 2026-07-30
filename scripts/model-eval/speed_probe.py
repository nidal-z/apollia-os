#!/usr/bin/env python3
"""Cold and warm latency and throughput against a running llama-server.

Produces two records, never one. Cold and warm prefill are different
quantities, and the previous version of this probe reported the second of two
identical requests, which is a cache-hit latency wearing a prefill label. Each
record here carries the `cache_state` observed on every one of its samples, so a
mislabelled measurement fails the campaign instead of entering it.

Cold is obtained with `cache_prompt: false` rather than by restarting the server
or varying the prompt. Neither of those works: a fresh server still serves a
warm request when the prompt shares the chat template prefix, measured at 534
cached tokens on this machine.

Env:
  BASE_URL     (default http://127.0.0.1:8080/v1)
  MODEL        (default "local")   -> sent as the "model" field
  LABEL        (default $MODEL)    -> model label in the record
  MAX_TOKENS   (default 200)
  REPS         (default 5)         -> repetitions per record, contract minimum
  PROMPT_TOK   (default 2048)      -> prompt size for the measured requests
  MODEL_PATH, LLAMA_BIN, LAUNCH_ARGS, N_CTX, CAMPAIGN_ID, RUN_INDEX
  RUN_ORDER, PAGE_CACHE, ENGINE_EXTRA, MODEL_SHA256  -> set by sweep.py
  OUT          (optional)          -> write a campaign file here

Prints one JSON campaign container on stdout. Stdlib only.
"""

import json
import os
import sys

import harness

BASE_URL = os.environ.get("BASE_URL", "http://127.0.0.1:8080/v1").rstrip("/")
MODEL = os.environ.get("MODEL", "local")
LABEL = os.environ.get("LABEL", MODEL)
MAX_TOKENS = int(os.environ.get("MAX_TOKENS", "200"))
REPS = int(os.environ.get("REPS", "5"))
PROMPT_TOK = int(os.environ.get("PROMPT_TOK", "2048"))
MODEL_PATH = os.environ.get("MODEL_PATH", "")
LLAMA_BIN = os.environ.get("LLAMA_BIN") or None
LAUNCH_ARGS = json.loads(os.environ.get("LAUNCH_ARGS", "[]"))
N_CTX = int(os.environ.get("N_CTX", "0")) or None
CAMPAIGN_ID = os.environ.get("CAMPAIGN_ID") or None
RUN_INDEX = int(os.environ.get("RUN_INDEX", "0"))
OUT = os.environ.get("OUT") or None
OVERLAY = harness.env_overlay()

QUESTION = "\n\nRepond en trois phrases: quel est le sujet de ce texte?"


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

    # Attempted once, and whatever it returns is what the records claim. Erase
    # is 501 unless the server ran with --slot-save-path, which is why cold is
    # obtained from cache_prompt=false rather than from a reset.
    slot_reset = harness.reset_slot(BASE_URL)

    cold_samples = []
    warm_samples = []
    for rep in range(REPS):
        # A distinct prompt per repetition, so no repetition can be served from
        # the previous one's cache even if cache_prompt were ignored.
        prompt = (
            harness.filler_for_tokens(BASE_URL, PROMPT_TOK, salt=rep + 1) + QUESTION
        )
        cold_samples.append(
            harness.measure(
                BASE_URL,
                MODEL,
                prompt,
                max_tokens=MAX_TOKENS,
                cache_prompt=False,
                n_ctx_slot_tok=slot_tok,
            )
        )
        # The warm path measured deliberately: prime the slot with this exact
        # prompt, then measure the resend. That is the shape of a second ReAct
        # iteration, and it is a real quantity, just not a prefill rate.
        harness.measure(
            BASE_URL, MODEL, prompt, max_tokens=1, cache_prompt=True,
            n_ctx_slot_tok=slot_tok,
        )
        warm_samples.append(
            harness.measure(
                BASE_URL,
                MODEL,
                prompt,
                max_tokens=MAX_TOKENS,
                cache_prompt=True,
                n_ctx_slot_tok=slot_tok,
            )
        )

    records = []
    for name, samples, note in (
        (
            "cold",
            cold_samples,
            "cache_prompt=false forces full recomputation; cache_state is read "
            "back from timings.cache_n and not assumed from the flag",
        ),
        (
            "warm",
            warm_samples,
            "slot primed with the identical prompt, then measured; prefill_tps "
            "here is fixed per-request cost, not a throughput",
        ),
    ):
        records.append(
            harness.build_record(
                probe="speed",
                record_id="speed-%s-%s" % (LABEL, name),
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

    container = {
        "schema_version": harness.SCHEMA_VERSION,
        "campaign_id": CAMPAIGN_ID or ("speed-%s" % LABEL),
        "started_at": started,
        "finished_at": harness.now_rfc3339(),
        "records": records,
        "records_excluded": [],
    }
    if OUT:
        harness.write_campaign(OUT, container["campaign_id"], records, started)
    return container


if __name__ == "__main__":
    try:
        sys.stdout.write(json.dumps(main(), ensure_ascii=False) + "\n")
    except (harness.ProbeError, OSError, ValueError) as exc:
        sys.stderr.write("speed_probe: %s\n" % exc)
        raise SystemExit(1)
