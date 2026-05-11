"""Code Review Director — orchestre 3 workers (security, style, perf) via A2A.

Pattern : L2 director (BaseReActAgent).
Les RÈGLES MÉTIER sont externalisées dans APOLLIA.md (lues par les workers via ctx.workspace).
Le director ne fait QUE de l'orchestration et de l'agrégation.
"""

from __future__ import annotations

import hashlib
import json
from datetime import datetime
from typing import Any

from apollia.agents.react import AIPResult, BaseReActAgent


class CodeReviewDirector(BaseReActAgent):
    SYSTEM_PROMPT = """Tu es le director d'une revue de code multi-aspect.

Workers disponibles via A2A :
- `a2a:review-security` : audit sécurité (input : {target_path, changed_lines?})
- `a2a:review-style` : audit style (input : {target_path})
- `a2a:review-performance` : audit performance (input : {target_path})

Stratégie :
1. Identifie les fichier(s) cibles depuis l'input utilisateur (chemin direct, ou diff git).
2. Pour chaque aspect choisi (par défaut : les trois), invoque le worker correspondant en parallèle conceptuel (un appel par worker).
3. Si un worker retourne `AIPResult.failed`, log l'erreur et continue avec les autres.
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

Ne fais PAS la review toi-même — délègue strictement.
"""
    MAX_STEPS = 25
    TEMPERATURE = 0.2

    def manifest(self) -> dict[str, Any]:
        return {
            "name": "code-review-director",
            "version": "1.0.0",
            "description": "Director orchestrant 3 workers de review (sécurité/style/perf)",
            "execution_mode": "direct",
            "agent_type": "user",
            "tools_required": ["file_read", "bash_executor"],
            "tools_optional": [
                "file_glob",
                "file_grep",
                "file_write",
                "a2a:review-security",
                "a2a:review-style",
                "a2a:review-performance",
            ],
            "memory_namespace": "code-review-multi",
            "supports_a2a": True,
            "max_concurrent_tasks": 1,
            "step_budget": {"max_steps": 25, "max_tool_calls": 20, "wall_clock_secs": 1500},
            "skills": [
                {
                    "id": "review-codebase",
                    "description": "Review multi-aspect d'un fichier ou d'un diff",
                    "input_modes": ["text"],
                    "output_modes": ["text"],
                    "input_schema": {"target": "str", "aspects": "list[str]?"},
                }
            ],
            "setup_notes": "Voir setup.md. CRUCIAL : remplir les sections APOLLIA.md.",
            "limitations": "Repo local uniquement. Sans APOLLIA.md, qualité dégradée.",
        }

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        if ctx.llm is None:
            return AIPResult.failed("NO_LLM", "Backend LLM requis")

        # Input
        parts = task.get("input", {}).get("parts", [])
        user_text = next((p.get("text", "") for p in parts if p.get("type") == "text"), "")
        if not user_text.strip():
            return AIPResult.failed("NO_INPUT", "Input vide. Format : 'Review src/foo.py' ou 'Review diff HEAD~1..HEAD'")

        # Cache check (par hash de l'input — utile pour réviser le même diff)
        cache_key = f"review:{self._input_hash(user_text)}"
        if ctx.memory is not None:
            try:
                cached = await ctx.memory.recall(cache_key)
                if cached:
                    ctx.log("info", "review cache hit")
                    return AIPResult.completed(cached)
            except Exception as e:
                ctx.log("debug", f"cache lookup failed: {e}")

        # Profil utilisateur (compliance peut influencer la sévérité security)
        user_context = await self._load_user_context(ctx)

        extra_context_parts = [
            "Sources de règles métier (lues par les workers via ctx.workspace) :",
            "- 'Code Review — Security Rules'",
            "- 'Code Review — Style Guide'",
            "- 'Code Review — Performance Budget'",
            "Si une section est absente d'APOLLIA.md, les workers utilisent un fallback minimal.",
        ]
        if user_context.get("user.tech.stack"):
            extra_context_parts.append(f"Stack utilisateur : {user_context['user.tech.stack']}")
        if user_context.get("user.constraints.compliance"):
            extra_context_parts.append(
                f"Contraintes compliance : {user_context['user.constraints.compliance']} "
                "(impact sur la sévérité security)."
            )

        try:
            result = await self.react(
                task,
                ctx,
                user_text,
                extra_context="\n".join(extra_context_parts),
            )
        except Exception as e:
            ctx.log("error", f"director react failed: {e}")
            return AIPResult.failed("DIRECTOR_FAILED", str(e))

        if isinstance(result, dict):
            return result

        await self._post_run(ctx, cache_key, result, task, user_text)
        return AIPResult.completed(result)

    @staticmethod
    def _input_hash(text: str) -> str:
        return hashlib.sha256(text.encode("utf-8")).hexdigest()[:16]

    async def _load_user_context(self, ctx: Any) -> dict[str, str]:
        # ADR-087 — read the user profile through ctx.profile.
        if ctx.profile is None:
            return {}
        out: dict[str, str] = {}
        try:
            all_entries = await ctx.profile.all()
        except Exception as e:
            ctx.log("debug", f"profile.all() failed: {e}")
            return out
        for key in ("tech.stack", "tech.languages", "constraints.compliance"):
            if all_entries.get(key):
                out[f"user.{key}"] = all_entries[key]
        return out

    async def _post_run(
        self,
        ctx: Any,
        cache_key: str,
        report: str,
        task: dict[str, Any],
        target: str,
    ) -> None:
        today = datetime.now().strftime("%Y-%m-%d")
        if ctx.memory is not None:
            try:
                await ctx.memory.remember(cache_key, report, source=task.get("task_id"))
                await ctx.memory.record(
                    content=f"Review terminée : {target[:120]}",
                    importance=0.6,
                    task_id=task.get("task_id"),
                    metadata={"date": today, "target": target},
                )
            except Exception as e:
                ctx.log("warn", f"memory persistence failed: {e}")

        if ctx.tools is not None:
            try:
                path = f"~/Documents/code-review-multi/{today}-{cache_key.split(':')[1]}.md"
                await ctx.tools.call("file_write", {"path": path, "content": report})
            except Exception as e:
                ctx.log("warn", f"file_write failed: {e}")

        if ctx.notify is not None:
            try:
                await ctx.notify.publish(
                    f"Review prête ({today})",
                    severity="info",
                    title="Code Review",
                )
            except Exception as e:
                ctx.log("debug", f"notify failed: {e}")


agent = CodeReviewDirector()


def manifest() -> dict[str, Any]:
    return agent.manifest()


async def run(task: dict[str, Any], ctx: Any) -> dict[str, Any]:
    return await agent.run(task, ctx)
