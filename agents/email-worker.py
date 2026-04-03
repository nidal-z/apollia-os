"""email-worker — Worker Agent for SMTP/IMAP email operations on Apollia OS.

Specialised agent that sends emails via SMTP and reads an inbox via IMAP.
Uses Python stdlib exclusively (``smtplib``, ``imaplib``, ``email``) —
no third-party packages required.

Send operations (send-email) require HITL approval before execution.

Required environment variables:
  APOLLIA_SMTP_HOST     — SMTP server hostname
  APOLLIA_SMTP_PORT     — SMTP server port (typically 465 or 587)
  APOLLIA_SMTP_USER     — SMTP authentication username
  APOLLIA_SMTP_PASSWORD — SMTP authentication password
  APOLLIA_IMAP_HOST     — IMAP server hostname (read-inbox only)
"""

from __future__ import annotations

import os
from typing import Any

from apollia.agents import AIPResult, WorkerAgent

_SMTP_ENV_VARS: tuple[str, ...] = (
    "APOLLIA_SMTP_HOST",
    "APOLLIA_SMTP_PORT",
    "APOLLIA_SMTP_USER",
    "APOLLIA_SMTP_PASSWORD",
)
_IMAP_ENV_VAR = "APOLLIA_IMAP_HOST"

SYSTEM_PROMPT: str = """Tu es email-worker, un agent spécialisé dans l'envoi et la lecture d'emails.

## RÈGLES ABSOLUES (non-négociables)

1. SEND-EMAIL nécessite une approbation HITL — ne jamais l'exécuter sans confirmation.
   L'approbation est gérée par le runtime avant d'appeler ce skill.

2. Utiliser EXCLUSIVEMENT la stdlib Python : smtplib, imaplib, email.
   Ne jamais importer requests, httpx ou tout autre package HTTP externe.

3. Toujours utiliser SMTP_SSL (port 465) ou SMTP avec starttls() (port 587).
   Ne jamais envoyer d'email en clair sur le port 25.

4. Toujours encoder le corps de l'email en UTF-8.
   Utiliser email.mime.text.MIMEText avec charset='utf-8'.

5. Pour read-inbox : limiter la lecture à N messages (défaut : 10) via IMAP SEARCH.
   Ne jamais télécharger toute la boîte en une seule requête.

## IMPORTS STANDARDS

```python
import smtplib
import imaplib
import email
from email.mime.text import MIMEText
from email.mime.multipart import MIMEMultipart
import os
```

## PATTERNS OBLIGATOIRES

### Envoi SMTP (port 587 avec STARTTLS)
```python
host = os.environ["APOLLIA_SMTP_HOST"]
port = int(os.environ["APOLLIA_SMTP_PORT"])
user = os.environ["APOLLIA_SMTP_USER"]
password = os.environ["APOLLIA_SMTP_PASSWORD"]

msg = MIMEMultipart("alternative")
msg["Subject"] = subject
msg["From"] = user
msg["To"] = recipient
msg.attach(MIMEText(body, "plain", "utf-8"))

with smtplib.SMTP(host, port, timeout=30) as server:
    server.ehlo()
    server.starttls()
    server.login(user, password)
    server.sendmail(user, [recipient], msg.as_string())
```

### Lecture IMAP
```python
host = os.environ["APOLLIA_IMAP_HOST"]
user = os.environ["APOLLIA_SMTP_USER"]
password = os.environ["APOLLIA_SMTP_PASSWORD"]

with imaplib.IMAP4_SSL(host) as conn:
    conn.login(user, password)
    conn.select("INBOX")
    _, uids = conn.search(None, "ALL")
    last_n = uids[0].split()[-n:]
    messages = []
    for uid in reversed(last_n):
        _, data = conn.fetch(uid, "(RFC822)")
        msg = email.message_from_bytes(data[0][1])
        messages.append({
            "from": msg.get("From"),
            "subject": msg.get("Subject"),
            "date": msg.get("Date"),
        })
```

## GESTION DES ERREURS DOMAINE

- Variable d'env manquante → domain_error("missing_config", "Variable(s) manquante(s) : {vars}")
- Authentification SMTP échouée → domain_error("smtp_auth_error", "Authentification SMTP échouée")
- Authentification IMAP échouée → domain_error("imap_auth_error", "Authentification IMAP échouée")
- Timeout connexion → domain_error("connection_timeout", "Timeout après 30s : {host}")
- Adresse email invalide → domain_error("invalid_email", "Adresse invalide : {address}")

## FORMAT DE RÉPONSE

- send-email : confirmation avec destinataire, sujet, et timestamp d'envoi
- read-inbox : liste des N messages (from, subject, date) en Markdown
"""


def manifest() -> dict[str, Any]:
    """Return the AIP agent manifest for email-worker."""
    return {
        "name": "email-worker",
        "version": "0.1.0",
        "description": (
            "Agent email : envoi SMTP et lecture IMAP. "
            "Utilise la stdlib Python (smtplib, imaplib) — aucun package tiers requis. "
            "L'envoi d'email (send-email) nécessite une approbation HITL."
        ),
        "execution_mode": "direct",
        "tools_required": ["python_executor"],
        "tools_optional": [],
        "tools_requiring_approval": ["send-email"],
        "packages": [],
        "memory_namespace": "email-worker",
        "supports_a2a": True,
        "skills": [
            {
                "id": "send-email",
                "name": "Envoyer un email",
                "description": (
                    "Envoie un email via SMTP (port 465 SSL ou 587 STARTTLS). "
                    "Nécessite une approbation HITL avant exécution. "
                    "Config requise : APOLLIA_SMTP_HOST, APOLLIA_SMTP_PORT, "
                    "APOLLIA_SMTP_USER, APOLLIA_SMTP_PASSWORD."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "to": {
                        "type": "string",
                        "description": "Adresse email du destinataire",
                        "required": True,
                    },
                    "subject": {
                        "type": "string",
                        "description": "Objet de l'email",
                        "required": True,
                    },
                    "body": {
                        "type": "string",
                        "description": "Corps de l'email (texte brut ou HTML)",
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
                "id": "read-inbox",
                "name": "Lire la boîte de réception",
                "description": (
                    "Lit les N derniers messages IMAP et retourne from, subject, date. "
                    "Config requise : APOLLIA_IMAP_HOST, APOLLIA_SMTP_USER, APOLLIA_SMTP_PASSWORD."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
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
        "tags": ["email", "smtp", "imap", "send", "inbox", "worker"],
        "max_concurrent_tasks": 1,
        "dangerous_tools_allowed": False,
    }


def _check_smtp_config() -> list[str]:
    """Return a list of missing SMTP environment variable names."""
    return [var for var in _SMTP_ENV_VARS if not os.environ.get(var)]


def _check_imap_config() -> list[str]:
    """Return a list of missing IMAP environment variable names.

    IMAP read also requires SMTP credentials for the username and password.
    """
    required = (*_SMTP_ENV_VARS, _IMAP_ENV_VAR)
    return [var for var in required if not os.environ.get(var)]


class EmailWorkerAgent(WorkerAgent):
    """Worker Agent specialised for SMTP email sending and IMAP inbox reading.

    Configuration is loaded from environment variables at run time.  Any missing
    variable causes an immediate, descriptive failure before the LLM loop starts
    (Principle #4 — fail fast).  The send-email skill requires HITL approval,
    declared in ``tools_requiring_approval`` so the runtime can enforce it.
    """

    SYSTEM_PROMPT = SYSTEM_PROMPT
    MAX_STEPS = 6
    TEMPERATURE = 0.1

    def manifest(self) -> dict[str, Any]:
        """Return the AIP agent manifest for email-worker."""
        return manifest()

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        """Execute the email task after verifying required configuration.

        Checks that all necessary environment variables are present before
        starting the ReAct loop.  Returns a descriptive failure if any are
        missing so the caller knows exactly what to configure.
        """
        skill = (
            task.get("skill")
            or (task.get("input", {}).get("skill") if isinstance(task.get("input"), dict) else None)
        )

        if skill == "read-inbox":
            missing = _check_imap_config()
        else:
            missing = _check_smtp_config()

        if missing:
            missing_str = ", ".join(missing)
            return AIPResult.failed(
                "MISSING_CONFIG",
                f"Variables d'environnement manquantes : {missing_str}",
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


agent = EmailWorkerAgent()
