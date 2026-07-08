"""Agent sonde E2E : valide ctx.llm.map de bout en bout et le chronometre.

Installe puis invoque via le daemon (voir scripts/e2e/run-map-e2e.sh), cet agent
exerce le VRAI chemin : dispatch -> ctx.llm.map -> LlmRouter -> backend (runner
embarque + modele par defaut, ou llama-server si configure). Il verifie l'ordre
et le compte de succes, et compare map a une boucle sequentielle equivalente.
"""

import time

from apollia import agent, on_message
from apollia.types import Ctx, Message

PREFIX = "Reponds en UN seul mot, en MAJUSCULES : le contraire du mot donne."
WORDS = ["grand", "chaud", "rapide", "clair", "haut", "plein", "ouvert", "fort"]
MAX_TOKENS = 8


@agent(name="map-e2e", version="0.1.0", description="Sonde E2E de ctx.llm.map")
class MapE2E:
    """Agent conversationnel minimal : le message est le nombre d'items (defaut 8)."""

    @on_message
    async def chat(self, message: str, history: list[Message], ctx: Ctx) -> str:
        try:
            n = max(1, min(32, int(message.strip())))
        except ValueError:
            n = 8
        items = [WORDS[i % len(WORDS)] for i in range(n)]

        # --- map (Apollia gere concurrence + prefixe partage) ---
        t0 = time.perf_counter()
        results = await ctx.llm.map(
            prefix=PREFIX, items=items, max_tokens=MAX_TOKENS, temperature=0.0
        )
        map_ms = (time.perf_counter() - t0) * 1000.0

        # --- sequentiel equivalent (un complete par item) ---
        t1 = time.perf_counter()
        for item in items:
            await ctx.llm.complete(
                messages=[
                    {"role": "system", "content": PREFIX},
                    {"role": "user", "content": item},
                ],
                max_tokens=MAX_TOKENS,
                temperature=0.0,
            )
        seq_ms = (time.perf_counter() - t1) * 1000.0

        oks = sum(1 for r in results if r.get("ok"))
        ordered = all(r.get("index") == i for i, r in enumerate(results))
        speedup = seq_ms / map_ms if map_ms > 0 else 0.0
        sample = (results[0].get("text", "").strip() if results else "")[:40]

        return (
            f"E2E ctx.llm.map : N={n} ok={oks}/{n} ordered={ordered} "
            f"map={map_ms:.0f}ms seq={seq_ms:.0f}ms speedup={speedup:.2f}x "
            f"sample={sample!r}"
        )
