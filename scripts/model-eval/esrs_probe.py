#!/usr/bin/env python3
"""ESRS classification quality probe (the Yumni use case) against a llama-server.

Model-level signal: prompts the model directly with the closed ESRS code list and
each action, parses the predicted codes, and scores micro precision/recall/F1 over
(sample, code) pairs against the labeled dataset. This isolates model quality on the
task without the full director orchestration (that is the post-selection E2E step).
Arch-agnostic via the OpenAI /v1 API. Stdlib only.

Env: BASE_URL (default http://127.0.0.1:8080/v1), MODEL (default "local"),
     DATASET, CRITERIA (default to agents/yumni-classification/eval/...).
Prints one JSON object: precision/recall/F1 + per-sample hits.
"""

import json
import os
import re
import urllib.request

BASE_URL = os.environ.get("BASE_URL", "http://127.0.0.1:8080/v1").rstrip("/")
MODEL = os.environ.get("MODEL", "local")
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DATASET = os.environ.get("DATASET", os.path.join(ROOT, "agents/yumni-classification/eval/dataset.json"))
CRITERIA = os.environ.get("CRITERIA", os.path.join(ROOT, "agents/yumni-classification/eval/fixtures/criteria.sample.json"))


def load():
    ds = json.load(open(DATASET, encoding="utf-8"))["samples"]
    craw = json.load(open(CRITERIA, encoding="utf-8"))
    crit = craw["criteria"] if isinstance(craw, dict) and "criteria" in craw else craw
    return ds, crit


def chat(prompt):
    body = json.dumps(
        {
            "model": MODEL,
            "messages": [{"role": "user", "content": prompt}],
            "temperature": 0.0,
            "seed": 42,
            "max_tokens": 400,
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
        d = json.load(resp)
    return d.get("choices", [{}])[0].get("message", {}).get("content", "") or ""


def parse_codes(text, valid):
    # try JSON object first
    codes = []
    m = re.search(r"\{.*\}", text, re.DOTALL)
    if m:
        try:
            obj = json.loads(m.group(0))
            c = obj.get("codes") if isinstance(obj, dict) else None
            if isinstance(c, list):
                codes = [str(x).strip() for x in c]
        except json.JSONDecodeError:
            pass
    if not codes:
        # fallback: regex the code tokens (E1-3, S2-1, G1-1, ...)
        codes = re.findall(r"\b[EGS]\d-\d+\b", text)
    return [c for c in dict.fromkeys(codes) if c in valid]


def main():
    ds, crit = load()
    valid = {c["code"] for c in crit}
    catalog = "\n".join(f"- {c['code']}: {c['title']}" for c in crit)
    tp = fp = fn = 0
    per = []
    try:
        for s in ds:
            a = s["action"]
            prompt = (
                "Tu es un classifieur ESRS. Voici la liste FERMEE des criteres :\n"
                f"{catalog}\n\n"
                "Pour l'action suivante, choisis le ou les codes les plus pertinents "
                "STRICTEMENT dans cette liste. Reponds UNIQUEMENT par un JSON de la forme "
                '{\"codes\": [\"E1-3\"]}.\n\n'
                f"Action : {a['title']}\n{a['description']}"
            )
            pred = set(parse_codes(chat(prompt), valid))
            exp = set(s["expected"])
            tp += len(pred & exp)
            fp += len(pred - exp)
            fn += len(exp - pred)
            per.append({"id": s["id"], "pred": sorted(pred), "expected": sorted(exp)})
    except Exception as exc:  # noqa: BLE001
        print(json.dumps({"error": str(exc)}))
        return 1

    prec = tp / (tp + fp) if (tp + fp) else 0.0
    rec = tp / (tp + fn) if (tp + fn) else 0.0
    f1 = 2 * prec * rec / (prec + rec) if (prec + rec) else 0.0
    print(json.dumps({
        "precision": round(prec, 3), "recall": round(rec, 3), "f1": round(f1, 3),
        "tp": tp, "fp": fp, "fn": fn, "n_samples": len(ds), "per_sample": per,
    }, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
