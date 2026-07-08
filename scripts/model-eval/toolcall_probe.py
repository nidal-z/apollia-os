#!/usr/bin/env python3
"""Agentic tool-calling probe against a llama-server (OpenAI-compatible /v1).

Top selection criterion for Apollia (agentic). Sends a battery of tool-use tasks
and scores whether the model emits well-formed tool calls: correct tool selected,
valid JSON arguments with the right values, correct abstention when no tool applies,
parallel calls, and multi-turn use of a returned tool result. Requires the server
started with --jinja so the model's chat template drives tool formatting. Arch-
agnostic (any model llama-server loads). Stdlib only.

Env: BASE_URL (default http://127.0.0.1:8080/v1), MODEL (default "local").
Prints one JSON object: per-task scores + aggregate.
"""

import json
import os
import urllib.request

BASE_URL = os.environ.get("BASE_URL", "http://127.0.0.1:8080/v1").rstrip("/")
MODEL = os.environ.get("MODEL", "local")

WEATHER = {
    "type": "function",
    "function": {
        "name": "get_weather",
        "description": "Meteo actuelle ou prevue pour une ville.",
        "parameters": {
            "type": "object",
            "properties": {
                "location": {"type": "string", "description": "Ville"},
                "unit": {"type": "string", "enum": ["celsius", "fahrenheit"]},
            },
            "required": ["location"],
        },
    },
}
EMAIL = {
    "type": "function",
    "function": {
        "name": "send_email",
        "description": "Envoie un email.",
        "parameters": {
            "type": "object",
            "properties": {
                "to": {"type": "string"},
                "subject": {"type": "string"},
                "body": {"type": "string"},
            },
            "required": ["to", "subject", "body"],
        },
    },
}
TASK = {
    "type": "function",
    "function": {
        "name": "create_task",
        "description": "Cree une tache.",
        "parameters": {
            "type": "object",
            "properties": {
                "title": {"type": "string"},
                "priority": {"type": "string", "enum": ["low", "medium", "high"]},
            },
            "required": ["title", "priority"],
        },
    },
}
GET_TIME = {
    "type": "function",
    "function": {
        "name": "get_time",
        "description": "Heure locale d'un fuseau.",
        "parameters": {
            "type": "object",
            "properties": {"timezone": {"type": "string"}},
            "required": ["timezone"],
        },
    },
}


def chat(messages, tools):
    body = json.dumps(
        {
            "model": MODEL,
            "messages": messages,
            "tools": tools,
            "tool_choice": "auto",
            "temperature": 0.0,
            "seed": 42,
            "max_tokens": 512,
            "stream": False,
            "chat_template_kwargs": {"enable_thinking": False},
        }
    ).encode()
    req = urllib.request.Request(
        BASE_URL + "/chat/completions",
        data=body,
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=600) as resp:
        return json.load(resp)


def tool_calls(resp):
    msg = resp.get("choices", [{}])[0].get("message", {})
    out = []
    for tc in msg.get("tool_calls") or []:
        fn = tc.get("function", {})
        args_raw = fn.get("arguments", "")
        try:
            args = json.loads(args_raw) if isinstance(args_raw, str) else (args_raw or {})
            valid = True
        except (json.JSONDecodeError, TypeError):
            args, valid = {}, False
        out.append({"name": fn.get("name", ""), "args": args, "valid_json": valid})
    return out, (msg.get("content") or "")


def has_kv(args, key, expected):
    v = args.get(key)
    if v is None:
        return False
    return expected.lower() in str(v).lower()


def score_single(calls, content, tool, need):
    """0.5 correct tool + valid json, +0.5 all expected args match."""
    hit = next((c for c in calls if c["name"] == tool), None)
    if not hit or not hit["valid_json"]:
        return 0.0
    args_ok = all(has_kv(hit["args"], k, v) for k, v in need.items())
    return 0.5 + (0.5 if args_ok else 0.0)


def run():
    results = {}

    # 1. single tool + arg
    r = chat([{"role": "user", "content": "Quel temps fait-il a Lyon aujourd'hui ?"}], [WEATHER])
    c, txt = tool_calls(r)
    results["weather_single"] = score_single(c, txt, "get_weather", {"location": "Lyon"})

    # 2. multi-arg
    r = chat(
        [{"role": "user", "content": "Envoie un email a bob@exemple.fr, objet 'Point projet', message 'On se voit lundi.'"}],
        [EMAIL],
    )
    c, txt = tool_calls(r)
    results["email_multiarg"] = score_single(c, txt, "send_email", {"to": "bob@exemple.fr", "subject": "Point projet"})

    # 3. enum-constrained arg
    r = chat([{"role": "user", "content": "Cree une tache 'Preparer la demo' en priorite haute."}], [TASK])
    c, txt = tool_calls(r)
    results["enum_priority"] = score_single(c, txt, "create_task", {"title": "demo", "priority": "high"})

    # 4. abstention (no relevant tool)
    r = chat(
        [{"role": "user", "content": "Bonjour, qui a ecrit Les Miserables ?"}],
        [WEATHER, EMAIL],
    )
    c, txt = tool_calls(r)
    results["abstain_relevance"] = 1.0 if not c else 0.0

    # 5. parallel calls (two cities)
    r = chat([{"role": "user", "content": "Compare la meteo entre Paris et Marseille."}], [WEATHER])
    c, txt = tool_calls(r)
    w = [x for x in c if x["name"] == "get_weather" and x["valid_json"]]
    got_paris = any(has_kv(x["args"], "location", "Paris") for x in w)
    got_marseille = any(has_kv(x["args"], "location", "Marseille") for x in w)
    results["parallel_weather"] = (0.5 if w else 0.0) + (0.5 if (got_paris and got_marseille) else 0.0)

    # 6. multi-turn: call get_time, then USE the returned result (no re-call)
    m = [{"role": "user", "content": "Quelle heure est-il a Tokyo ?"}]
    r = chat(m, [GET_TIME])
    c, txt = tool_calls(r)
    hit = next((x for x in c if x["name"] == "get_time"), None)
    turn1 = 0.5 if (hit and hit["valid_json"] and has_kv(hit["args"], "timezone", "tok")) else 0.0
    turn2 = 0.0
    if hit:
        raw = r["choices"][0]["message"]
        # echo the assistant tool-call turn, then the tool result
        m.append({"role": "assistant", "content": raw.get("content") or "", "tool_calls": raw.get("tool_calls")})
        m.append({"role": "tool", "tool_call_id": (raw.get("tool_calls") or [{}])[0].get("id", "call_0"),
                  "name": "get_time", "content": json.dumps({"time": "18:42", "timezone": "Asia/Tokyo"})})
        r2 = chat(m, [GET_TIME])
        c2, txt2 = tool_calls(r2)
        turn2 = 0.5 if (not c2 and "18:42" in txt2) else 0.0
    results["multiturn_use_result"] = turn1 + turn2

    total = sum(results.values())
    n = len(results)
    return {
        "tasks": {k: round(v, 2) for k, v in results.items()},
        "score": round(total / n, 3),
        "passed_full": sum(1 for v in results.values() if v >= 0.99),
        "n_tasks": n,
    }


def main():
    try:
        out = run()
    except Exception as exc:  # noqa: BLE001
        print(json.dumps({"error": str(exc)}))
        return 1
    print(json.dumps(out, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
