"""Security Worker — audit sécurité selon APOLLIA.md `Code Review — Security Rules`."""

from __future__ import annotations

from typing import Any

from apollia import DomainError, agent, react, skill
from apollia.types import Ctx
from apollia.utils.parsing import extract_json


FALLBACK_SECURITY_RULES = """
Règles de sécurité minimales (fallback) :
- Pas de credentials en dur (mots de passe, tokens, clés API).
- Validation des inputs externes (HTTP, fichiers, DB).
- Pas de SQL/shell injection (paramétrage des requêtes, échappement des args).
- Logs : pas de PII, pas de tokens.
- Crypto : pas de MD5/SHA1 pour des usages sécurité.
"""


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


@agent(
    name="security-worker",
    version="0.1.0",
    description="Audit sécurité d'un fichier selon règles workspace",
    tags=("code-review", "security", "worker"),
    agent_type="worker",
    memory_namespace="code-review-multi",
    tools_required=("file_read", "file_grep"),
    step_budget={"max_steps": 8, "max_tool_calls": 15, "wall_clock_secs": 240},
)
class SecurityWorker:
    """Audit sécurité."""

    @skill("review.security", description="Audit sécurité")
    async def review_security(
        self,
        target_path: str,
        changed_lines: list[int] | None = None,
        ctx: Ctx = None,
    ) -> dict[str, Any]:
        if ctx.llm is None:
            raise DomainError("NO_LLM", "Backend LLM requis")
        if not target_path.strip():
            raise DomainError(
                "MISSING_TARGET", "target_path requis dans le payload"
            )

        rules = FALLBACK_SECURITY_RULES
        rules_source = "fallback"
        if ctx.workspace is not None:
            try:
                custom = ctx.workspace.get("Code Review — Security Rules")
                if custom:
                    rules = custom
                    rules_source = "APOLLIA.md"
            except Exception as exc:
                ctx.logger.debug("workspace.get failed", error=str(exc))

        ctx.logger.info("security review", target=target_path, source=rules_source)
        user_message = (
            f"target_path: {target_path}\nchanged_lines: {changed_lines or 'all'}"
        )
        try:
            result = await react(
                ctx,
                system=SYSTEM_PROMPT
                + f"\n\n<security_rules source='{rules_source}'>\n{rules}\n</security_rules>",
                user=user_message,
                max_steps=8,
                temperature=0.1,
            )
        except DomainError:
            raise
        except Exception as exc:
            raise DomainError("EXECUTION_FAILED", str(exc)) from exc

        parsed = extract_json(result)
        if parsed is None:
            raise DomainError(
                "PARSE_ERROR", "Output worker non parseable en JSON"
            )
        return parsed
