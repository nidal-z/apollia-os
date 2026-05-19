"""Code Review Director — orchestre 3 workers (security, style, perf) via A2A.

Pattern : L2 director (decorator-first SDK, ADR-098..ADR-112). Les règles
métier vivent dans APOLLIA.md (lues par les workers via ``ctx.workspace``).
Le director ne fait QUE de l'orchestration et de l'agrégation.
"""

from __future__ import annotations

import hashlib
from datetime import datetime
from typing import Any

from apollia import DomainError, agent, react, skill
from apollia.types import Ctx


SYSTEM_PROMPT = """Tu es le director d'une revue de code multi-aspect.

Workers disponibles via A2A :
- `review.security` : audit sécurité (input : {target_path, changed_lines?})
- `review.style` : audit style (input : {target_path})
- `review.performance` : audit performance (input : {target_path})

Stratégie :
1. Identifie les fichier(s) cibles depuis l'input utilisateur (chemin direct, ou diff git).
2. Pour chaque aspect choisi (par défaut : les trois), invoque le worker correspondant.
3. Si un worker retourne une erreur, log et continue avec les autres.
4. Agrège les findings en un rapport markdown structuré.

Format final attendu :
```
# Review — {target}

**Date :** YYYY-MM-DD

## 🔒 Sécurité
{output worker security}

## 🎨 Style
{output worker style}

## ⚡ Performance
{output worker perf}

## Synthèse
{N findings critiques, N total}
```

Ne fais PAS la review toi-même — délègue strictement aux workers.
"""


_DELEGATED_SKILLS = ("review.security", "review.style", "review.performance")


@agent(
    name="code-review-director",
    version="1.0.0",
    description="Director orchestrant 3 workers de review (sécurité/style/perf)",
    tags=("code-review", "director"),
    memory_namespace="code-review-multi",
    tools_required=("file_read", "bash_executor"),
    step_budget={"max_steps": 25, "max_tool_calls": 20, "wall_clock_secs": 1500},
)
class CodeReviewDirector:
    """Orchestration multi-aspect — délègue, n'inspecte pas le code lui-même."""

    @skill(
        "review.codebase",
        description="Review multi-aspect d'un fichier ou d'un diff",
    )
    async def review_codebase(
        self,
        target: str,
        aspects: list[str] | None = None,
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        """Review multi-aspect.

        Args:
            target: Chemin du fichier ou range git (ex. ``"HEAD~1..HEAD"``).
            aspects: Sous-ensemble des workers à invoquer ; ``None`` ⇒ tous.
        """
        if ctx.llm is None:
            raise DomainError("NO_LLM", "Backend LLM requis")
        if not target.strip():
            raise DomainError(
                "NO_INPUT",
                "Input vide. Format : 'src/foo.py' ou 'HEAD~1..HEAD'",
            )

        cache_key = f"review:{_input_hash(target)}"
        if ctx.memory is not None:
            try:
                cached = await ctx.memory.recall(cache_key)
                if cached:
                    ctx.logger.info("review cache hit", key=cache_key)
                    return {"report": cached, "cached": True}
            except Exception as exc:
                ctx.logger.debug("cache lookup failed", error=str(exc))

        # Assemble tool descriptors for delegated workers.
        tools: list[dict[str, Any]] = []
        wanted = tuple(aspects) if aspects else _DELEGATED_SKILLS
        for sid in wanted:
            if ctx.a2a is None:
                break
            try:
                tools.append(ctx.a2a.skill_as_tool(sid))
            except Exception as exc:
                ctx.logger.warning(
                    "skill_as_tool failed", skill_id=sid, error=str(exc)
                )

        user_context = await _load_user_context(ctx)
        extra_lines = [
            "Sources de règles métier (lues par les workers via ctx.workspace) :",
            "- 'Code Review — Security Rules'",
            "- 'Code Review — Style Guide'",
            "- 'Code Review — Performance Budget'",
        ]
        if user_context.get("user.tech.stack"):
            extra_lines.append(f"Stack utilisateur : {user_context['user.tech.stack']}")
        if user_context.get("user.constraints.compliance"):
            extra_lines.append(
                f"Contraintes compliance : {user_context['user.constraints.compliance']} "
                "(impact sur la sévérité security)."
            )

        try:
            report = await react(
                ctx,
                system=SYSTEM_PROMPT + "\n\n" + "\n".join(extra_lines),
                user=f"Review : {target}",
                tools=tools,
                max_steps=25,
                temperature=0.2,
            )
        except DomainError:
            raise
        except Exception as exc:
            ctx.logger.error("director react failed", error=str(exc))
            raise DomainError("DIRECTOR_FAILED", str(exc)) from exc

        await _post_run(ctx, cache_key, report, target)
        return {"report": report, "cached": False, "target": target}


# ─── Helpers ─────────────────────────────────────────────────────────────


def _input_hash(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()[:16]


async def _load_user_context(ctx: Ctx) -> dict[str, str]:
    if ctx.profile is None:
        return {}
    out: dict[str, str] = {}
    try:
        entries = await ctx.profile.all()
    except Exception as exc:
        ctx.logger.debug("profile.all() failed", error=str(exc))
        return out
    for key in ("tech.stack", "tech.languages", "constraints.compliance"):
        if entries.get(key):
            out[f"user.{key}"] = entries[key]
    return out


async def _post_run(ctx: Ctx, cache_key: str, report: str, target: str) -> None:
    today = datetime.now().strftime("%Y-%m-%d")
    if ctx.memory is not None:
        try:
            await ctx.memory.remember(cache_key, report)
            await ctx.memory.record(
                content=f"Review terminée : {target[:120]}",
                importance=0.6,
                metadata={"date": today, "target": target},
            )
        except Exception as exc:
            ctx.logger.warning("memory persistence failed", error=str(exc))

    if ctx.tools is not None:
        try:
            path = f"~/Documents/code-review-multi/{today}-{cache_key.split(':')[1]}.md"
            await ctx.tools.call("file_write", {"path": path, "content": report})
        except Exception as exc:
            ctx.logger.warning("file_write failed", error=str(exc))

    if ctx.notify is not None:
        try:
            await ctx.notify.publish(
                f"Review prête ({today})",
                severity="info",
                title="Code Review",
            )
        except Exception as exc:
            ctx.logger.debug("notify failed", error=str(exc))
