#!/usr/bin/env python3
"""Run a command, capture its I/O, and measure streaming characteristics.

Used by Track 3 (non-deterministic / LLM-backed commands). We never assert the
content: this tool records exit code, wall time, time-to-first-chunk and the
number of distinct stdout bursts (the structural signal that a response was
streamed rather than delivered in one buffered write), and dumps the full output
to a file for human review via the Markdown report.

Emits a one-line JSON summary on stdout:
  {"exit","duration_ms","first_chunk_ms","chunks","bytes","timed_out"}

Stdlib only (zero external dependency principle).
"""

import argparse
import json
import selectors
import subprocess
import time


def run(argv, timeout, out_file):
    t0 = time.monotonic()
    proc = subprocess.Popen(
        argv,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        bufsize=0,
    )
    sel = selectors.DefaultSelector()
    sel.register(proc.stdout, selectors.EVENT_READ, "out")
    sel.register(proc.stderr, selectors.EVENT_READ, "err")

    out_chunks = []
    err_chunks = []
    first_chunk_ms = 0
    chunks = 0
    timed_out = False
    open_streams = 2

    while open_streams > 0:
        remaining = timeout - (time.monotonic() - t0)
        if remaining <= 0:
            timed_out = True
            proc.kill()
            break
        events = sel.select(timeout=min(remaining, 0.5))
        for key, _ in events:
            data = key.fileobj.read(4096)
            if not data:
                sel.unregister(key.fileobj)
                open_streams -= 1
                continue
            if key.data == "out":
                if first_chunk_ms == 0:
                    first_chunk_ms = int((time.monotonic() - t0) * 1000)
                chunks += 1
                out_chunks.append(data)
            else:
                err_chunks.append(data)

    try:
        proc.wait(timeout=2)
    except subprocess.TimeoutExpired:
        proc.kill()
        timed_out = True

    duration_ms = int((time.monotonic() - t0) * 1000)
    stdout = b"".join(out_chunks).decode("utf-8", errors="replace")
    stderr = b"".join(err_chunks).decode("utf-8", errors="replace")

    body = stdout
    if stderr.strip():
        body += "\n--- stderr ---\n" + stderr
    with open(out_file, "w", encoding="utf-8") as fh:
        fh.write(body)

    return {
        "exit": proc.returncode if proc.returncode is not None else -1,
        "duration_ms": duration_ms,
        "first_chunk_ms": first_chunk_ms,
        "chunks": chunks,
        "bytes": len(stdout),
        "timed_out": timed_out,
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--timeout", type=float, default=120.0)
    ap.add_argument("--out", required=True)
    ap.add_argument("cmd", nargs=argparse.REMAINDER)
    args = ap.parse_args()
    argv = args.cmd
    if argv and argv[0] == "--":
        argv = argv[1:]
    if not argv:
        print(json.dumps({"exit": -1, "error": "no command"}))
        return
    print(json.dumps(run(argv, args.timeout, args.out)))


if __name__ == "__main__":
    main()
