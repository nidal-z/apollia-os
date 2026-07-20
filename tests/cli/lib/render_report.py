#!/usr/bin/env python3
"""Assemble report.json (machine) and report.md (human) from the TSV buffers.

Kept dependency-free (stdlib only) to honour the zero-external-dependency
principle. Rows are one assertion each; captures carry the non-deterministic
Track 3 payloads (input/output/streaming) for human review.
"""

import argparse
import json
from pathlib import Path

CAP_EXCERPT = 1200  # characters of captured output embedded in the Markdown


def read_rows(path):
    rows = []
    p = Path(path)
    if not p.exists():
        return rows
    for line in p.read_text(encoding="utf-8", errors="replace").splitlines():
        if not line:
            continue
        parts = line.split("\t")
        while len(parts) < 5:
            parts.append("")
        track, label, verdict, exit_code, dur = parts[:5]
        rows.append(
            {
                "track": track,
                "label": label,
                "verdict": verdict,
                "exit": _int(exit_code),
                "duration_ms": _int(dur),
            }
        )
    return rows


def read_captures(path):
    caps = []
    p = Path(path)
    if not p.exists():
        return caps
    for line in p.read_text(encoding="utf-8", errors="replace").splitlines():
        if not line:
            continue
        parts = line.split("\t")
        while len(parts) < 7:
            parts.append("")
        label, exit_code, dur, chunks, first_ms, inp, cap_file = parts[:7]
        output = ""
        if cap_file and Path(cap_file).exists():
            output = Path(cap_file).read_text(encoding="utf-8", errors="replace")
        caps.append(
            {
                "label": label,
                "exit": _int(exit_code),
                "duration_ms": _int(dur),
                "stream_chunks": _int(chunks),
                "first_chunk_ms": _int(first_ms),
                "input": inp,
                "output": output,
            }
        )
    return caps


def _int(s):
    try:
        return int(s)
    except (TypeError, ValueError):
        return 0


def render_md(rows, caps, totals):
    out = []
    out.append("# Apollia OS CLI E2E report\n")
    out.append(
        f"- PASS **{totals['pass']}**, FAIL **{totals['fail']}**, "
        f"SKIP **{totals['skip']}**, wall {totals['wall']}s\n"
    )

    # Per-track summary.
    by_track = {}
    for r in rows:
        t = by_track.setdefault(r["track"], {"PASS": 0, "FAIL": 0, "SKIP": 0})
        if r["verdict"] in t:
            t[r["verdict"]] += 1
    out.append("\n## Coverage by track\n")
    out.append("| Track | PASS | FAIL | SKIP |")
    out.append("|---|---|---|---|")
    for track in sorted(by_track):
        t = by_track[track]
        out.append(f"| {track} | {t['PASS']} | {t['FAIL']} | {t['SKIP']} |")

    # Failures first: what a human needs to see.
    fails = [r for r in rows if r["verdict"] == "FAIL"]
    if fails:
        out.append("\n## Failures\n")
        for r in fails:
            out.append(f"- `{r['label']}` (track {r['track']}, exit {r['exit']})")

    # Justified skips.
    skips = [r for r in rows if r["verdict"] == "SKIP"]
    if skips:
        out.append("\n## Justified skips\n")
        for r in skips:
            out.append(f"- `{r['label']}`")

    # The point of Track 3: surface inputs/outputs for human precision.
    if caps:
        out.append("\n## Non-deterministic captures (human review)\n")
        out.append(
            "The content below is NOT asserted. Only structural properties "
            "(exit code, streaming happened, terminated in time) are checked. "
            "Read these to judge the actual answer quality.\n"
        )
        for c in caps:
            out.append(f"\n### {c['label']}\n")
            out.append(
                f"- exit **{c['exit']}**, {c['stream_chunks']} stream chunk(s), "
                f"first chunk {c['first_chunk_ms']} ms, total "
                f"{c['duration_ms']} ms"
            )
            if c["input"]:
                out.append(f"\n**Input:** {c['input']}\n")
            excerpt = c["output"][:CAP_EXCERPT]
            if len(c["output"]) > CAP_EXCERPT:
                excerpt += "\n... (truncated)"
            out.append("\n**Output:**\n")
            out.append("```")
            out.append(excerpt if excerpt.strip() else "(empty)")
            out.append("```")

    return "\n".join(out) + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--rows", required=True)
    ap.add_argument("--captures", required=True)
    ap.add_argument("--out-json", required=True)
    ap.add_argument("--out-md", required=True)
    ap.add_argument("--pass", dest="npass", type=int, default=0)
    ap.add_argument("--fail", dest="nfail", type=int, default=0)
    ap.add_argument("--skip", dest="nskip", type=int, default=0)
    ap.add_argument("--wall", dest="wall", default="0")
    args = ap.parse_args()

    rows = read_rows(args.rows)
    caps = read_captures(args.captures)
    totals = {
        "pass": args.npass,
        "fail": args.nfail,
        "skip": args.nskip,
        "wall": args.wall,
    }

    payload = {
        "summary": totals,
        "assertions": rows,
        "captures": caps,
    }
    Path(args.out_json).write_text(
        json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8"
    )
    Path(args.out_md).write_text(render_md(rows, caps, totals), encoding="utf-8")


if __name__ == "__main__":
    main()
