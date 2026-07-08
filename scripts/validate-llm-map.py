#!/usr/bin/env python3
"""Validation E2E du gain de `ctx.llm.map` contre un llama-server.

`ctx.llm.map` n'est pas appelable seul (methode injectee dans le contexte agent).
Ce script reproduit EXACTEMENT ce que `map` fait en interne, contre le meme
backend (un llama-server OpenAI-compatible) que le router d'Apollia utiliserait :

- un prefixe systeme byte-identique pour chaque item (maximise le cache de
  prefixe cote serveur) ;
- N items completes en concurrence bornee (ce que fait le semaphore de `map`),
  compares a une boucle SEQUENTIELLE (ce qu'un integrateur naif ferait).

Il chiffre le speedup que `map` delivre : sur un llama-server lance en continuous
batching, le gain vient du batching + du cache de prefixe partage.

Prerequis :
  llama-server -m <model.gguf> -ngl 999 -c 16384 -np 8 -cb --flash-attn on \\
               --host 127.0.0.1 --port 8080

Usage :
  python3 scripts/validate-llm-map.py
  BASE_URL=http://127.0.0.1:8080/v1 N=32 CONCURRENCY=8 MAX_TOKENS=128 \\
      python3 scripts/validate-llm-map.py

Sortie : wall-clock et tok/s agreges pour les deux modes, plus le speedup.
Stdlib uniquement (urllib + concurrent.futures), aucune dependance.
"""

import concurrent.futures
import json
import os
import sys
import time
import urllib.error
import urllib.request

BASE_URL = os.environ.get("BASE_URL", "http://127.0.0.1:8080/v1").rstrip("/")
URL = BASE_URL + "/chat/completions"
N = int(os.environ.get("N", "32"))
CONCURRENCY = int(os.environ.get("CONCURRENCY", "8"))
MAX_TOKENS = int(os.environ.get("MAX_TOKENS", "128"))
TIMEOUT_S = int(os.environ.get("TIMEOUT_S", "600"))

# Prefixe systeme PARTAGE, identique pour chaque item : c'est ce que `map`
# construit (system(prefix) + user(item)) pour declencher le cache de prefixe.
PREFIX = (
    "Tu es un classifieur ESRS. Pour chaque paragraphe fourni, reponds par le "
    "code du datapoint ESRS le plus pertinent suivi d'une phrase de "
    "justification. Sois concis et deterministe."
)


def item_text(i: int) -> str:
    """Partie variable (le `item` de map), differente d'un appel a l'autre."""
    scope = 1 + (i % 3)
    return (
        f"Paragraphe {i} : au cours de l'exercice, l'entreprise a reduit ses "
        f"emissions de gaz a effet de serre de scope {scope} de {5 + i} pour "
        f"cent, grace a des investissements dans l'efficacite energetique de "
        f"ses sites de production."
    )


def complete_one(item: str) -> int:
    """Une completion (system(prefix) + user(item)). Retourne les tokens generes."""
    body = json.dumps(
        {
            "messages": [
                {"role": "system", "content": PREFIX},
                {"role": "user", "content": item},
            ],
            "max_tokens": MAX_TOKENS,
            "temperature": 0.0,
            "stream": False,
        }
    ).encode()
    req = urllib.request.Request(
        URL, data=body, headers={"Content-Type": "application/json"}
    )
    with urllib.request.urlopen(req, timeout=TIMEOUT_S) as resp:
        data = json.load(resp)
    return int(data.get("usage", {}).get("completion_tokens", 0))


def run_sequential(items: list[str]) -> tuple[float, int]:
    """Baseline : un item a la fois (boucle naive)."""
    start = time.time()
    total = sum(complete_one(it) for it in items)
    return time.time() - start, total


def run_map_style(items: list[str], concurrency: int) -> tuple[float, int]:
    """Mode `map` : concurrence bornee, prefixe partage, ordre preserve."""
    start = time.time()
    total = 0
    with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as pool:
        # ex.map preserve l'ordre des items, comme le contrat de ctx.llm.map.
        for tokens in pool.map(complete_one, items):
            total += tokens
    return time.time() - start, total


def tps(tokens: int, seconds: float) -> float:
    return tokens / seconds if seconds > 0 else 0.0


def check_reachable() -> bool:
    try:
        with urllib.request.urlopen(BASE_URL.rsplit("/v1", 1)[0] + "/health", timeout=10):
            return True
    except (urllib.error.URLError, OSError):
        # /health absent sur certains builds : on tentera quand meme /v1.
        return True


def main() -> int:
    print(
        f"validate-llm-map  endpoint={BASE_URL}  N={N}  "
        f"concurrency={CONCURRENCY}  max_tokens={MAX_TOKENS}"
    )
    if not check_reachable():
        print(f"  [ERREUR] serveur injoignable sur {BASE_URL}", file=sys.stderr)
        return 1

    items = [item_text(i) for i in range(N)]

    try:
        # Warmup : amorce le cache de prefixe partage avant la mesure.
        complete_one(items[0])

        seq_wall, seq_tok = run_sequential(items)
        print(
            f"  [sequentiel] wall={seq_wall * 1000:.0f} ms  "
            f"tok={seq_tok}  agg_tps={tps(seq_tok, seq_wall):.1f}"
        )

        map_wall, map_tok = run_map_style(items, CONCURRENCY)
        print(
            f"  [map       ] wall={map_wall * 1000:.0f} ms  "
            f"tok={map_tok}  agg_tps={tps(map_tok, map_wall):.1f}"
        )
    except (urllib.error.URLError, OSError) as exc:
        print(f"  [ERREUR] appel au serveur echoue : {exc}", file=sys.stderr)
        return 1

    speedup = seq_wall / map_wall if map_wall > 0 else 0.0
    print(f"  => speedup (map vs sequentiel) = {speedup:.2f}x")
    print(
        "  (le gain reflete le continuous batching + le cache de prefixe du "
        "backend ; c'est ce que ctx.llm.map exploite sans que l'integrateur "
        "gere la concurrence.)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
