"""Email Triage — agent L3 orchestré ORIA.

ORIA (Reasoner) génère le plan, ActorLoop l'exécute step-by-step. L'agent
fournit uniquement le ``system_prompt`` (via ``@orchestrated``) et un
``on_plan_complete`` pour formatter la synthèse finale.

⚠️ Apollia v0.1 ne fournit pas d'outils Gmail natifs. Cet agent assume des
wrappers ``http_fetch`` vers Gmail API (ou équivalent Microsoft Graph).
Voir setup.md.
"""

from __future__ import annotations

from typing import Any

from apollia import agent, orchestrated


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


@agent(
    name="email-triage",
    version="0.1.0",
    description="Triage inbox orchestré ORIA, HITL avant envoi",
    tags=("email", "triage", "orchestrated"),
    memory_namespace="email-triage",
    tools_required=("http_fetch", "memory_search"),
    step_budget={"max_steps": 30, "max_tool_calls": 25, "wall_clock_secs": 1800},
)
@orchestrated(system_prompt=SYSTEM_PROMPT)
class EmailTriage:
    """Email triage — exécution pilotée par ORIA."""

    def on_plan_complete(self, step_results: dict[str, Any]) -> dict[str, Any]:
        """Agrège les résultats du plan en une synthèse markdown."""
        triaged_count = 0
        escalated: list[str] = []
        actions: dict[str, int] = {
            "reply_draft": 0,
            "label": 0,
            "archive": 0,
            "skip": 0,
        }
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
                low = text.lower()
                if "[escalate]" in low:
                    escalated.append(text[:200])
                if "[email-triaged]" in low:
                    triaged_count += 1
                for action in actions:
                    if f"[action:{action}]" in low:
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

        return {"output": [{"type": "text", "text": "\n".join(synthesis_lines)}]}
