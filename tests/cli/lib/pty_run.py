#!/usr/bin/env python3
"""Drive an interactive REPL under a pseudo-terminal and capture its output.

The `apollia-os chat` REPL uses rustyline, which requires a real tty, so it
cannot be driven by a plain pipe. This helper allocates a pty, launches the
command, types the given prompt, waits for the streamed answer to settle (idle
heuristic, since the content is non-deterministic), then sends EOF to exit.

Like run_capture.py, it asserts nothing about the content: it records exit code,
time-to-first-output, stdout burst count (the streaming signal) and the full
transcript for human review.

Emits a one-line JSON summary on stdout:
  {"exit","duration_ms","first_chunk_ms","chunks","bytes","timed_out"}

Stdlib only.
"""

import argparse
import errno
import json
import os
import pty
import selectors
import subprocess
import time


def drive(argv, prompt, timeout, idle, out_file):
    master, slave = pty.openpty()
    proc = subprocess.Popen(
        argv,
        stdin=slave,
        stdout=slave,
        stderr=slave,
        close_fds=True,
        start_new_session=True,
    )
    os.close(slave)

    sel = selectors.DefaultSelector()
    sel.register(master, selectors.EVENT_READ)

    t0 = time.monotonic()
    buf = []
    first_chunk_ms = 0
    chunks = 0
    timed_out = False
    last_data = time.monotonic()
    prompt_sent = False
    exit_sent = False

    # Give the REPL a moment to draw its banner/prompt before typing.
    time.sleep(0.6)

    while True:
        now = time.monotonic()
        if now - t0 > timeout:
            timed_out = True
            break
        if not prompt_sent:
            os.write(master, (prompt + "\r").encode("utf-8"))
            prompt_sent = True
            last_data = now

        events = sel.select(timeout=0.25)
        got = False
        for key, _ in events:
            try:
                data = os.read(key.fileobj, 4096)
            except OSError as exc:
                if exc.errno == errno.EIO:  # pty closed on child exit
                    data = b""
                else:
                    raise
            if not data:
                # Child closed the pty: we are done.
                proc.wait()
                buf.append(data)
                _finish(sel, master)
                return _summary(
                    proc, t0, buf, first_chunk_ms, chunks, timed_out, out_file
                )
            got = True
            if first_chunk_ms == 0:
                first_chunk_ms = int((now - t0) * 1000)
            chunks += 1
            buf.append(data)
            last_data = now

        if proc.poll() is not None:
            break

        # After the answer streamed and the line went idle, send EOF to exit.
        if prompt_sent and not exit_sent and not got and (now - last_data) > idle:
            try:
                os.write(master, b"\x04")  # Ctrl-D
            except OSError:
                pass
            exit_sent = True
            last_data = now
        # If we already asked to exit and it stays idle, stop waiting.
        if exit_sent and not got and (now - last_data) > idle:
            break

    try:
        proc.terminate()
        proc.wait(timeout=2)
    except (subprocess.TimeoutExpired, ProcessLookupError):
        try:
            proc.kill()
        except ProcessLookupError:
            pass
    _finish(sel, master)
    return _summary(proc, t0, buf, first_chunk_ms, chunks, timed_out, out_file)


def _finish(sel, master):
    try:
        sel.unregister(master)
    except (KeyError, ValueError):
        pass
    try:
        os.close(master)
    except OSError:
        pass


def _summary(proc, t0, buf, first_chunk_ms, chunks, timed_out, out_file):
    duration_ms = int((time.monotonic() - t0) * 1000)
    text = b"".join(buf).decode("utf-8", errors="replace")
    with open(out_file, "w", encoding="utf-8") as fh:
        fh.write(text)
    return {
        "exit": proc.returncode if proc.returncode is not None else -1,
        "duration_ms": duration_ms,
        "first_chunk_ms": first_chunk_ms,
        "chunks": chunks,
        "bytes": len(text),
        "timed_out": timed_out,
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--timeout", type=float, default=120.0)
    ap.add_argument("--idle", type=float, default=6.0)
    ap.add_argument("--prompt", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("cmd", nargs=argparse.REMAINDER)
    args = ap.parse_args()
    argv = args.cmd
    if argv and argv[0] == "--":
        argv = argv[1:]
    if not argv:
        print(json.dumps({"exit": -1, "error": "no command"}))
        return
    print(json.dumps(drive(argv, args.prompt, args.timeout, args.idle, args.out)))


if __name__ == "__main__":
    main()
