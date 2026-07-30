#!/usr/bin/env python3
"""Wrap the quality probes' output in contract-shaped records.

The scoring logic of `toolcall_probe.py` and `esrs_probe.py` is untouched: they
are scored, not timed, so neither the warm-cache defect nor the chunk-counting
defect applies to them. What they lacked was a provenance block and a place in
the campaign container, which is all this adds.

Their payloads keep their existing shape, per the schema's `toolcall`, `esrs`
and `batch` blocks. They carry no `samples`, so they carry no dispersion, so
they are single-observation records and say so in `notes`.

An argument that is the empty string is skipped rather than wrapped, so a caller
that runs only the tool-calling probe writes only that record. `sweep.py` uses
this as the quality gate on the KV quantisation levels.

Usage: quality_record.py <label> <toolcall-json> <esrs-json> <batch-json>
Env:   QUALITY_OUT (required), MODEL_PATH, LLAMA_BIN, LAUNCH_ARGS, N_CTX,
       BASE_URL, CAMPAIGN_ID, RUN_INDEX, RECORD_SUFFIX,
       RUN_ORDER, PAGE_CACHE, ENGINE_EXTRA, MODEL_SHA256
"""

import json
import os
import sys

import harness


def parse(text):
    try:
        return json.loads(text)
    except (json.JSONDecodeError, TypeError):
        return {"raw": str(text)[:200]}


def main(argv):
    if len(argv) != 4:
        sys.stderr.write("usage: quality_record.py <label> <tool> <esrs> <batch>\n")
        return 2
    label, tool, esrs, batch = argv
    out = os.environ.get("QUALITY_OUT")
    if not out:
        sys.stderr.write("quality_record: QUALITY_OUT is unset\n")
        return 2

    started = harness.now_rfc3339()
    overlay = harness.env_overlay()
    base_url = os.environ.get("BASE_URL", "http://127.0.0.1:8080/v1")
    try:
        props = harness.server_props(base_url)
        slot_tok = props["n_ctx_slot_tok"]
        slots = props.get("total_slots")
    except (OSError, ValueError):
        slot_tok = None
        slots = None
    n_ctx = int(os.environ.get("N_CTX", "0")) or slot_tok
    prov = harness.provenance(
        os.environ.get("MODEL_PATH", ""),
        json.loads(os.environ.get("LAUNCH_ARGS", "[]")),
        os.environ.get("LLAMA_BIN") or None,
        model_sha256=overlay["model_sha256"],
    )
    engine = harness.engine_block(
        n_ctx, slot_tok, n_parallel=slots, **overlay["engine_extra"]
    )
    campaign_id = os.environ.get("CAMPAIGN_ID") or ("quality-%s" % label)
    run_index = int(os.environ.get("RUN_INDEX", "0"))
    suffix = os.environ.get("RECORD_SUFFIX") or ""

    records = []
    for probe, raw in (("toolcall", tool), ("esrs", esrs), ("batch", batch)):
        if not (raw or "").strip():
            continue
        records.append(
            harness.build_record(
                probe=probe,
                record_id="%s-%s%s" % (probe, label, suffix),
                label=label,
                provenance_block=prov,
                conditions_block=harness.conditions(
                    run_index=run_index,
                    run_order=overlay["run_order"],
                    page_cache=overlay["page_cache"],
                    notes="single observation, no dispersion: this probe scores "
                    "output rather than timing it, so it is provisional under I8 "
                    "and excluded from comparisons",
                ),
                engine=engine,
                campaign_id=campaign_id,
                extra={probe: parse(raw)},
            )
        )
    harness.write_campaign(out, campaign_id, records, started)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
