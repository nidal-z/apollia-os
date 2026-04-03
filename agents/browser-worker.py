"""browser-worker — Worker Agent for web browsing and screenshot capture on Apollia OS.

Specialised agent that fetches web pages and extracts their main text content,
or captures headless screenshots.  Both skills delegate execution to
``python_executor`` using ``requests`` and ``beautifulsoup4``.

The agent runs under the ``NetworkRestricted`` sandbox profile, which allows
outbound HTTP/HTTPS traffic while blocking access to the local network.

Required packages (installed automatically at agent initialisation):
  requests, beautifulsoup4
"""

from __future__ import annotations

from typing import Any

from apollia.agents import AIPResult, WorkerAgent

SYSTEM_PROMPT: str = """Tu es browser-worker, un agent spécialisé dans la navigation web et la capture d'écran.

## RÈGLES ABSOLUES (non-négociables)

1. N'accède JAMAIS à des URLs internes (localhost, 127.x, 10.x, 192.168.x, 172.16-31.x).
   RAISON : le sandbox NetworkRestricted autorise le web public uniquement.
   Répondre avec domain_error("private_network_blocked", "Accès au réseau privé interdit").

2. Utilise TOUJOURS requests avec un timeout explicite (30 secondes) et un User-Agent standard.
   Ne jamais faire une requête sans timeout — cela bloquerait indéfiniment l'agent.

3. Pour browse-url : extraire le texte via BeautifulSoup en supprimant les balises script/style.
   Retourner au maximum 8 000 caractères pour éviter de saturer le contexte LLM.

4. Pour screenshot-url : utiliser playwright (si disponible) ou signaler que le rendu visuel
   nécessite un environnement headless. Retourner le chemin du fichier PNG créé.

5. Toujours vérifier le code HTTP de la réponse avant d'extraire le contenu.
   Codes 4xx → domain_error("http_client_error", ...).
   Codes 5xx → domain_error("http_server_error", ...).

## IMPORTS STANDARDS

```python
import requests
from bs4 import BeautifulSoup
```

## PATTERNS OBLIGATOIRES

### Extraction de texte (browse-url)
```python
headers = {"User-Agent": "ApolliaOS/1.0 browser-worker"}
response = requests.get(url, headers=headers, timeout=30)
response.raise_for_status()

soup = BeautifulSoup(response.text, "html.parser")
for tag in soup(["script", "style", "nav", "footer", "header"]):
    tag.decompose()

text = soup.get_text(separator=" ", strip=True)
# Tronquer à 8 000 caractères pour ne pas saturer le contexte
return text[:8000]
```

### Capture d'écran headless (screenshot-url)
```python
from playwright.sync_api import sync_playwright

with sync_playwright() as p:
    browser = p.chromium.launch(headless=True)
    page = browser.new_page()
    page.goto(url, timeout=30000)
    page.screenshot(path=output_path)
    browser.close()
```

## GESTION DES ERREURS DOMAINE

- URL réseau privé → domain_error("private_network_blocked", "Accès au réseau privé interdit : {url}")
- Code HTTP 4xx → domain_error("http_client_error", "Erreur client HTTP {status} : {url}")
- Code HTTP 5xx → domain_error("http_server_error", "Erreur serveur HTTP {status} : {url}")
- Timeout connexion → domain_error("connection_timeout", "Timeout après 30s : {url}")
- Package playwright absent → domain_error("missing_package", "playwright requis pour screenshot-url")

## FORMAT DE RÉPONSE

- browse-url : texte extrait (max 8 000 chars) + titre de la page + URL canonique
- screenshot-url : chemin du fichier PNG + dimensions (largeur × hauteur)
"""


def manifest() -> dict[str, Any]:
    """Return the AIP agent manifest for browser-worker."""
    return {
        "name": "browser-worker",
        "version": "0.1.0",
        "description": (
            "Agent de navigation web : extraction de texte et capture d'écran headless. "
            "Fonctionne sous sandbox NetworkRestricted (web public uniquement). "
            "Packages requis : requests, beautifulsoup4."
        ),
        "execution_mode": "direct",
        "tools_required": ["python_executor"],
        "tools_optional": [],
        "tools_requiring_approval": [],
        "packages": ["requests", "beautifulsoup4"],
        "sandbox_profile": "NetworkRestricted",
        "memory_namespace": "browser-worker",
        "supports_a2a": True,
        "skills": [
            {
                "id": "browse-url",
                "name": "Naviguer vers une URL",
                "description": (
                    "Effectue un GET sur une URL et extrait le texte principal de la page. "
                    "Supprime les balises script, style, nav et footer. "
                    "Retourne au maximum 8 000 caractères."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "url": {
                        "type": "string",
                        "description": "URL complète à récupérer (https:// obligatoire)",
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
                "id": "screenshot-url",
                "name": "Capturer une URL en image",
                "description": (
                    "Capture une capture d'écran headless d'une URL via Playwright Chromium. "
                    "Retourne le chemin du fichier PNG généré."
                ),
                "input_modes": ["text"],
                "output_modes": ["text"],
                "input_schema": {
                    "url": {
                        "type": "string",
                        "description": "URL complète à capturer (https:// obligatoire)",
                        "required": True,
                    },
                    "output_path": {
                        "type": "string",
                        "description": "Chemin du fichier PNG de sortie (défaut : /tmp/screenshot.png)",
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
        "tags": ["browser", "web", "scraping", "screenshot", "worker"],
        "max_concurrent_tasks": 2,
        "dangerous_tools_allowed": False,
    }


class BrowserWorkerAgent(WorkerAgent):
    """Worker Agent specialised for web browsing and headless screenshot capture.

    Runs under the NetworkRestricted sandbox profile, which grants outbound
    HTTP/HTTPS access while blocking all private-range addresses.  The expertise
    (URL validation, text extraction via BeautifulSoup, Playwright screenshot
    pattern, domain error codes) is compiled into SYSTEM_PROMPT so the agent
    behaves reliably on small LLMs (7B+).
    """

    SYSTEM_PROMPT = SYSTEM_PROMPT
    MAX_STEPS = 6
    TEMPERATURE = 0.1

    def manifest(self) -> dict[str, Any]:
        """Return the AIP agent manifest for browser-worker."""
        return manifest()

    async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
        """Execute the browsing task using the ReAct loop.

        Extracts the user message from the task input, delegates to the
        inherited ReAct loop, and wraps the result in an AIPResult dict.
        """
        user_message = (
            task.get("input", {}).get("text", "")
            if isinstance(task.get("input"), dict)
            else str(task.get("input", ""))
        )
        result = await self.react(task, ctx, user_message)
        if isinstance(result, dict):
            return result
        return AIPResult.completed(result)


agent = BrowserWorkerAgent()
