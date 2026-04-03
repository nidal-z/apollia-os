"""slack-worker — Worker Agent for Slack messaging on Apollia OS.

Specialised agent that sends messages to Slack channels and reads recent
channel history via the Slack Web API.  Uses the ``http_fetch`` tool so no
Python packages are required — all HTTP calls go through the Apollia tool layer.

Send operations (send-message) require HITL approval before execution.

Required environment variables:
  APOLLIA_SLACK_TOKEN — Slack Bot OAuth token (xoxb-...)
"""

from __future__ import annotations

import os
from typing import Any

from apollia.agents import AIPResult, WorkerAgent

_SLACK_TOKEN_VAR = "APOLLIA_SLACK_TOKEN"

SYSTEM_PROMPT: str = """Tu es slack-worker, un agent spécialisé dans la messagerie Slack.

## RÈGLES ABSOLUES (non-négociables)

1. SEND-MESSAGE nécessite une approbation HITL — ne jamais l'exécuter sans confirmation.
   L'approbation est gérée par le runtime avant d'appeler ce skill.

2. Utiliser UNIQUEMENT l'outil http_fetch pour les appels à l'API Slack.
   Ne jamais utiliser python_executor avec requests ou httpx pour des appels Slack.
   RAISON : http_fetch est audité par le runtime — les appels raw Python contournent l'audit.

3. Authentification via le header Authorization: Bearer {APOLLIA_SLACK_TOKEN}.
   Ne jamais logger ni afficher le token dans les réponses.

4. Toujours vérifier le champ "ok" dans la réponse JSON Slack avant de retourner.
   Si "ok" est false : domain_error("slack_api_error", "{error}").

5. Pour read-channel : limiter à N messages via le paramètre "limit" de l'API
   conversations.history (défaut : 10, max : 50).

## ENDPOINTS API SLACK UTILISÉS

- POST https://slack.com/api/chat.postMessage — envoi de message
- GET  https://slack.com/api/conversations.history?channel={id}&limit={n} — lecture

## PATTERNS OBLIGATOIRES

### Envoi de message (send-message)
```json
POST https://slack.com/api/chat.postMessage
Authorization: Bearer {token}
Content-Type: application/json

{
  "channel": "#channel-name",
  "text": "message content"
}
```

### Lecture de canal (read-channel)
```
GET https://slack.com/api/conversations.history
  ?channel=C12345678
  &limit=10
Authorization: Bearer {token}
```

## GESTION DES ERREURS DOMAINE

- Token manquant → domain_error("missing_config", "Variable manquante : APOLLIA_SLACK_TOKEN")
- API Slack "ok": false → domain_error("slack_api_error", "{error_code}")
- Channel introuvable → domain_error("channel_not_found", "Channel introuvable : {channel}")
- Token invalide → domain_error("invalid_token", "Token Slack invalide ou révoqué")
- Rate limit (429) → domain_error("rate_limited", "API Slack rate limitée — réessayer dans {retry_after}s")

## FORMAT DE RÉPONSE

- send-message : confirmation avec channel, timestamp Slack (ts), et preview du message
- read-channel : liste des N messages (user, text, ts) en Markdown, du plus récent au plus ancien
"""


def manifest() -> dict[str, Any]:
    """Return the AIP agent manifest for slack-worker."""
    return {
        "name": "slack-worker",
        "version": "0.1.0",
        "description": (
            "Agent Slack : envoi et lecture de messages via la Slack Web API. "
            "Utilise http_fetch — aucun package Python requis. "
            "L'envoi de message (send-message) nécessite une approbation HITL."
        ),
        "execution_mode": "direct",
        "tools_required": ["http_fetch"],
        "tools_optional": [],
        "tools_requiring_approval": ["send-message"],
        "packages": [],
        "memory_namespace": "slack-worker",
        "supports_a2a": True,
        "skills": [
            {
                "id": "send-message",
                "name": "Envoyer un message Slack",
                "description": (
                    "Poste un message dans un channel Slack via chat.postMessage. "
                    "Nécessite une approbation HITL avant exécution. "
                    "Config requise : APOLLIA_SLACK_TOKEN."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "channel": {
                        "type": "string",
                        "description": "Nom du channel (#general) ou ID Slack (C12345678)",
                        "required": True,
                    },
                    "message": {
                        "type": "string",
                        "description": "Texte du message à envoyer",
                        "required": True,
                    },
                    "text": {
                        "type": "string",
                        "description": "Instruction en langage naturel",
                        "required": False,
                    },
                },
            },
            {
                "id": "read-channel",
                "name": "Lire un channel Slack",
                "description": (
                    "Lit les N derniers messages d'un channel via conversations.history. "
                    "Retourne : auteur, texte, timestamp. "
                    "Config requise : APOLLIA_SLACK_TOKEN."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "channel": {
                        "type": "string",
                        "description": "Nom du channel (#general) ou ID Slack (C12345678)",
                        "required": True,
                    },
                    "count": {
                        "type": "integer",
                        "description": "Nombre de messages à lire (défaut : 10, max : 50)",
                        "required": False,
                    },
                    "text": {
                        "type": "string",
                        "description": "Instruction en langage naturel",
                        "required": False,
                    },
                },
            },
        ],
        "tags": ["slack", "messaging", "chat", "notifications", "worker"],
        "max_concurrent_tasks": 2,
        "dangerous_tools_allowed": False,
    }


class SlackWorkerAgent(WorkerAgent):
    """Worker Agent specialised for Slack channel messaging and history reading.

    Configuration is loaded from the APOLLIA_SLACK_TOKEN environment variable
    at run time.  A missing token causes an immediate, descriptive failure before
    the LLM loop starts (Principle #4 — fail fast).  The send-message skill
    requires HITL approval, declared in ``tools_requiring_approval`` so the
    runtime enforces it without any agent-side workaround.
    """

    SYSTEM_PROMPT = SYSTEM_PROMPT
    MAX_STEPS = 6
    TEMPERATURE = 0.1

    def manifest(self) -> dict[str, Any]:
        """Return the AIP agent manifest for slack-worker."""
        return manifest()

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        """Execute the Slack task after verifying required configuration.

        Checks that APOLLIA_SLACK_TOKEN is set before starting the ReAct loop.
        Returns a descriptive failure if it is missing so the caller knows
        exactly which variable to configure.
        """
        if not os.environ.get(_SLACK_TOKEN_VAR):
            return AIPResult.failed(
                "MISSING_CONFIG",
                f"Variable d'environnement manquante : {_SLACK_TOKEN_VAR}",
            )

        user_message = (
            task.get("input", {}).get("text", "")
            if isinstance(task.get("input"), dict)
            else str(task.get("input", ""))
        )
        result = await self.react(task, ctx, user_message)
        if isinstance(result, dict):
            return result
        return AIPResult.completed(result)


agent = SlackWorkerAgent()
