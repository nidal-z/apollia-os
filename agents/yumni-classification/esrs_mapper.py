"""ESRS mapper worker - proposes ESRS criteria for a Yumni entity.

Business rules enforced here (prompt + code):
  * The AI does CLASSIFICATION / SUGGESTION, never a DECISION. The output is
    a list of *candidate* mappings with a confidence, not a verdict.
  * The justification is DESCRIPTIVE ("contribue a ...") and never
    AFFIRMATIVE ("conforme a ..."). Conformity is a human/audit judgement.
  * The output is CONSTRAINED to the closed list of real criteria codes
    passed in ``criteria``. Any code the LLM returns that is not in that
    set is treated as a hallucination and rejected hard (DomainError).

The agent never hardcodes a model: it calls ``ctx.llm`` and lets the
runtime router (see apollia.toml) pick the backend (sovereign-local first).
"""

from __future__ import annotations

import json
from typing import Any

from apollia import DomainError, agent, skill
from apollia.types import Ctx

# ---------------------------------------------------------------------------
# System prompt - encodes the business rules for the LLM.
# ---------------------------------------------------------------------------
SYSTEM_PROMPT = """\
Tu es un assistant de CLASSIFICATION RSE. Ton role est de SUGGERER, jamais de DECIDER.

On te donne :
  1. Une ENTITE Yumni (une "action" RSE : titre, description, contexte).
  2. Une LISTE FERMEE de criteres du referentiel ESRS, chacun avec {code, title, description}.

Ta tache : proposer les criteres ESRS auxquels l'action CONTRIBUE potentiellement.

REGLES ABSOLUES :
  - Tu ne PEUX utiliser QUE les codes presents dans la liste fournie. N'invente
    JAMAIS de code. Si aucun critere ne correspond, renvoie une liste vide.
  - Tu SUGGERES une contribution possible, tu ne PRONONCES PAS de conformite.
  - La justification est DESCRIPTIVE : formule "contribue a ...", "va dans le sens
    de ...", "soutient ...". Elle ne doit JAMAIS affirmer "conforme a ...",
    "respecte ...", "satisfait l'exigence ...".
  - La conformite reste une decision humaine / d'audit, hors de ton perimetre.
  - "confidence" est ta certitude que l'action se rapporte au critere (0.0 a 1.0).

FORMAT DE SORTIE : STRICTEMENT un objet JSON, sans texte autour :
{"mappings": [{"criterionCode": "<code de la liste>", "confidence": <0..1>, "justification": "<phrase descriptive>"}]}
"""

# System prompt for the reverse direction: one criterion -> which entities match.
SYSTEM_PROMPT_MATCH = """\
Tu es un assistant de CLASSIFICATION RSE. Ton role est de SUGGERER, jamais de DECIDER.

On te donne :
  1. Un CRITERE d'un referentiel (code, title, description).
  2. Une LISTE FERMEE d'ENTITES Yumni (actions RSE), chacune avec {id, title, description}.

Ta tache : identifier les entites qui CONTRIBUENT potentiellement a ce critere.

REGLES ABSOLUES :
  - Tu ne PEUX referencer QUE les "id" presents dans la liste fournie. N'invente JAMAIS
    d'id. Si aucune entite ne correspond, renvoie une liste vide.
  - Tu SUGGERES une contribution possible, tu ne PRONONCES PAS de conformite.
  - La justification est DESCRIPTIVE ("contribue a ...", "va dans le sens de ..."),
    jamais AFFIRMATIVE ("conforme a ...", "respecte ...").
  - "confidence" est ta certitude que l'entite se rapporte au critere (0.0 a 1.0).

FORMAT DE SORTIE : STRICTEMENT un objet JSON, sans texte autour :
{"matches": [{"entityId": "<id de la liste>", "confidence": <0..1>, "justification": "<phrase descriptive>"}]}
"""


def _extract_json(raw: str) -> dict[str, Any]:
    """Best-effort extraction of the first JSON object from an LLM answer.

    Small local models often wrap JSON in prose or ```json fences. We locate
    the outermost ``{...}`` span and parse it. Any failure is surfaced as a
    DomainError so the director can branch on a stable code.
    """
    text = raw.strip()
    start = text.find("{")
    end = text.rfind("}")
    if start == -1 or end == -1 or end < start:
        raise DomainError(
            "LLM_OUTPUT_NOT_JSON",
            "LLM response did not contain a JSON object",
            details={"raw": raw[:500]},
        )
    try:
        parsed = json.loads(text[start : end + 1])
    except json.JSONDecodeError as exc:
        raise DomainError(
            "LLM_OUTPUT_NOT_JSON",
            f"LLM response was not valid JSON: {exc}",
            details={"raw": raw[:500]},
        ) from exc
    if not isinstance(parsed, dict):
        raise DomainError(
            "LLM_OUTPUT_NOT_JSON",
            "LLM JSON root was not an object",
            details={"raw": raw[:500]},
        )
    return parsed


def _fmt_items(items: list[dict], key: str) -> str:
    """Compact, cache-friendly listing of criteria or entities.

    Plain text (``- <id> | <title> : <description>``) instead of pretty JSON:
    same information, far fewer tokens (no braces, quotes or indentation), which
    shrinks the prompt and its prefill cost.
    """
    lines: list[str] = []
    for it in items:
        if not isinstance(it, dict):
            continue
        ident = str(it.get(key, "")).strip()
        title = str(it.get("title", "")).strip()
        desc = str(it.get("description", "")).strip()
        lines.append(f"- {ident} | {title} : {desc}" if desc else f"- {ident} | {title}")
    return "\n".join(lines)


@agent(
    name="esrs-mapper",
    version="0.1.0",
    description=(
        "Worker that proposes candidate ESRS criteria for a Yumni action, "
        "constrained to a closed list of real criteria codes (anti-hallucination)."
    ),
    tags=("rse", "esrs", "classification", "worker"),
    agent_type="worker",
    # Doit être >= au cap du director : plusieurs classifications parallèles
    # appellent ce worker en même temps (A2A).
    max_concurrent_tasks=3,
)
class EsrsMapper:
    """Proposes ESRS criteria mappings from a closed criteria list."""

    SYSTEM_PROMPT = SYSTEM_PROMPT

    @skill(
        "esrs.propose_mappings",
        description=(
            "Propose candidate ESRS criteria to which a Yumni entity (action) "
            "contributes. Output is constrained to the provided closed criteria "
            "list; unknown codes are rejected. Suggestion only, never a decision."
        ),
        examples=[
            {
                "entity": {
                    "title": "Deploiement de bornes de recharge electrique",
                    "description": "Installation de 20 bornes sur les parkings du siege.",
                    "context": "Plan de mobilite durable 2026.",
                },
                "criteria": [
                    {
                        "code": "E1-1",
                        "title": "Plan de transition climatique",
                        "description": "Attenuation du changement climatique.",
                    },
                    {
                        "code": "E1-3",
                        "title": "Actions et ressources climat",
                        "description": "Actions de reduction des emissions de GES.",
                    },
                ],
            }
        ],
    )
    async def propose_mappings(
        self,
        entity: dict,
        criteria: list[dict],
        ctx: Ctx = None,  # type: ignore[assignment]
    ) -> dict:
        """Propose ESRS mappings for ``entity`` restricted to ``criteria``.

        Args:
            entity: The entity text ``{title, description, context}``.
            criteria: Closed list ``[{code, title, description}]``.

        Returns:
            ``{"mappings": [{"criterionCode", "confidence", "justification"}]}``.

        Raises:
            DomainError EMPTY_CRITERIA: no criteria provided (nothing to map to).
            DomainError UNKNOWN_CRITERION: LLM returned a code outside the list.
            DomainError LLM_OUTPUT_NOT_JSON / LLM_OUTPUT_MALFORMED: bad LLM output.
        """
        allowed_codes = {
            str(c["code"]) for c in criteria if isinstance(c, dict) and c.get("code")
        }
        if not allowed_codes:
            raise DomainError(
                "EMPTY_CRITERIA",
                "No criteria provided; cannot propose mappings against an empty list.",
            )

        # Criteria block FIRST (stable across every action of a batch): it becomes
        # the cacheable prompt prefix on the backend, so classifying the next action
        # re-prefills only the trailing entity instead of the whole criteria list.
        # Entity LAST (the only part that varies per action). Compact encoding on
        # top, to cut the prompt size (and thus the prefill time) with no info loss.
        user_prompt = (
            "LISTE FERMEE DE CRITERES ESRS (utilise UNIQUEMENT ces codes) :\n"
            + _fmt_items(criteria, "code")
            + "\n\nENTITE (action Yumni) a classer :\n"
            + json.dumps(entity, ensure_ascii=False)
            + "\n\nRenvoie STRICTEMENT le JSON demande."
        )

        # No hardcoded model: the runtime router picks the backend.
        ctx.logger.info("propose_mappings: calling LLM", entity_title=entity.get("title"))
        response = await ctx.llm.complete(
            messages=[
                {"role": "system", "content": self.SYSTEM_PROMPT},
                {"role": "user", "content": user_prompt},
            ],
            temperature=0.1,
        )
        ctx.logger.info("propose_mappings: LLM response received", content_len=len(response.content))

        parsed = _extract_json(response.content)
        ctx.logger.info("propose_mappings: JSON parsed", mappings_count=len(parsed.get("mappings", [])))
        raw_mappings = parsed.get("mappings", [])
        if not isinstance(raw_mappings, list):
            raise DomainError(
                "LLM_OUTPUT_MALFORMED",
                "'mappings' must be a list",
                details={"got": type(raw_mappings).__name__},
            )

        mappings: list[dict[str, Any]] = []
        for item in raw_mappings:
            if not isinstance(item, dict):
                continue
            code = str(item.get("criterionCode", "")).strip()
            # ANTI-HALLUCINATION: hard-reject any code outside the closed list.
            if code not in allowed_codes:
                raise DomainError(
                    "UNKNOWN_CRITERION",
                    f"LLM proposed a criterion code absent from the closed list: {code!r}",
                    details={"proposed": code, "allowed": sorted(allowed_codes)},
                )
            try:
                confidence = float(item.get("confidence", 0.0))
            except (TypeError, ValueError):
                confidence = 0.0
            confidence = max(0.0, min(1.0, confidence))
            justification = str(item.get("justification", "")).strip()
            mappings.append(
                {
                    "criterionCode": code,
                    "confidence": confidence,
                    "justification": justification,
                }
            )

        return {"mappings": mappings}

    @skill(
        "esrs.match_entities",
        description=(
            "Reverse direction: given ONE referential criterion and a closed list "
            "of Yumni entities, identify which entities contribute to it. Output is "
            "constrained to the provided entity ids; unknown ids are rejected. "
            "Suggestion only, never a decision."
        ),
        examples=[
            {
                "criterion": {
                    "code": "E1-6",
                    "title": "Emissions de GES (Scope 1, 2, 3)",
                    "description": "Emissions de gaz a effet de serre.",
                },
                "entities": [
                    {
                        "id": "a1",
                        "title": "Politique de deplacements (visio, train < 4h)",
                        "description": "Reduction des deplacements physiques.",
                    }
                ],
            }
        ],
    )
    async def match_entities(
        self,
        criterion: dict,
        entities: list[dict],
        ctx: Ctx = None,  # type: ignore[assignment]
    ) -> dict:
        """Identify which ``entities`` contribute to a single ``criterion``.

        Returns ``{"matches": [{"entityId", "confidence", "justification"}]}``,
        constrained to the provided entity ids (anti-hallucination).
        """
        allowed_ids = {
            str(e["id"]) for e in entities if isinstance(e, dict) and e.get("id")
        }
        if not allowed_ids:
            raise DomainError(
                "EMPTY_ENTITIES",
                "No entities provided; cannot match against an empty list.",
            )

        # Criterion FIRST (stable), entities LAST (compact). Same cache/size logic
        # as propose_mappings, applied to the reverse direction.
        user_prompt = (
            "CRITERE :\n"
            + json.dumps(criterion, ensure_ascii=False)
            + "\n\nLISTE FERMEE D'ENTITES (utilise UNIQUEMENT ces id) :\n"
            + _fmt_items(entities, "id")
            + "\n\nRenvoie STRICTEMENT le JSON demande."
        )

        ctx.logger.info("match_entities: calling LLM", criterion_code=criterion.get("code"))
        response = await ctx.llm.complete(
            messages=[
                {"role": "system", "content": SYSTEM_PROMPT_MATCH},
                {"role": "user", "content": user_prompt},
            ],
            temperature=0.1,
        )
        ctx.logger.info("match_entities: LLM response received", content_len=len(response.content))

        parsed = _extract_json(response.content)
        ctx.logger.info("match_entities: JSON parsed", matches_count=len(parsed.get("matches", [])))
        raw = parsed.get("matches", [])
        if not isinstance(raw, list):
            raise DomainError(
                "LLM_OUTPUT_MALFORMED",
                "'matches' must be a list",
                details={"got": type(raw).__name__},
            )

        matches: list[dict[str, Any]] = []
        for item in raw:
            if not isinstance(item, dict):
                continue
            entity_id = str(item.get("entityId", "")).strip()
            if entity_id not in allowed_ids:
                raise DomainError(
                    "UNKNOWN_ENTITY",
                    f"LLM referenced an entity id absent from the closed list: {entity_id!r}",
                    details={"proposed": entity_id, "allowed": sorted(allowed_ids)},
                )
            try:
                confidence = float(item.get("confidence", 0.0))
            except (TypeError, ValueError):
                confidence = 0.0
            confidence = max(0.0, min(1.0, confidence))
            matches.append(
                {
                    "entityId": entity_id,
                    "confidence": confidence,
                    "justification": str(item.get("justification", "")).strip(),
                }
            )

        return {"matches": matches}


# Expose the singleton for the runtime bridge / inspect.
agent = EsrsMapper()  # type: ignore[assignment]
