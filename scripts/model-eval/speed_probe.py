#!/usr/bin/env python3
"""Single-stream speed probe against a llama-server (OpenAI-compatible /v1).

Measures TTFT (time to first token) and decode throughput (tok/s) on a short
generation, plus a sanity signal (non-empty, non-degenerate output). Arch-agnostic:
works for any model llama-server can load, so it is the uniform speed measure across
the whole shortlist. Stdlib only.

Env:
  BASE_URL   (default http://127.0.0.1:8080/v1)
  MODEL      (default "local")  -> sent as the "model" field
  MAX_TOKENS (default 200)
  PROMPT     (optional override)

Prints one JSON object on stdout.
"""

import json
import os
import sys
import time
import urllib.request

BASE_URL = os.environ.get("BASE_URL", "http://127.0.0.1:8080/v1").rstrip("/")
MODEL = os.environ.get("MODEL", "local")
MAX_TOKENS = int(os.environ.get("MAX_TOKENS", "200"))
PROMPT = os.environ.get(
    "PROMPT",
    "Explique en trois phrases ce qu'est un cache KV dans un LLM, de facon claire.",
)


def stream_once() -> dict:
    body = json.dumps(
        {
            "model": MODEL,
            "messages": [{"role": "user", "content": PROMPT}],
            "max_tokens": MAX_TOKENS,
            "temperature": 0.0,
            "seed": 42,
            "stream": True,
            # Reasoning off: measure instruct-mode speed comparably across models
            # (thinking models otherwise spend the budget on reasoning_content).
            "chat_template_kwargs": {"enable_thinking": False},
        }
    ).encode()
    req = urllib.request.Request(
        BASE_URL + "/chat/completions",
        data=body,
        headers={"Content-Type": "application/json"},
    )
    start = time.perf_counter()
    ttft = None
    tokens = 0
    text_parts = []
    with urllib.request.urlopen(req, timeout=600) as resp:
        for raw in resp:
            line = raw.decode("utf-8", "replace").strip()
            if not line.startswith("data:"):
                continue
            payload = line[len("data:") :].strip()
            if payload == "[DONE]":
                break
            try:
                obj = json.loads(payload)
            except json.JSONDecodeError:
                continue
            delta = obj.get("choices", [{}])[0].get("delta", {})
            piece = delta.get("content") or delta.get("reasoning_content") or ""
            if piece:
                if ttft is None:
                    ttft = time.perf_counter() - start
                tokens += 1
                text_parts.append(piece)
    total = time.perf_counter() - start
    text = "".join(text_parts)
    decode_s = max(total - (ttft or 0.0), 1e-6)
    # crude degeneracy signal: fraction of a repeated 12-char window
    degenerate = False
    if len(text) > 60:
        w = text[20:32]
        degenerate = text.count(w) > 5
    return {
        "ttft_ms": round((ttft or 0.0) * 1000, 1),
        "decode_tok": tokens,
        "decode_tps": round(tokens / decode_s, 1),
        "total_ms": round(total * 1000, 1),
        "chars": len(text),
        "empty": len(text.strip()) == 0,
        "degenerate": degenerate,
        "sample": text[:120].replace("\n", " "),
    }


def main() -> int:
    try:
        # warmup (primes the server / model), then measure
        stream_once()
        r = stream_once()
    except Exception as exc:  # noqa: BLE001
        print(json.dumps({"error": str(exc)}))
        return 1
    print(json.dumps(r, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
