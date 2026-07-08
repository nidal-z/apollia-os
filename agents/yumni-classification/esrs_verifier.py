"""ESRS verifier worker - adversarial check of a single candidate mapping.

Second opinion in the pipeline. Where the mapper is generous (propose broadly),
the verifier is skeptical: it re-reads ONE candidate mapping against ONE
criterion and decides whether the link is genuine or an over-claim /
hallucination.

Business rules (same spirit as the mapper):
  * Verification is a SUGGESTION of whether to keep, never a conformity verdict.
  * Reasoning stays DESCRIPTIVE ("contribue a ..." / "ne se rapporte pas a ...").
    It never asserts "conforme" / "non conforme".
  * The criterion code is fixed by the caller; the verifier cannot invent one.
  * Calls ``ctx.llm`` - no hardcoded model.
"""

from __future__ import annotations

import json
from typing import Any

from apollia import DomainError, agent, skill
from apollia.types import Ctx

SYSTEM_PROMPT = """\
Tu es un RELECTEUR CRITIQUE (adversarial) de classification RSE.

On te soumet :
  1. Une ENTITE Yumni (action RSE).
  2. Un CANDIDAT de mapping : {criterionCode, justification} produit par un premier modele.
  3. Le CRITERE ESRS vise : {code, title, description}.

Ta mission : verifier si l'action se rapporte REELLEMENT et de maniere PLAUSIBLE
au critere, ou s'il s'agit d'une sur-interpretation / d'un lien artificiel.

REGLES ABSOLUES :
  - Tu evalues la PERTINENCE du lien, tu ne prononces PAS de conformite.
  - Sois EXIGEANT : rejette (keep=false) si le lien est vague, generique, ou
    force. Garde (keep=true) seulement si la contribution est claire et defendable.
  - Ton "reason" est DESCRIPTIF : "l'action contribue a ..." / "l'action ne se
    rapporte pas a ... car ...". Jamais "conforme" / "non conforme".
  - "confidence" (0.0 a 1.0) = ta certitude dans ton propre verdict keep.

FORMAT DE SORTIE : STRICTEMENT un objet JSON, sans texte autour :
{"keep": <true|false>, "confidence": <0..1>, "reason": "<phrase descriptive>"}
"""


def _extract_json(raw: str) -> dict[str, Any]:
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


@agent(
    name="esrs-verifier",
    version="0.1.0",
    description=(
        "Worker that adversarially verifies a single candidate ESRS mapping "
        "against its criterion, guarding against over-claim and hallucination."
    ),
    tags=("rse", "esrs", "verification", "worker"),
    agent_type="worker",
    # Le director verifie desormais tous les candidats d'une classification EN
    # PARALLELE (asyncio.gather). Ce cap borne le nombre de verifications
    # simultanees : on l'aligne sur les slots du backend (llama-server -np 8) pour
    # que les appels concurrents batchent sans se re-serialiser au niveau du worker.
    max_concurrent_tasks=8,
)
class EsrsVerifier:
    """Adversarially verifies one candidate mapping against one criterion."""

    SYSTEM_PROMPT = SYSTEM_PROMPT

    @skill(
        "esrs.verify_mapping",
        description=(
            "Adversarially check whether a candidate ESRS mapping genuinely "
            "relates to the criterion (no over-claim, no hallucination). "
            "Returns a keep/confidence/reason suggestion, never a verdict."
        ),
        examples=[
            {
                "entity": {
                    "title": "Deploiement de bornes de recharge electrique",
                    "description": "Installation de 20 bornes sur les parkings du siege.",
                },
                "candidate": {
                    "criterionCode": "E1-3",
                    "justification": "Contribue a la reduction des emissions liees a la mobilite.",
                },
                "criterion": {
                    "code": "E1-3",
                    "title": "Actions et ressources climat",
                    "description": "Actions de reduction des emissions de GES.",
                },
            }
        ],
    )
    async def verify_mapping(
        self,
        entity: dict,
        candidate: dict,
        criterion: dict,
        ctx: Ctx = None,  # type: ignore[assignment]
    ) -> dict:
        """Verify ``candidate`` against ``criterion`` for ``entity``.

        Args:
            entity: The entity text ``{title, description, ...}``.
            candidate: ``{criterionCode, justification}`` from the mapper.
            criterion: ``{code, title, description}`` of the targeted criterion.

        Returns:
            ``{"keep": bool, "confidence": float, "reason": str}``.

        Raises:
            DomainError CANDIDATE_CRITERION_MISMATCH: candidate code != criterion code.
            DomainError LLM_OUTPUT_NOT_JSON: LLM did not return parseable JSON.
        """
        cand_code = str(candidate.get("criterionCode", "")).strip()
        crit_code = str(criterion.get("code", "")).strip()
        # Consistency guard: the two inputs must describe the same criterion.
        if cand_code and crit_code and cand_code != crit_code:
            raise DomainError(
                "CANDIDATE_CRITERION_MISMATCH",
                "candidate.criterionCode does not match criterion.code",
                details={"candidate": cand_code, "criterion": crit_code},
            )

        user_prompt = (
            "ENTITE (action Yumni) :\n"
            + json.dumps(entity, ensure_ascii=False, indent=2)
            + "\n\nCANDIDAT DE MAPPING :\n"
            + json.dumps(candidate, ensure_ascii=False, indent=2)
            + "\n\nCRITERE ESRS VISE :\n"
            + json.dumps(criterion, ensure_ascii=False, indent=2)
            + "\n\nRenvoie STRICTEMENT le JSON demande."
        )

        response = await ctx.llm.complete(
            messages=[
                {"role": "system", "content": self.SYSTEM_PROMPT},
                {"role": "user", "content": user_prompt},
            ],
            temperature=0.0,
        )

        parsed = _extract_json(response.content)
        keep = bool(parsed.get("keep", False))
        try:
            confidence = float(parsed.get("confidence", 0.0))
        except (TypeError, ValueError):
            confidence = 0.0
        confidence = max(0.0, min(1.0, confidence))
        reason = str(parsed.get("reason", "")).strip()

        return {"keep": keep, "confidence": confidence, "reason": reason}


agent = EsrsVerifier()  # type: ignore[assignment]
