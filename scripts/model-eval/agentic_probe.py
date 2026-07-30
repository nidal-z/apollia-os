#!/usr/bin/env python3
"""Replay a fixed multi-turn scenario against the Apollia runtime.

The only probe here that measures what a user experiences. Every other probe
talks to `llama-server` directly and therefore measures the engine; this one
drives the runtime, so its numbers include prompt assembly, tool dispatch,
persistence and scheduling. The difference between the two is
`orchestration_residual_ms`, the term this project can act on without touching
the model.

It does not decompose anything itself. The runtime writes one turn record per
completed turn to `APOLLIA_PERF_TRACE`, already in the contract's shape, and
this probe drives the turns and reads that file. Re-deriving the decomposition
client-side would produce a second answer to a question the runtime has already
answered authoritatively, which is the drift the contract forbids.

A record arriving with a non-empty `invalid` is surfaced, never aggregated over.
A turn that violated I6 has a residual whose additive model did not hold, and
averaging it into a mean would convert a broken measurement into a plausible
number.

Env:
  APOLLIA_URL         (default http://127.0.0.1:7771)
  APOLLIA_TOKEN       (default: read from $APOLLIA_HOME/api-token or ~/.apollia)
  APOLLIA_PERF_TRACE  (required) trace file the runtime is appending to
  SCENARIO            (optional) path to a JSON array of user messages
  TOOLS               (default file_list,file_read)
  MODE                (default libre)
  TURN_TIMEOUT_S      (default 300)
  CAMPAIGN_ID, OUT

Prints one JSON campaign container on stdout. Stdlib only.
"""

import json
import os
import sys
import threading
import time
import urllib.error
import urllib.request

import harness

APOLLIA_URL = os.environ.get("APOLLIA_URL", "http://127.0.0.1:7771").rstrip("/")
PERF_TRACE = os.environ.get("APOLLIA_PERF_TRACE", "")
SCENARIO_PATH = os.environ.get("SCENARIO") or None
TOOLS = [t for t in os.environ.get("TOOLS", "file_list,file_read").split(",") if t]
MODE = os.environ.get("MODE", "libre")
TURN_TIMEOUT_S = float(os.environ.get("TURN_TIMEOUT_S", "300"))
CAMPAIGN_ID = os.environ.get("CAMPAIGN_ID") or None
OUT = os.environ.get("OUT") or None

# Fixed, so two campaigns are comparable. One turn with no tool, one that has to
# call a tool, one that has to use the previous result: the three shapes a ReAct
# loop actually produces.
DEFAULT_SCENARIO = [
    "Reponds en une phrase: qu'est-ce qu'un cache KV?",
    "Liste les fichiers du repertoire courant.",
    "Combien de fichiers as-tu trouve? Reponds par un nombre.",
]


def api_token():
    explicit = os.environ.get("APOLLIA_TOKEN")
    if explicit:
        return explicit
    home = os.environ.get("APOLLIA_HOME") or os.path.expanduser("~/.apollia")
    path = os.path.join(home, "api-token")
    if os.path.isfile(path):
        with open(path, "r", encoding="utf-8") as handle:
            return handle.read().strip()
    return None


def auth_headers():
    token = api_token()
    return {"Authorization": "Bearer %s" % token} if token else {}


class ApprovalResponder(threading.Thread):
    """Answer tool-approval prompts so a scripted turn can finish.

    The runtime's assisted tier blocks every tool call on a human decision. A
    replay has no human, so without this the turn stalls until the approval
    times out and the measurement is of a timeout, not of a turn.

    Answering immediately is not cheating the measurement: the wait it removes
    is recorded separately as `tool_approval_ms`, which 1.4.7 keeps out of the
    residual precisely because a turn that waited on a person is not a slow
    turn. What remains in `tool_ms` is the tool actually running.
    """

    daemon = True

    def __init__(self, session_id):
        super().__init__()
        self.session_id = session_id
        self.approved = 0
        self._stop = threading.Event()

    def stop(self):
        self._stop.set()

    def run(self):
        url = "%s/api/v1/sessions/%s/stream" % (APOLLIA_URL, self.session_id)
        try:
            request = urllib.request.Request(url, headers=auth_headers())
            with urllib.request.urlopen(request, timeout=None) as stream:
                for raw in stream:
                    if self._stop.is_set():
                        return
                    line = raw.decode("utf-8", "replace").strip()
                    if not line.startswith("data:"):
                        continue
                    # The payload names its own event, and the SSE `event:`
                    # header arrives after its data line rather than before, so
                    # keying off the header order drops every approval.
                    try:
                        payload = json.loads(line[len("data:") :].strip())
                    except json.JSONDecodeError:
                        continue
                    if payload.get("event") == "approval_required":
                        self._authorize(payload)
        except (urllib.error.URLError, OSError, ValueError):
            # The stream dying must not take the campaign with it. A turn that
            # then stalls fails loudly on its own timeout.
            return

    def _authorize(self, payload):
        try:
            harness.post_json(
                "%s/api/v1/sessions/%s/authorize" % (APOLLIA_URL, self.session_id),
                {
                    "message_id": payload.get("message_id"),
                    "tool_call_id": payload.get("tool_call_id"),
                    "tool_name": payload.get("tool_name"),
                    "decision": "accept",
                },
                headers=auth_headers(),
                timeout=60,
            )
            self.approved += 1
        except (urllib.error.URLError, OSError, ValueError):
            return


def read_trace(path):
    """Turn records written so far. A truncated last line is ignored."""
    if not path or not os.path.isfile(path):
        return []
    records = []
    with open(path, "r", encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            try:
                records.append(json.loads(line))
            except json.JSONDecodeError:
                continue
    return records


def run_turn(session_id, content, seen_before):
    """Send one message and wait for the runtime to append its turn record.

    The completion signal is the trace file growing, not an HTTP status: the
    record is written when the turn closes, so waiting on it is waiting on the
    exact event being measured.
    """
    harness.post_json(
        "%s/api/v1/sessions/%s/messages" % (APOLLIA_URL, session_id),
        {"content": content},
        headers=auth_headers(),
        timeout=TURN_TIMEOUT_S,
    )
    deadline = time.monotonic() + TURN_TIMEOUT_S
    while time.monotonic() < deadline:
        records = read_trace(PERF_TRACE)
        fresh = [r for r in records if r.get("record_id") not in seen_before]
        if fresh:
            return fresh
        time.sleep(0.25)
    raise harness.ProbeError(
        "no turn record appeared in %s within %.0f s for message %r. Is the "
        "runtime running with APOLLIA_PERF_TRACE set to that path?"
        % (PERF_TRACE, TURN_TIMEOUT_S, content[:60])
    )


def main():
    if not PERF_TRACE:
        raise harness.ProbeError(
            "APOLLIA_PERF_TRACE is unset. This probe reads the runtime's own "
            "turn records and does not re-derive them, so without that opt-in "
            "there is nothing to read."
        )
    started = harness.now_rfc3339()
    scenario = DEFAULT_SCENARIO
    if SCENARIO_PATH:
        with open(SCENARIO_PATH, "r", encoding="utf-8") as handle:
            scenario = json.load(handle)

    session = harness.post_json(
        "%s/api/v1/sessions" % APOLLIA_URL,
        {"mode": MODE, "agent_name": None, "system_prompt": None, "tools": TOOLS},
        headers=auth_headers(),
        timeout=120,
    )
    session_id = session.get("session_id") or session.get("id")
    if not session_id:
        raise harness.ProbeError("session creation returned no id: %r" % session)

    responder = ApprovalResponder(session_id)
    responder.start()

    seen = {r.get("record_id") for r in read_trace(PERF_TRACE)}
    records = []
    excluded = []
    for index, message in enumerate(scenario):
        for record in run_turn(session_id, message, seen):
            seen.add(record.get("record_id"))
            record["campaign_id"] = CAMPAIGN_ID
            conds = record.setdefault("conditions", {})
            conds["run_index"] = index
            # I8 as well: one turn is one sample. The runtime writes what it
            # measured; marking it provisional is the consumer's job, and the
            # invariant array already says so.
            if record.get("invalid"):
                excluded.append(
                    {
                        "record_id": record.get("record_id"),
                        "reason": "violates %s" % ",".join(record["invalid"]),
                    }
                )
            records.append(record)

    responder.stop()
    kept = [r for r in records if not r.get("invalid")]
    residuals = [
        r["turn"]["orchestration_residual_ratio"]
        for r in kept
        if r.get("turn", {}).get("orchestration_residual_ratio") is not None
    ]
    summary = (
        "%d turn(s) recorded, %d flagged invalid and excluded from aggregates, "
        "%d tool approval(s) answered"
        % (len(records), len(excluded), responder.approved)
    )
    if residuals:
        block = harness.aggregate(residuals)
        summary += "; orchestration residual median %.1f percent of turn wall-clock" % (
            100.0 * block["median"]
        )
    sys.stderr.write(summary + "\n")
    for item in excluded:
        sys.stderr.write("  excluded %s: %s\n" % (item["record_id"], item["reason"]))

    container = {
        "schema_version": harness.SCHEMA_VERSION,
        "campaign_id": CAMPAIGN_ID or "agentic",
        "started_at": started,
        "finished_at": harness.now_rfc3339(),
        "records": records,
        "records_excluded": excluded,
        "agentic_summary": summary,
    }
    if OUT:
        harness.write_campaign(
            OUT, container["campaign_id"], records, started, excluded
        )
    return container


if __name__ == "__main__":
    try:
        sys.stdout.write(json.dumps(main(), ensure_ascii=False) + "\n")
    except (harness.ProbeError, OSError, ValueError, urllib.error.URLError) as exc:
        sys.stderr.write("agentic_probe: %s\n" % exc)
        raise SystemExit(1)
