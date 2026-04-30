"""Email Triage — agent orchestré ORIA.

Pattern : L3 (OrchestratedAgent).
ORIA (Reasoner) génère le plan, ActorLoop l'exécute step-by-step.
L'agent ne fait QUE :
- déclarer le manifest (system_prompt riche, tools_requiring_approval)
- post-traitement via on_plan_complete

⚠️ Apollia v0.1 ne fournit pas d'outils gmail natifs. Cet agent assume des wrappers
http_fetch vers Gmail API (ou équivalent Microsoft Graph). Voir setup.md.
"""

from __future__ import annotations

from typing import Any

from apollia.agents.orchestrated import OrchestratedAgent
from apollia.agents.react import AIPResult


SYSTEM_PROMPT = """Tu es un agent de triage d'inbox. Mode orchestré ORIA.

WORKFLOW (à respecter dans le plan que tu génères) :

1. **Récupérer les emails non lus** depuis Gmail API.
   - Outil : http_fetch GET https://gmail.googleapis.com/gmail/v1/users/me/messages?q=is:unread
   - Authentification : Bearer token depuis ctx.memory.recall("gmail.access_token")
   - Si token expiré : refresh via gmail.refresh_token + client_id/secret stockés en mémoire

2. **Pour chaque email**, faire :
   a. Récupérer le contenu complet (from, subject, body)
   b. Classer selon les règles workspace :
      - urgent : action requise sous 24h (mots : "ASAP", "urgent", "deadline aujourd'hui")
      - important : à traiter cette semaine (clients, projets actifs)
      - newsletter : à archiver après lecture rapide
      - spam : à supprimer
      - automatique : notif système sans action (à archiver)
   c. Décider d'une action :
      - reply_draft : préparer une réponse (sans envoyer — HITL bloquera l'envoi)
      - label : appliquer un label
      - archive : archiver
      - skip : laisser tel quel
      - escalate : signaler à l'utilisateur

3. **Toute action http_fetch nécessite HITL** (manifest tools_requiring_approval).
   Le runtime suspendra automatiquement avant l'appel et demandera l'approbation utilisateur.

4. **Règles métier** :
   - Si emetteur ∈ liste VIP (cf APOLLIA.md `Email Triage — VIP List`) → priorité MAX.
   - Si subject contient mots-clés critiques (compétiteur direct, breach, fonds levés, acquisition,
     legal, contrat) → escalate + notification immédiate.
   - Templates de réponse : APOLLIA.md `Email Triage — Auto-Reply Templates`.

5. **Sortie finale** (formatée par on_plan_complete) :
   - Synthèse markdown : N emails triés, X urgent, Y newsletters, Z draft réponses préparés.
   - Liste des emails escaladés à l'utilisateur.
"""


class EmailTriageAgent(OrchestratedAgent):
    def manifest(self) -> dict[str, Any]:
        return {
            "name": "email-triage",
            "version": "1.0.0",
            "description": "Triage inbox orchestré ORIA, HITL avant envoi",
            "execution_mode": "orchestrated",
            "agent_type": "user",
            "system_prompt": SYSTEM_PROMPT,
            "tools_required": ["http_fetch", "memory_search"],
            "tools_optional": ["file_write"],
            "tools_requiring_approval": ["http_fetch"],
            "memory_namespace": "email-triage",
            "supports_a2a": False,
            "max_concurrent_tasks": 1,
            "step_budget": {
                "max_steps": 30,
                "max_tool_calls": 25,
                "wall_clock_secs": 1800,
            },
            "tags": ["email", "triage", "orchestrated"],
            "setup_notes": (
                "Voir setup.md. CRITIQUE : configurer credentials Gmail en mémoire et "
                "remplir APOLLIA.md sections classification/templates."
            ),
            "limitations": (
                "Apollia v0.1 sans outils gmail natifs — wrappers http_fetch requis. "
                "user.agents.hitl pas lu dynamiquement par le runtime — fixé au manifest."
            ),
        }

    async def run(self, task: dict[str, Any], ctx: Any) -> AIPResult:
        raise RuntimeError(
            "OrchestratedAgent.run() ne doit pas être appelé directement. "
            "ORIA gère l'exécution via Reasoner + ActorLoop."
        )

    def on_plan_complete(self, step_results: dict[str, Any]) -> dict[str, Any]:
        """Post-traitement : agrège les résultats du plan en une synthèse markdown."""
        triaged_count = 0
        escalated: list[str] = []
        actions: dict[str, int] = {"reply_draft": 0, "label": 0, "archive": 0, "skip": 0}
        errors: list[str] = []

        for step_id, result in step_results.items():
            status = result.get("status", "unknown")
            if status == "failed":
                errors.append(f"{step_id}: {result.get('error', 'unknown error')}")
                continue
            for part in result.get("output", []):
                if part.get("type") != "text":
                    continue
                text = part.get("text", "")
                # Parsing best-effort : on cherche des marqueurs simples produits par les steps
                if "[escalate]" in text.lower():
                    escalated.append(text[:200])
                if "[email-triaged]" in text.lower():
                    triaged_count += 1
                for action in actions:
                    if f"[action:{action}]" in text.lower():
                        actions[action] += 1

        synthesis_lines = [
            "# Triage Inbox — Synthèse",
            "",
            f"- **Emails triés :** {triaged_count}",
            f"- **Drafts préparés (HITL pending) :** {actions['reply_draft']}",
            f"- **Labels appliqués :** {actions['label']}",
            f"- **Archivés :** {actions['archive']}",
            f"- **Ignorés :** {actions['skip']}",
            "",
        ]

        if escalated:
            synthesis_lines.append(f"## ⚠️ Escaladés ({len(escalated)})")
            for e in escalated[:10]:
                synthesis_lines.append(f"- {e}")
            synthesis_lines.append("")

        if errors:
            synthesis_lines.append(f"## Erreurs steps ({len(errors)})")
            for err in errors[:5]:
                synthesis_lines.append(f"- {err}")

        text = "\n".join(synthesis_lines)
        return {"output": [{"type": "text", "text": text}]}


agent = EmailTriageAgent()


def manifest() -> dict[str, Any]:
    return agent.manifest()


async def run(task: dict[str, Any], ctx: Any) -> AIPResult:
    return await agent.run(task, ctx)


def on_plan_complete(step_results: dict[str, Any]) -> dict[str, Any]:
    return agent.on_plan_complete(step_results)
