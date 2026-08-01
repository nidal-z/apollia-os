#!/usr/bin/env python3
"""Shared measurement library for the model-eval probes.

Every probe in this directory produces records in the shape fixed by Part 1 of
README.md. That shape is built here, once, so a field cannot be spelled two ways
by two probes, which is the drift the contract exists to prevent.

What lives here:
  - one streamed or non-streamed request, parsed into a sample carrying every
    field of dictionary sections 1.4.1 to 1.4.6
  - aggregation into the dispersion block of 1.4.8
  - the provenance block of 1.4.9 and the run conditions of 1.4.10
  - the invariant checks I1 to I13 of 1.5, computed rather than assumed
  - the campaign container of 1.9

Cold and warm are never asserted by intent. A cold request is obtained with
`cache_prompt: false`, which makes the engine recompute the whole prompt, and
the resulting `cache_state` is then read back off the response like any other
observation. On a server that has already answered once, an unrelated prompt
still hits the chat template prefix: measured at 534 cached tokens on this
machine, which is exactly the trap 1.4.4 describes.

Stdlib only.
"""

import json
import math
import os
import platform
import re
import subprocess
import sys
import time
import urllib.error
import urllib.request

SCHEMA_VERSION = 1
HTTP_TIMEOUT_S = 900.0
REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))

THINK_RE = re.compile(r"<think>(.*?)(?:</think>|$)", re.S)


class ProbeError(Exception):
    """A measurement that cannot be made. Never a silently degraded number."""


# ---------------------------------------------------------------------------
# HTTP
# ---------------------------------------------------------------------------


def _request(url, body=None, headers=None, timeout=HTTP_TIMEOUT_S):
    data = json.dumps(body).encode() if body is not None else None
    merged = {"Content-Type": "application/json"}
    merged.update(headers or {})
    req = urllib.request.Request(url, data=data, headers=merged)
    return urllib.request.urlopen(req, timeout=timeout)


def get_json(url, headers=None, timeout=HTTP_TIMEOUT_S):
    with _request(url, None, headers, timeout) as response:
        return json.loads(response.read())


def post_json(url, body, headers=None, timeout=HTTP_TIMEOUT_S):
    with _request(url, body, headers, timeout) as response:
        return json.loads(response.read())


def server_root(base_url):
    """The server root. `/props` and `/tokenize` sit there, not under `/v1`."""
    trimmed = base_url.rstrip("/")
    return trimmed[: -len("/v1")] if trimmed.endswith("/v1") else trimmed


def server_props(base_url):
    """Per-slot context window and slot count, observed rather than assumed.

    1.4.6: `-c` is the total across slots, so a campaign that reports occupancy
    against the launch value reports a fiction as soon as `-np > 1`. `/props` is
    used rather than `/slots`, which can expose prompt content.
    """
    payload = get_json(server_root(base_url) + "/props", timeout=60)
    settings = payload.get("default_generation_settings") or {}
    return {
        "n_ctx_slot_tok": settings.get("n_ctx"),
        "total_slots": payload.get("total_slots"),
    }


def reset_slot(base_url, slot_id=0):
    """Try to clear the slot's KV state. Returns whether it actually happened.

    `POST /slots/{id}?action=erase` answers 501 unless the server was launched
    with `--slot-save-path`, so this is not available by default and the caller
    records the real outcome rather than the intent. `conditions.slot_reset_before`
    is an observation like any other: claiming a reset that returned 501 would
    put a false statement in the provenance of every record that followed.
    """
    try:
        request = urllib.request.Request(
            "%s/slots/%d?action=erase" % (server_root(base_url), slot_id),
            data=b"{}",
            headers={"Content-Type": "application/json"},
        )
        urllib.request.urlopen(request, timeout=60).read()
        return True
    except (urllib.error.HTTPError, urllib.error.URLError, OSError):
        return False


def count_tokens(base_url, text):
    """Token count from the engine's own tokenizer, or None if unavailable."""
    try:
        payload = post_json(
            server_root(base_url) + "/tokenize", {"content": text}, timeout=120
        )
    except (urllib.error.URLError, OSError, ValueError):
        return None
    tokens = payload.get("tokens")
    return len(tokens) if isinstance(tokens, list) else None


# ---------------------------------------------------------------------------
# Deterministic filler
# ---------------------------------------------------------------------------

_FILLER_WORDS = (
    "alpha bravo charlie delta echo foxtrot golf hotel india juliett kilo lima "
    "mike november oscar papa quebec romeo sierra tango uniform victor whiskey"
).split()


def filler_text(word_count, salt=0):
    """Deterministic filler, reproducible across runs and machines.

    No randomness: the same `(word_count, salt)` yields the same text, so a
    prefill curve rerun a week later is comparable to this one. `salt` shifts
    the sequence so two fillers of the same length share no prefix, which is
    what keeps a cold request cold.
    """
    words = []
    for index in range(word_count):
        pick = _FILLER_WORDS[(index * 7 + salt * 13) % len(_FILLER_WORDS)]
        words.append("%s%d" % (pick, (index + salt) % 1000))
    return " ".join(words)


def filler_for_tokens(base_url, target_tok, salt=0, tolerance=0.02):
    """Filler calibrated to a token target using the engine's own tokenizer.

    The curve's x-axis is the measured `prompt_tok_total`, so this only has to
    land near the target. It converges by ratio, then stops: an exact hit is not
    worth the requests it would cost.
    """
    words = max(1, int(target_tok * 0.6))
    text = filler_text(words, salt)
    for _ in range(6):
        actual = count_tokens(base_url, text)
        if actual is None or actual == 0:
            return text
        if abs(actual - target_tok) <= max(1, target_tok * tolerance):
            return text
        words = max(1, int(words * (float(target_tok) / float(actual))))
        text = filler_text(words, salt)
    return text


# ---------------------------------------------------------------------------
# One measurement
# ---------------------------------------------------------------------------


def _split_reasoning(text):
    """Characters inside and outside `<think>` blocks. Always exact."""
    reasoning = "".join(THINK_RE.findall(text))
    content = THINK_RE.sub("", text)
    return len(reasoning), len(content)


def _is_degenerate(text):
    if len(text) <= 60:
        return False
    window = text[20:32]
    return text.count(window) > 5


def measure(
    base_url,
    model,
    prompt,
    max_tokens=200,
    stream=True,
    cache_prompt=True,
    seed=42,
    temperature=0.0,
    n_ctx_slot_tok=None,
    extra_body=None,
):
    """One request, returned as a sample conforming to 1.4.1 through 1.4.6.

    `cache_prompt=False` forces the engine to recompute the whole prompt, which
    is how a cold measurement is obtained on a warm server. The resulting
    `cache_state` is still read back from `timings.cache_n` rather than assumed:
    the flag states the intent, the response states the fact, and only the
    second one goes in the record.

    The clock origin is taken **before** the request is dispatched. A client
    that starts timing where it begins consuming the response body has already
    spent the prefill, and reports a first-token time that excludes the wait it
    claims to measure. I13 catches exactly that.
    """
    body = {
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "max_tokens": max_tokens,
        "temperature": temperature,
        "seed": seed,
        "stream": bool(stream),
        "cache_prompt": bool(cache_prompt),
    }
    if stream:
        body["stream_options"] = {"include_usage": True}
    if extra_body:
        body.update(extra_body)

    url = base_url.rstrip("/") + "/chat/completions"
    timings = None
    usage = None
    ttft_ms = None
    sse_chunks = 0
    pieces = []

    start = time.perf_counter()
    if stream:
        with _request(url, body) as response:
            for raw in response:
                line = raw.decode("utf-8", "replace").strip()
                if not line.startswith("data:"):
                    continue
                payload = line[len("data:") :].strip()
                if payload == "[DONE]":
                    break
                try:
                    event = json.loads(payload)
                except json.JSONDecodeError:
                    continue
                choices = event.get("choices") or []
                delta = (choices[0].get("delta") or {}) if choices else {}
                piece = delta.get("content") or delta.get("reasoning_content") or ""
                if piece:
                    if ttft_ms is None:
                        ttft_ms = (time.perf_counter() - start) * 1000.0
                    sse_chunks += 1
                    pieces.append(piece)
                if event.get("timings"):
                    timings = event["timings"]
                if event.get("usage"):
                    usage = event["usage"]
    else:
        with _request(url, body) as response:
            payload = json.loads(response.read())
        timings = payload.get("timings")
        usage = payload.get("usage")
        choices = payload.get("choices") or []
        message = (choices[0].get("message") or {}) if choices else {}
        pieces.append(message.get("content") or "")
    request_wall_ms = (time.perf_counter() - start) * 1000.0

    if not timings:
        raise ProbeError(
            "response carried no timings object, so token counts and engine "
            "durations are unavailable and no sample can be built"
        )

    text = "".join(pieces)
    computed = int(timings.get("prompt_n") or 0)
    cached = int(timings.get("cache_n") or 0)
    total = computed + cached
    decode_tok = int(timings.get("predicted_n") or 0)
    prefill_ms = float(timings.get("prompt_ms") or 0.0)
    decode_ms = float(timings.get("predicted_ms") or 0.0)

    cache_state = "cold" if cached == 0 else "warm"
    reasoning_chars, content_chars = _split_reasoning(text)
    chars_total = reasoning_chars + content_chars
    if reasoning_chars > 0 and chars_total > 0 and decode_tok:
        # Apportioned by characters, which assumes a uniform characters-per-token
        # ratio across the two regions. Labelled approximate for that reason.
        split_method = "chars_apportioned"
        reasoning_tok = int(round(decode_tok * (float(reasoning_chars) / chars_total)))
        content_tok = decode_tok - reasoning_tok
    else:
        split_method = "none"
        reasoning_tok = None
        content_tok = decode_tok or None

    sample = {
        "streamed": bool(stream),
        "cache_state": cache_state,
        "prompt_cache_hit_ratio": (float(cached) / total) if total else None,
        "prompt_tok_total": total,
        "prompt_tok_computed": computed,
        "prompt_tok_cached": cached,
        "decode_tok": decode_tok,
        "decode_tok_reasoning": reasoning_tok,
        "decode_tok_content": content_tok,
        "reasoning_split_method": split_method,
        "reasoning_chars": reasoning_chars,
        "content_chars": content_chars,
        # Rule A: the event count never substitutes for the token count. It is
        # kept so the gap stays visible, and it is systematically non-zero.
        "sse_chunks": sse_chunks if stream else None,
        "token_count_discrepancy_ratio": (
            abs(sse_chunks - decode_tok) / float(decode_tok)
            if stream and decode_tok
            else None
        ),
        # I12: undefined without a stream, because there is no first token to
        # observe. Null, not zero.
        "ttft_ms": ttft_ms if stream else None,
        "prefill_ms": prefill_ms,
        "decode_ms": decode_ms,
        "ttft_overhead_ms": (ttft_ms - prefill_ms) if ttft_ms is not None else None,
        "request_wall_ms": request_wall_ms,
        # I4: null on a fully cached prompt, where the figure would be fixed
        # overhead rather than a rate.
        "prefill_tps": (computed / (prefill_ms / 1000.0)) if computed and prefill_ms else None,
        "decode_tps": (decode_tok / (decode_ms / 1000.0)) if decode_tok and decode_ms else None,
        "decode_tps_wall": (
            decode_tok / ((request_wall_ms - ttft_ms) / 1000.0)
            if decode_tok and ttft_ms is not None and request_wall_ms > ttft_ms
            else None
        ),
        "engine_prefill_tps": timings.get("prompt_per_second"),
        "engine_decode_tps": timings.get("predicted_per_second"),
        "degenerate": _is_degenerate(text),
        "empty": len(text.strip()) == 0,
    }

    # I3: the cold and warm first-token fields are admissible only on a record
    # whose observed state matches. Never both, never on the wrong one.
    if ttft_ms is not None:
        sample["ttft_cold_ms"] = ttft_ms if cache_state == "cold" else None
        sample["ttft_warm_ms"] = ttft_ms if cache_state == "warm" else None
    else:
        sample["ttft_cold_ms"] = None
        sample["ttft_warm_ms"] = None

    if n_ctx_slot_tok:
        sample["context_occupancy_at_call_ratio"] = total / float(n_ctx_slot_tok)
        sample["context_occupancy_ratio"] = (total + decode_tok) / float(n_ctx_slot_tok)
    else:
        sample["context_occupancy_at_call_ratio"] = None
        sample["context_occupancy_ratio"] = None

    if usage:
        # Independent counters, kept as a cross-check under Rule A rather than
        # merged with the engine's own.
        sample["_usage_prompt_tokens"] = usage.get("prompt_tokens")
        details = usage.get("prompt_tokens_details") or {}
        sample["_usage_cached_tokens"] = details.get("cached_tokens")
    return sample


# ---------------------------------------------------------------------------
# Dispersion, 1.4.8
# ---------------------------------------------------------------------------


def _median(sorted_values):
    count = len(sorted_values)
    middle = count // 2
    if count % 2:
        return sorted_values[middle]
    return (sorted_values[middle - 1] + sorted_values[middle]) / 2.0


def aggregate(values):
    """The dispersion block. Raw samples are always retained."""
    clean = [float(v) for v in values if v is not None]
    if not clean:
        return None
    ordered = sorted(clean)
    count = len(ordered)
    mean = sum(ordered) / count
    if count > 1:
        variance = sum((v - mean) ** 2 for v in ordered) / (count - 1)
        stdev = math.sqrt(variance)
    else:
        stdev = 0.0
    # Nearest-rank, 1-indexed. At n = 5 this is the maximum, not a tail
    # estimate, which is why 1.4.8 says a p95 carrying information needs n >= 20.
    rank = max(1, int(math.ceil(0.95 * count)))
    return {
        "n": count,
        "median": _median(ordered),
        "p95": ordered[rank - 1],
        "min": ordered[0],
        "max": ordered[-1],
        "mean": mean,
        "cv": (stdev / mean) if mean else 0.0,
        "samples": [float(v) for v in values if v is not None],
    }


AGGREGATED_FIELDS = (
    "ttft_ms",
    "ttft_cold_ms",
    "ttft_warm_ms",
    "prefill_ms",
    "decode_ms",
    "ttft_overhead_ms",
    "request_wall_ms",
    "prefill_tps",
    "decode_tps",
    "decode_tps_wall",
    "prompt_tok_total",
    "prompt_tok_computed",
    "prompt_tok_cached",
    "prompt_cache_hit_ratio",
    "decode_tok",
    "token_count_discrepancy_ratio",
    "context_occupancy_at_call_ratio",
)


def stats_for(samples, fields=AGGREGATED_FIELDS):
    stats = {}
    for field in fields:
        block = aggregate([s.get(field) for s in samples])
        if block is not None:
            stats[field] = block
    return stats


# ---------------------------------------------------------------------------
# Provenance and conditions, 1.4.9 and 1.4.10
# ---------------------------------------------------------------------------


def run_capture(args, merge_stderr=False, timeout=60):
    try:
        result = subprocess.run(
            args, capture_output=True, text=True, timeout=timeout, check=False
        )
    except (OSError, subprocess.SubprocessError):
        return None
    if result.returncode != 0:
        return None
    return (result.stdout + result.stderr) if merge_stderr else result.stdout


def sha256_file(path):
    """SHA-256 of a file, or None. Slow on a 20 GiB model, so hash once."""
    out = run_capture(["shasum", "-a", "256", path], timeout=1800)
    return out.split()[0] if out else None


_sha256 = sha256_file


def sha256_scope(model_path):
    """`first_shard` when the path names a shard, `whole_file` otherwise."""
    return (
        "first_shard"
        if re.search(r"-\d{5}-of-\d{5}\.gguf$", os.path.basename(model_path))
        else "whole_file"
    )


def provenance(
    model_path,
    launch_args,
    llama_server_path=None,
    hash_model=True,
    model_sha256=None,
):
    """The block of 1.4.9, inline in every record. Absent stays null.

    `model_sha256` supplies a hash computed elsewhere. A campaign that restarts
    the server twenty times reloads the same file twenty times, and hashing 20
    GiB on each of them spends more wall-clock on `shasum` than on measurement
    while proving the same thing twenty times. The caller that hashes once is
    responsible for the file not changing under it, which for a sweep holds by
    construction: the path is fixed by the plan.
    """
    sha = model_sha256
    scope = sha256_scope(model_path) if sha else "none"
    if model_path and sha is None and hash_model and os.path.isfile(model_path):
        sha = _sha256(model_path)
        if sha:
            scope = sha256_scope(model_path)
    version = None
    if llama_server_path:
        raw = run_capture([llama_server_path, "--version"], merge_stderr=True)
        if raw:
            version = raw.strip().splitlines()[0]
    git_sha = run_capture(["git", "-C", REPO_ROOT, "rev-parse", "HEAD"])
    git_status = run_capture(["git", "-C", REPO_ROOT, "status", "--porcelain"])
    machine = _machine_identity()
    return {
        "git_sha": git_sha.strip() if git_sha else "unknown",
        "git_dirty": bool(git_status and git_status.strip()),
        "llama_server_version": version or "unknown",
        "llama_server_path": llama_server_path or "unknown",
        "model_path": model_path or "unknown",
        "model_sha256": sha,
        "model_sha256_scope": scope,
        "launch_args": list(launch_args or []),
        "machine_id": machine["machine_id"],
        "machine_chip": machine["machine_chip"],
        "machine_memory_bytes": machine["machine_memory_bytes"],
        "os_version": machine["os_version"],
    }


def _machine_identity():
    """The four machine fields of 1.4.9, per platform.

    macOS keeps the original sysctl sources unchanged. Windows uses CIM through
    powershell; the baseboard product is the identifier because
    Win32_ComputerSystem.Model on a self-built PC is a vendor placeholder
    ("System Product Name"). Linux reads DMI and /proc. Anything undeterminable
    stays "unknown" or 0, never invented.
    """
    if sys.platform == "darwin":
        memory = run_capture(["sysctl", "-n", "hw.memsize"])
        return {
            "machine_id": (run_capture(["sysctl", "-n", "hw.model"]) or "unknown").strip(),
            "machine_chip": (
                run_capture(["sysctl", "-n", "machdep.cpu.brand_string"]) or "unknown"
            ).strip(),
            "machine_memory_bytes": int(memory.strip()) if memory else 0,
            "os_version": (run_capture(["sw_vers", "-productVersion"]) or "unknown").strip(),
        }
    if sys.platform == "win32":
        def cim(query):
            out = run_capture(["powershell", "-NoProfile", "-Command", query])
            return out.strip() if out else ""
        memory = cim("(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory")
        return {
            "machine_id": cim("(Get-CimInstance Win32_BaseBoard).Product") or "unknown",
            "machine_chip": cim("(Get-CimInstance Win32_Processor).Name") or "unknown",
            "machine_memory_bytes": int(memory) if memory.isdigit() else 0,
            "os_version": cim("(Get-CimInstance Win32_OperatingSystem).Version") or "unknown",
        }
    # Linux and anything else: DMI board name, /proc/cpuinfo, sysconf.
    def read_first(path):
        try:
            with open(path, "r", encoding="utf-8", errors="replace") as handle:
                return handle.readline().strip()
        except OSError:
            return ""
    chip = ""
    try:
        with open("/proc/cpuinfo", "r", encoding="utf-8", errors="replace") as handle:
            for line in handle:
                if line.startswith("model name"):
                    chip = line.split(":", 1)[1].strip()
                    break
    except OSError:
        pass
    try:
        memory_bytes = os.sysconf("SC_PAGE_SIZE") * os.sysconf("SC_PHYS_PAGES")
    except (ValueError, OSError):
        memory_bytes = 0
    return {
        "machine_id": read_first("/sys/devices/virtual/dmi/id/board_name") or "unknown",
        "machine_chip": chip or "unknown",
        "machine_memory_bytes": memory_bytes,
        "os_version": platform.platform(terse=True) if platform else "unknown",
    }


def conditions(
    run_index=0,
    run_order="sequential",
    page_cache="unknown",
    server_restarted_before=False,
    slot_reset_before=False,
    sampling_seed=42,
    sampling_temperature=0.0,
    notes=None,
):
    return {
        "run_index": run_index,
        "run_order": run_order,
        "page_cache": page_cache,
        "server_restarted_before": server_restarted_before,
        "slot_reset_before": slot_reset_before,
        "sampling_seed": sampling_seed,
        "sampling_temperature": sampling_temperature,
        "notes": notes,
    }


def engine_block(n_ctx, n_ctx_slot_tok, **extra):
    block = {"n_ctx": n_ctx, "n_ctx_slot_tok": n_ctx_slot_tok}
    block.update(extra)
    return block


RUN_ORDERS = ("sequential", "randomised")
PAGE_CACHE_STATES = ("cold", "warm", "unknown")


def env_overlay(environ=None):
    """What an experiment runner tells a probe it launches as a subprocess.

    `sweep.py` runs the probes unmodified, because a comparison against the
    frozen baseline is only meaningful if the code that produced both sides is
    the same code. What it still has to communicate is the context the probe
    cannot observe from inside one run: where this run sits in a randomised
    order, whether the model file was already in the page cache, which launch
    flags produced the server it is talking to, and the model hash computed once
    for the whole campaign.

    Every value is validated rather than defaulted on malformed input. A run
    order silently falling back to `"sequential"` because of a typo would put a
    false statement in the conditions block of every record it touched, and I10
    would then pass on a campaign that never randomised anything.
    """
    env = os.environ if environ is None else environ

    # Unset means the probe was invoked directly and the default applies. Set to
    # an empty string means a caller computed a value and produced nothing, which
    # is a different situation and not one to paper over: an empty RUN_ORDER
    # silently becoming "sequential" would put a false statement in the
    # conditions block of every record it touched.
    order = env.get("RUN_ORDER")
    order = "sequential" if order is None else order
    if order not in RUN_ORDERS:
        raise ProbeError(
            "RUN_ORDER is %r, expected one of %s" % (order, ", ".join(RUN_ORDERS))
        )

    page_cache = env.get("PAGE_CACHE")
    page_cache = "unknown" if page_cache is None else page_cache
    if page_cache not in PAGE_CACHE_STATES:
        raise ProbeError(
            "PAGE_CACHE is %r, expected one of %s"
            % (page_cache, ", ".join(PAGE_CACHE_STATES))
        )

    raw_engine = env.get("ENGINE_EXTRA")
    raw_engine = "{}" if raw_engine is None else raw_engine
    try:
        engine_extra = json.loads(raw_engine)
    except json.JSONDecodeError as exc:
        raise ProbeError("ENGINE_EXTRA is not JSON: %s" % exc)
    if not isinstance(engine_extra, dict):
        raise ProbeError("ENGINE_EXTRA must be a JSON object, got %s" % type(engine_extra).__name__)

    return {
        "run_order": order,
        "page_cache": page_cache,
        "engine_extra": engine_extra,
        "model_sha256": env.get("MODEL_SHA256") or None,
    }


def now_rfc3339():
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


# ---------------------------------------------------------------------------
# Invariants, 1.5
# ---------------------------------------------------------------------------

PROVENANCE_REQUIRED = (
    "git_sha",
    "git_dirty",
    "llama_server_version",
    "llama_server_path",
    "model_path",
    "model_sha256",
    "model_sha256_scope",
    "launch_args",
    "machine_id",
    "machine_chip",
    "machine_memory_bytes",
    "os_version",
)


def check_invariants(record):
    """Every invariant that applies to this record, computed not assumed.

    Returns the sorted list of violated identifiers for the record's `invalid`
    array. An empty list means the record checked itself and passed, which is a
    different statement from nobody having looked.
    """
    violated = set()
    samples = record.get("samples") or []
    conds = record.get("conditions") or {}
    prov = record.get("provenance") or {}

    for sample in samples:
        total = sample.get("prompt_tok_total")
        computed = sample.get("prompt_tok_computed")
        cached = sample.get("prompt_tok_cached")
        if None not in (total, computed, cached) and total != computed + cached:
            violated.add("I1")
        if cached is not None and sample.get("cache_state") is not None:
            expected = "cold" if cached == 0 else "warm"
            if sample["cache_state"] != expected:
                violated.add("I2")
        state = sample.get("cache_state")
        if sample.get("ttft_cold_ms") is not None and state != "cold":
            violated.add("I3")
        if sample.get("ttft_warm_ms") is not None:
            if state != "warm" or sample.get("prompt_cache_hit_ratio") is None:
                violated.add("I3")
        if computed == 0 and sample.get("prefill_tps") is not None:
            violated.add("I4")
        # I5 is structural: decode_tps is computed over decode_ms in `measure`
        # and the wall-clock figure lives under its own name. The check guards a
        # future producer that conflates them.
        decode_tok = sample.get("decode_tok")
        decode_ms = sample.get("decode_ms")
        decode_tps = sample.get("decode_tps")
        if decode_tok and decode_ms and decode_tps is not None:
            expected_tps = decode_tok / (decode_ms / 1000.0)
            if abs(expected_tps - decode_tps) > max(1e-6, expected_tps * 1e-6):
                violated.add("I5")
        if not sample.get("streamed") and sample.get("ttft_ms") is not None:
            violated.add("I12")
        ttft = sample.get("ttft_ms")
        prefill_ms = sample.get("prefill_ms")
        if ttft is not None and prefill_ms is not None and ttft < prefill_ms:
            violated.add("I13")

    turn = record.get("turn")
    if turn:
        parts = (
            turn.get("engine_ms_total", 0.0)
            + turn.get("tool_ms_total", 0.0)
            + turn.get("orchestration_residual_ms", 0.0)
        )
        if abs(turn.get("turn_wall_ms", 0.0) - parts) > 1.0:
            violated.add("I6")
        if turn.get("orchestration_residual_ms", 0.0) < 0.0:
            violated.add("I6")

    if any(field not in prov for field in PROVENANCE_REQUIRED):
        violated.add("I7")

    stats = record.get("stats") or {}
    for block in stats.values():
        if block.get("n", 0) < 5:
            violated.add("I8")
            break
    if not stats and record.get("probe") in ("toolcall", "esrs", "batch"):
        # A scored probe reports one observation and no dispersion. That is one
        # repetition, which is below the floor I8 sets, so the record is
        # provisional. Absence of a stats block is not absence of the question.
        violated.add("I8")

    if conds.get("sampling_temperature") != 0.0 or conds.get("sampling_seed") == -1:
        violated.add("I9")

    return sorted(violated)


def check_campaign(records, comparing=False):
    """Campaign-level invariants, which no single record can see.

    I10 asks whether the campaign that produced these records compared
    configurations in a randomised order. That is a property of the set, not of
    a member of it, so `check_invariants` cannot reach it: a record carrying
    `run_order == "sequential"` is perfectly valid on its own and only becomes a
    violation once it is placed against a record from another configuration.

    `comparing` is the caller's statement that this is such a campaign. A
    baseline characterisation is not, and the two baseline campaigns in
    `results/` were run sequentially for that reason rather than in violation.

    Returns a list of `(record_id, invariant_id, detail)`, empty when the set
    checks out.
    """
    findings = []
    if not comparing:
        return findings
    for record in records or []:
        order = (record.get("conditions") or {}).get("run_order")
        if order != "randomised":
            findings.append(
                (
                    record.get("record_id") or "unknown",
                    "I10",
                    "run_order is %r on a record used in a configuration "
                    "comparison; thermal drift is indistinguishable from a "
                    "factor effect under a fixed order" % (order,),
                )
            )
    return findings


def build_record(
    probe,
    record_id,
    label,
    provenance_block,
    conditions_block,
    engine,
    campaign_id=None,
    samples=None,
    stats=None,
    extra=None,
):
    """A measurement record, with its `invalid` array computed before return."""
    record = {
        "schema_version": SCHEMA_VERSION,
        "record_id": record_id,
        "campaign_id": campaign_id,
        "probe": probe,
        "label": label,
        "measured_at": now_rfc3339(),
        "provenance": provenance_block,
        "conditions": conditions_block,
        "engine": engine,
    }
    if samples is not None:
        record["samples"] = samples
    if stats is not None:
        record["stats"] = stats
    if extra:
        record.update(extra)
    record["invalid"] = check_invariants(record)
    return record


# ---------------------------------------------------------------------------
# Containers, 1.9
# ---------------------------------------------------------------------------


def write_campaign(path, campaign_id, records, started_at, excluded=None):
    """The campaign container. `records_excluded` is never omitted."""
    container = {
        "schema_version": SCHEMA_VERSION,
        "campaign_id": campaign_id,
        "started_at": started_at,
        "finished_at": now_rfc3339(),
        "records": records,
        "records_excluded": list(excluded or []),
    }
    directory = os.path.dirname(os.path.abspath(path))
    if directory:
        os.makedirs(directory, exist_ok=True)
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(container, handle, ensure_ascii=False, indent=1)
    return container


def read_campaign(path):
    with open(path, "r", encoding="utf-8") as handle:
        text = handle.read().strip()
    if not text:
        return {"records": []}
    if text.startswith("["):
        return {"records": json.loads(text)}
    if text.lstrip().startswith("{") and '"records"' in text:
        return json.loads(text)
    records = []
    for line in text.splitlines():
        line = line.strip()
        if line:
            records.append(json.loads(line))
    return {"records": records}
