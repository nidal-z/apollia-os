"""Security Worker — audit sécurité d'un fichier selon les règles APOLLIA.md.

Pattern : L2 worker (WorkerAgent).
Règles métier lues via ctx.workspace.get("Code Review — Security Rules").
Si la section est absente, fallback minimal explicite.
"""

from __future__ import annotations

import json
from typing import Any

from apollia.agents.react import AIPResult
from apollia.agents.worker import WorkerAgent
from apollia.utils.parsing import extract_json


FALLBACK_SECURITY_RULES = """
Règles de sécurité minimales (fallback — APOLLIA.md `Code Review — Security Rules` absente) :
- Pas de credentials en dur (mots de passe, tokens, clés API).
- Validation des inputs externes (HTTP, fichiers, DB).
- Pas de SQL/shell injection (paramétrage des requêtes, échappement des args).
- Logs : pas de PII, pas de tokens.
- Crypto : pas de MD5/SHA1 pour des usages sécurité.
"""


class SecurityWorker(WorkerAgent):
    SYSTEM_PROMPT = """Tu es un expert sécurité applicative.

Tu reçois :
- Un chemin de fichier à auditer.
- (Optionnel) Une liste de lignes modifiées à scruter en priorité.
- Les règles de sécurité du projet (depuis APOLLIA.md ou fallback).

Étapes obligatoires :
1. Lis le fichier via `file_read`.
2. Si lignes modifiées fournies, concentre-toi dessus en priorité.
3. Compare contre chaque règle.
4. Pour chaque finding : sévérité (critical/high/medium/low), ligne, description, recommandation.

Format de sortie OBLIGATOIRE (JSON brut, pas de markdown wrapper) :
{
  "findings": [
    {"severity": "high", "line": 42, "rule": "no-hardcoded-secret", "description": "...", "recommendation": "..."}
  ],
  "summary": "N findings critiques, M total"
}

Si aucun finding : `{"findings": [], "summary": "Aucun problème de sécurité détecté"}`.
"""
    MAX_STEPS = 8
    TEMPERATURE = 0.1

    def manifest(self) -> dict[str, Any]:
        return {
            "name": "security-worker",
            "version": "1.0.0",
            "description": "Audit sécurité d'un fichier selon règles workspace",
            "execution_mode": "direct",
            "agent_type": "user",
            "tools_required": ["file_read", "file_grep"],
            "tools_optional": ["file_glob"],
            "memory_namespace": "code-review-multi",
            "supports_a2a": True,
            "max_concurrent_tasks": 2,
            "step_budget": {"max_steps": 8, "max_tool_calls": 15, "wall_clock_secs": 240},
            "skills": [
                {
                    "id": "review-security",
                    "description": "Audit sécurité",
                    "input_modes": ["text"],
                    "output_modes": ["text"],
                    "input_schema": {"target_path": "str", "changed_lines": "list[int]?"},
                }
            ],
        }

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        if ctx.llm is None:
            return AIPResult.failed("NO_LLM", "Backend LLM requis")

        # Extract payload
        parts = task.get("input", {}).get("parts", [])
        raw = next((p.get("text", "") for p in parts if p.get("type") == "text"), "{}")
        try:
            payload = json.loads(raw) if raw.strip().startswith("{") else {"target_path": raw.strip()}
        except json.JSONDecodeError:
            payload = {"target_path": raw.strip()}

        target_path = payload.get("target_path", "").strip()
        if not target_path:
            return self.domain_error("missing_target", "target_path requis dans le payload")

        # Lecture règles métier depuis APOLLIA.md (CRITIQUE pour qualité)
        rules = FALLBACK_SECURITY_RULES
        rules_source = "fallback"
        if ctx.workspace is not None:
            try:
                custom = ctx.workspace.get("Code Review — Security Rules")
                if custom:
                    rules = custom
                    rules_source = "APOLLIA.md"
            except Exception as e:
                ctx.log("debug", f"workspace.get failed: {e}")

        ctx.log("info", f"security review {target_path} (rules: {rules_source})")

        user_message = json.dumps(
            {
                "target_path": target_path,
                "changed_lines": payload.get("changed_lines"),
            },
            ensure_ascii=False,
        )
        extra_context = f"<security_rules source='{rules_source}'>\n{rules}\n</security_rules>"

        try:
            result = await self.react(task, ctx, user_message, extra_context=extra_context)
        except Exception as e:
            return self.domain_error("execution_failed", str(e))

        if isinstance(result, dict):
            return result

        parsed = extract_json(result)
        if parsed is None:
            return self.domain_error("parse_error", "Output worker non parseable en JSON")

        return AIPResult.completed(json.dumps(parsed, ensure_ascii=False))


agent = SecurityWorker()


def manifest() -> dict[str, Any]:
    return agent.manifest()


async def run(task: dict[str, Any], ctx: Any) -> dict[str, Any]:
    return await agent.run(task, ctx)
