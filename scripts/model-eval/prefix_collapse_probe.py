#!/usr/bin/env python3
"""Whether a ReAct continuation reuses its prefix, as a function of what ran before.

1.12 finding 1 records the same probe, the same model and the same prompt shape
serving 86.1 percent of the continuation from cache against a fresh server and
0.02 percent after longer sequences had exercised the slot. `cache_reuse_probe`
cannot separate the two: it always runs the pair alone. This probe puts the
history under the experimenter's control, so the condition that collapses reuse
is a parameter of the run rather than an accident of the campaign order.

The sequence is declared, executed in order, and written verbatim into
`conditions.notes` of every record it produces, so a claim can never be read
without the requests that produced it.

  SEQUENCE grammar, comma separated, executed once per repetition:
    pre:<tokens>:<cold|warm>   one intervening request of that many tokens
    trap                       one request of exactly 4 + n_ubatch + 1 tokens,
                               the length that leaves an unerasable checkpoint
    pair                       the ReAct shape: a cold seed, then the same
                               prefix plus a short appended result

`pre` steps use a filler salt disjoint from the pair's, so an intervening
request shares no prefix with the measurement it precedes. Reuse is read back
from `timings.cache_n` on every sample, as 1.4.4 requires, and never asserted
from the shape of the sequence.

Env:
  BASE_URL     (default http://127.0.0.1:8080/v1)
  MODEL        (default "local")
  LABEL        (default $MODEL)
  CONDITION    (default "baseline")  -> names the launch delta under test
  SEQUENCE     (default "pair")
  PREAMBLE     (default none)   -> `pre` steps run once before any measurement
  PREAMBLE_REPS (default 1)     -> how many times the preamble is run
  REPS         (default 5)
  PROMPT_TOK   (default 4096)   -> the long prefix
  SUFFIX_TOK   (default 64)     -> the appended tool result
  MAX_TOKENS   (default 32)
  PRE_MAX_TOKENS (default 8)    -> intervening requests measure nothing
  MODEL_PATH, LLAMA_BIN, LAUNCH_ARGS, N_CTX, CAMPAIGN_ID, RUN_INDEX, OUT

Prints one JSON campaign container on stdout. Stdlib only.
"""

import json
import os
import sys

import harness

BASE_URL = os.environ.get("BASE_URL", "http://127.0.0.1:8080/v1").rstrip("/")
MODEL = os.environ.get("MODEL", "local")
LABEL = os.environ.get("LABEL", MODEL)
CONDITION = os.environ.get("CONDITION", "baseline")
SEQUENCE = os.environ.get("SEQUENCE", "pair")
PREAMBLE = os.environ.get("PREAMBLE", "")
PREAMBLE_REPS = int(os.environ.get("PREAMBLE_REPS", "1"))
REPS = int(os.environ.get("REPS", "5"))
PROMPT_TOK = int(os.environ.get("PROMPT_TOK", "4096"))
SUFFIX_TOK = int(os.environ.get("SUFFIX_TOK", "64"))
MAX_TOKENS = int(os.environ.get("MAX_TOKENS", "32"))
PRE_MAX_TOKENS = int(os.environ.get("PRE_MAX_TOKENS", "8"))
N_UBATCH = int(os.environ.get("N_UBATCH", "512"))
MODEL_PATH = os.environ.get("MODEL_PATH", "")
LLAMA_BIN = os.environ.get("LLAMA_BIN") or None
LAUNCH_ARGS = json.loads(os.environ.get("LAUNCH_ARGS", "[]"))
N_CTX = int(os.environ.get("N_CTX", "0")) or None
CAMPAIGN_ID = os.environ.get("CAMPAIGN_ID") or None
RUN_INDEX = int(os.environ.get("RUN_INDEX", "0"))
OUT = os.environ.get("OUT") or None

# Salt ranges kept apart so no two texts in a sequence share a prefix. 1.4.4:
# a request that accidentally shares a prefix with an earlier one is warm
# whatever the probe intended.
SALT_PREFIX = 101
SALT_SUFFIX = 811
SALT_PRE = 2503
SALT_CALIB = 4001
SALT_TRAP = 5501


def parse_sequence(spec, require_pair=True):
    """The declared sequence, as a list of steps. Rejects anything it cannot run.

    A malformed step is a ProbeError rather than a skipped request: a sequence
    silently shorter than the one written in the record's notes would put a
    false statement in the provenance of every claim that followed.

    `require_pair` is false for the preamble, which is history rather than
    measurement and therefore carries no pair of its own.
    """
    steps = []
    for index, raw in enumerate(spec.split(",")):
        token = raw.strip()
        if not token:
            continue
        if token == "pair":
            steps.append({"kind": "pair"})
            continue
        if token == "trap":
            steps.append({"kind": "trap", "index": index})
            continue
        parts = token.split(":")
        if parts[0] != "pre" or len(parts) not in (2, 3):
            raise harness.ProbeError(
                "unknown sequence step %r, expected 'pair', 'trap' or "
                "'pre:<tokens>[:cold|warm]'" % token
            )
        try:
            tokens = int(parts[1])
        except ValueError:
            raise harness.ProbeError(
                "step %r: %r is not a token count" % (token, parts[1])
            )
        state = parts[2] if len(parts) == 3 else "cold"
        if state not in ("cold", "warm"):
            raise harness.ProbeError("step %r: %r is not cold or warm" % (token, state))
        steps.append({"kind": "pre", "tokens": tokens, "intent": state, "index": index})
    if require_pair and not any(step["kind"] == "pair" for step in steps):
        raise harness.ProbeError(
            "sequence %r contains no pair, so it measures nothing" % spec
        )
    if not require_pair and any(step["kind"] == "pair" for step in steps):
        raise harness.ProbeError(
            "preamble %r contains a pair; the preamble is history, not measurement"
            % spec
        )
    return steps


def exact_filler(base_url, target_tok, salt):
    """Filler whose own token count is exactly `target_tok`, or None.

    `filler_for_tokens` converges to within a tolerance, which is enough for a
    curve whose x-axis is the measured length. It is not enough here: the step
    this serves depends on the prompt landing on one specific total, so a two
    percent miss is the difference between reproducing the effect and not.
    Padding is a single token at a time; a pad that does not move the count is
    a dead end and returns None rather than looping.
    """
    text = harness.filler_text(max(1, int(target_tok * 0.6)), salt)
    count = harness.count_tokens(base_url, text)
    if count is None:
        return None
    while count > target_tok and " " in text:
        text = text.rsplit(" ", 1)[0]
        count = harness.count_tokens(base_url, text)
    while count < target_tok:
        text = text + " a"
        moved = harness.count_tokens(base_url, text)
        if moved is None or moved <= count:
            return None
        count = moved
    return text if count == target_tok else None


def trap_prompt(base_url, model, slot_tok, buckets, rep):
    """The shortest prompt that poisons the slot, plus the calibration behind it.

    A checkpoint whose `pos_max` is 0 is never erased by the invalidation loop,
    which drops only checkpoints past the reuse point. It is created when the
    engine checkpoints a slot holding exactly one token, which happens when a
    prompt is `4 + n_ubatch + 1` tokens long. The chat template contributes a
    constant to that total, so the template's own cost is measured here rather
    than assumed, with one request whose length is nowhere near the target.
    """
    target_total = N_UBATCH + 5

    calib_text = harness.filler_for_tokens(base_url, 2048, salt=SALT_CALIB + rep)
    calib = harness.measure(
        base_url,
        model,
        calib_text,
        max_tokens=1,
        cache_prompt=False,
        n_ctx_slot_tok=slot_tok,
    )
    buckets.setdefault("calibration", []).append(calib)
    counted = harness.count_tokens(base_url, calib_text)
    if counted is None:
        raise harness.ProbeError(
            "the engine's tokenizer is unavailable, so no exact length can be built"
        )
    overhead = calib["prompt_tok_total"] - counted
    if overhead < 0 or overhead >= target_total:
        raise harness.ProbeError(
            "measured chat template overhead of %d tokens leaves no room for a %d token prompt"
            % (overhead, target_total)
        )

    text = exact_filler(base_url, target_total - overhead, salt=SALT_TRAP + rep)
    if text is None:
        raise harness.ProbeError(
            "could not build a prompt of exactly %d tokens, so the step cannot be run"
            % target_total
        )
    sample = harness.measure(
        base_url,
        model,
        text,
        max_tokens=PRE_MAX_TOKENS,
        cache_prompt=False,
        n_ctx_slot_tok=slot_tok,
    )
    # Stated intent, observed outcome. A trap request that missed its length is
    # a different experiment and says so instead of being averaged in.
    if sample["prompt_tok_total"] != target_total:
        raise harness.ProbeError(
            "trap request was %d tokens, not the %d it was built for"
            % (sample["prompt_tok_total"], target_total)
        )
    buckets.setdefault("trap-%dtok" % target_total, []).append(sample)


def run_repetition(steps, rep, slot_tok, buckets, bucket_prefix="pre"):
    """One pass through the declared sequence, appending to the per-class buckets."""
    for position, step in enumerate(steps):
        if step["kind"] == "trap":
            trap_prompt(BASE_URL, MODEL, slot_tok, buckets, rep)
            continue
        if step["kind"] == "pre":
            text = harness.filler_for_tokens(
                BASE_URL, step["tokens"], salt=SALT_PRE + position * 37 + rep
            )
            sample = harness.measure(
                BASE_URL,
                MODEL,
                text,
                max_tokens=PRE_MAX_TOKENS,
                # The intent is stated here and the outcome is read back off the
                # response like any other observation.
                cache_prompt=(step["intent"] == "warm"),
                n_ctx_slot_tok=slot_tok,
            )
            name = "%s%d-%dtok" % (bucket_prefix, position, step["tokens"])
            buckets.setdefault(name, []).append(sample)
            continue

        prefix = harness.filler_for_tokens(BASE_URL, PROMPT_TOK, salt=SALT_PREFIX + rep)
        suffix = harness.filler_for_tokens(BASE_URL, SUFFIX_TOK, salt=SALT_SUFFIX + rep)
        buckets.setdefault("seed", []).append(
            harness.measure(
                BASE_URL,
                MODEL,
                prefix,
                max_tokens=MAX_TOKENS,
                cache_prompt=False,
                n_ctx_slot_tok=slot_tok,
            )
        )
        buckets.setdefault("continuation", []).append(
            harness.measure(
                BASE_URL,
                MODEL,
                prefix + "\n\nOutil execute. Resultat:\n" + suffix,
                max_tokens=MAX_TOKENS,
                cache_prompt=True,
                n_ctx_slot_tok=slot_tok,
            )
        )


def summarise(samples):
    """The one line a reader needs, plus the per repetition ratios behind it.

    The ratios are printed in run order rather than aggregated away: if reuse
    holds on the first repetition and collapses on the rest, a median hides the
    only thing worth knowing.
    """
    ratios = [s.get("prompt_cache_hit_ratio") for s in samples]
    cached = harness.aggregate([s["prompt_tok_cached"] for s in samples])
    computed = harness.aggregate([s["prompt_tok_computed"] for s in samples])
    hit = harness.aggregate(ratios)
    if not (cached and computed and hit):
        return "continuation produced no usable sample"
    return (
        "continuation served %d of %d prompt tokens from cache, %.2f percent, "
        "recomputing %d; per repetition %s"
        % (
            int(cached["median"]),
            int(cached["median"] + computed["median"]),
            100.0 * hit["median"],
            int(computed["median"]),
            ", ".join(
                "%.2f%%" % (100.0 * r) if r is not None else "n/a" for r in ratios
            ),
        )
    )


def main():
    started = harness.now_rfc3339()
    steps = parse_sequence(SEQUENCE)
    preamble = parse_sequence(PREAMBLE, require_pair=False) if PREAMBLE else []
    props = harness.server_props(BASE_URL)
    slot_tok = props["n_ctx_slot_tok"]
    engine = harness.engine_block(
        N_CTX or slot_tok, slot_tok, n_parallel=props.get("total_slots")
    )
    prov = harness.provenance(MODEL_PATH, LAUNCH_ARGS, LLAMA_BIN)
    slot_reset = harness.reset_slot(BASE_URL)

    buckets = {}
    # The preamble runs once, before any measurement, and its own repetitions
    # are declared inside it. It exists because the collapse under audit
    # appeared after a whole probe had run, not after a single request.
    for rep in range(PREAMBLE_REPS):
        run_repetition(preamble, rep, slot_tok, buckets, bucket_prefix="preamble")
    for rep in range(REPS):
        run_repetition(steps, rep, slot_tok, buckets)

    note_head = (
        "condition %s; preamble %s x%d; sequence %s; %d repetitions of that "
        "sequence against one server"
        % (
            CONDITION,
            PREAMBLE or "none",
            PREAMBLE_REPS if PREAMBLE else 0,
            SEQUENCE,
            REPS,
        )
    )
    records = []
    for name in sorted(buckets):
        samples = buckets[name]
        records.append(
            harness.build_record(
                probe="prefix_collapse",
                record_id="prefix-collapse-%s-%s-%s" % (LABEL, CONDITION, name),
                label=LABEL,
                provenance_block=prov,
                conditions_block=harness.conditions(
                    run_index=RUN_INDEX,
                    slot_reset_before=slot_reset,
                    notes="%s; this record holds the %s requests of that sequence"
                    % (note_head, name),
                ),
                engine=engine,
                campaign_id=CAMPAIGN_ID,
                samples=samples,
                stats=harness.stats_for(samples),
                extra={"condition": CONDITION, "sequence": SEQUENCE},
            )
        )

    summary = "%s: %s" % (CONDITION, summarise(buckets.get("continuation") or []))
    sys.stderr.write(summary + "\n")

    container = {
        "schema_version": harness.SCHEMA_VERSION,
        "campaign_id": CAMPAIGN_ID or ("prefix-collapse-%s-%s" % (LABEL, CONDITION)),
        "started_at": started,
        "finished_at": harness.now_rfc3339(),
        "records": records,
        "records_excluded": [],
        "prefix_collapse_summary": summary,
    }
    if OUT:
        harness.write_campaign(OUT, container["campaign_id"], records, started)
    return container


if __name__ == "__main__":
    try:
        sys.stdout.write(json.dumps(main(), ensure_ascii=False) + "\n")
    except (harness.ProbeError, OSError, ValueError) as exc:
        sys.stderr.write("prefix_collapse_probe: %s\n" % exc)
        raise SystemExit(1)
