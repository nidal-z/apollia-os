"""web-watcher — Agent de veille automatique sur pages web.

╔══════════════════════════════════════════════════════════════════════════════╗
║  SCÉNARIO MÉTIER                                                             ║
╚══════════════════════════════════════════════════════════════════════════════╝

À intervalle configurable, cet agent :

  1. Récupère le contenu d'une URL cible via curl (zéro dépendance Python).
  2. Nettoie le HTML pour extraire le texte visible.
  3. Compare le contenu actuel avec la version précédente stockée en mémoire.
  4. Si aucun changement → retour immédiat, pas d'appel LLM, zéro coût.
  5. Si changement détecté → envoie les deux versions au LLM, qui décrit
     les modifications en français et rédige un rapport Markdown structuré.
  6. Sauvegarde le nouveau contenu en mémoire pour le prochain run.

L'URL n'est jamais codée en dur — elle provient de l'input_template du
trigger. On peut déployer plusieurs watchers en ajoutant des triggers
dans apollia.toml.

Cas d'usage : veille concurrentielle, suivi des marchés publics (BOAMP,
Journal Officiel), surveillance de sites d'emploi, agrégateurs d'actualités.

╔══════════════════════════════════════════════════════════════════════════════╗
║  FONCTIONNALITÉS APOLLIA DÉMONTRÉES                                          ║
╚══════════════════════════════════════════════════════════════════════════════╝

  • Trigger interval      Se déclenche toutes les N minutes pour interroger l'URL
  • Mode Direct           Boucle ReAct — le LLM décrit les changements en langage naturel
  • bash_executor         Pipeline curl + sed pour récupérer et nettoyer le HTML
  • file_io               Écrit le rapport de changement structuré en Markdown
  • Moteur mémoire        Stocke le contenu précédent pour détection de différences
  • Dégradation gracieuse Fonctionne sans LLM (détecte le changement, saute la description)

╔══════════════════════════════════════════════════════════════════════════════╗
║  CONFIGURATION TRIGGER  (ajouter dans ~/.apollia/apollia.toml)              ║
╚══════════════════════════════════════════════════════════════════════════════╝

  [[triggers]]
  id             = "watch-hn"
  agent          = "web-watcher"
  enabled        = true
  on_busy        = "drop"
  input_template = "Vérifie https://news.ycombinator.com/"

  [triggers.source]
  type  = "interval"
  every = "5m"

╔══════════════════════════════════════════════════════════════════════════════╗
║  DÉMARRAGE RAPIDE                                                            ║
╚══════════════════════════════════════════════════════════════════════════════╝

  1. Démarrer Apollia et enregistrer l'agent :
       apollia-os agent start agents/web-watcher.py

  2. Lancer manuellement (le premier run indexe la page) :
       apollia-os run web-watcher "Vérifie https://news.ycombinator.com/"

  3. Relancer après quelques minutes (détecte les changements si la page a bougé) :
       apollia-os run web-watcher "Vérifie https://news.ycombinator.com/"

  4. Ou laisser le trigger se déclencher automatiquement toutes les 5 minutes.

  5. Lire le rapport de changement :
       cat ~/.apollia/reports/watch-news.ycombinator.com-*.md
"""

import re
from datetime import datetime
from urllib.parse import urlparse

from apollia_base import AIPResult, BaseReActAgent

# ─────────────────────────────────────────────────────────────────────────────
# Configuration
# ─────────────────────────────────────────────────────────────────────────────

# Nombre maximum de caractères de texte envoyés au contexte LLM.
# Maintient la consommation de tokens prévisible tout en couvrant la majorité
# des pages d'accueil.
MAX_CONTENT_CHARS: int = 4_000

# Nombre maximum de lignes extraites de la page (après nettoyage HTML).
# Empêche une sortie incontrôlée sur les pages très longues.
MAX_EXTRACTED_LINES: int = 200

# Timeout HTTP en secondes pour les requêtes curl.
CURL_TIMEOUT_SECONDS: int = 15

# Répertoire de sortie des rapports (relatif à $HOME — racine sandbox file_io).
REPORTS_DIR: str = ".apollia/reports"

# Préfixe des clés mémoire pour stocker le contenu des pages entre les runs.
# Clé complète : "page-content:<domaine>" (ex. "page-content:news.ycombinator.com")
MEMORY_KEY_PREFIX: str = "page-content:"

# Scores d'importance pour la mémoire épisodique.
IMPORTANCE_INDEXATION: float = 0.5
IMPORTANCE_CHANGEMENT: float = 0.8


# ─────────────────────────────────────────────────────────────────────────────
# Prompt système — identité du LLM et contrat de sortie
# ─────────────────────────────────────────────────────────────────────────────

_SYSTEM_PROMPT = """\
Tu es Web Watcher, un expert en détection et description de changements \
sur les pages web. Tu rédiges en français clair et professionnel.

Tu reçois deux versions du contenu textuel d'une page web : la version \
PRÉCÉDENTE et la version ACTUELLE. Ton travail :

1. COMPARER les deux versions et identifier ce qui a changé :
   - Éléments apparus (nouveaux articles, nouvelles entrées)
   - Éléments disparus (retirés de la page)
   - Éléments modifiés (changement de position, de texte)

2. ÉCRIRE un rapport de changement via file_io :
   - action: "write"
   - path: le report_path indiqué dans le message utilisateur
   - content: un rapport Markdown propre suivant ce template :

# Changement détecté — <domaine>
**Date :** <date>
**URL :** <url>

## Résumé du changement
<2-5 phrases décrivant les changements clés>

## Détail
<liste à puces des éléments ajoutés, modifiés ou supprimés>

3. Retourner un final_answer avec un résumé en une ligne :
   "<N> changements détectés sur <domaine> — <description courte>"

RÈGLES :
- Décrire les changements du point de vue contenu/métier, pas technique.
- Ne jamais inventer de changements absents des données.
- Compléter toutes les étapes avant de retourner final_answer.
- Utiliser UNIQUEMENT les outils disponibles.
"""


# ─────────────────────────────────────────────────────────────────────────────
# Agent
# ─────────────────────────────────────────────────────────────────────────────

class WebWatcherAgent(BaseReActAgent):
    """Agent de surveillance de pages web utilisant la boucle ReAct.

    Flux (premier run — indexation) :
      bash_executor × 1  → curl + sed pour récupérer et nettoyer le contenu
      memory.remember    → stocke le contenu sous "page-content:<domaine>"
      → AIPResult.completed("Première vérification — contenu indexé.")

    Flux (runs suivants — aucun changement) :
      bash_executor × 1  → récupère le contenu actuel
      memory.recall      → récupère le contenu précédent
      comparaison         → contenu identique
      → AIPResult.completed("Aucun changement détecté.")

    Flux (runs suivants — changement détecté) :
      bash_executor × 1  → récupère le contenu actuel
      memory.recall      → récupère le contenu précédent
      comparaison         → contenu différent
      boucle ReAct LLM   → décrit les changements + rédige le rapport Markdown
      memory.remember    → met à jour le contenu stocké
      → AIPResult.completed("<résumé>")

    Flux (sans LLM — dégradation gracieuse) :
      Identique au flux précédent mais sans génération de rapport.
      → AIPResult.completed("Changement détecté (pas de LLM pour décrire).")
    """

    SYSTEM_PROMPT = _SYSTEM_PROMPT
    MAX_STEPS = 6

    def manifest(self) -> dict:
        return {
            "name": "web-watcher",
            "version": "1.0.0",
            "description": (
                "Surveille une page web à intervalle régulier et génère un "
                "rapport de changement structuré quand le contenu évolue. "
                "L'URL cible provient du trigger — aucun site n'est hardcodé."
            ),
            "tools_required": ["bash_executor", "file_io"],
            "tools_optional": [],
            "memory_namespace": "web-watcher",
            "execution_mode": "direct",
            "max_concurrent_tasks": 1,
            "step_budget": {
                "max_steps": 8,
                "max_tool_calls": 6,
                "wall_clock_secs": 60,
            },
            "dangerous_tools_allowed": False,
            "tags": ["monitoring", "web", "veille", "competitive-intelligence"],
        }

    async def run(self, task: dict, ctx) -> dict:
        """Point d'entrée appelé par le runtime Apollia pour chaque tâche.

        1. Extraire l'URL cible depuis l'input du trigger.
        2. Récupérer et nettoyer le contenu de la page via bash_executor.
        3. Comparer avec le contenu précédent stocké en mémoire.
        4. Si changement, déléguer à la boucle ReAct LLM pour la description.
        5. Sauvegarder le nouveau contenu en mémoire pour le prochain run.
        """
        url = _extract_url(task)
        if not url:
            return AIPResult.failed(
                "NO_URL",
                "Impossible d'extraire une URL depuis l'input de la tâche. "
                "Format attendu : 'Vérifie https://example.com/'",
            )

        domain = urlparse(url).netloc
        task_id = task.get("task_id")

        # ── Récupération du contenu de la page ───────────────────────────────
        content = await _fetch_page(ctx, url)
        if not content:
            return AIPResult.failed(
                "FETCH_FAILED",
                f"Impossible de récupérer le contenu de {url}. "
                "Vérifiez que l'URL est accessible et que curl est disponible.",
            )

        current_text = content[:MAX_CONTENT_CHARS]

        # ── Comparaison avec le contenu précédent en mémoire ─────────────────
        memory_key = f"{MEMORY_KEY_PREFIX}{domain}"
        previous_text = await _recall_previous(ctx, memory_key)

        # Premier run — pas de contenu précédent, on indexe
        if previous_text is None:
            await _save_content(ctx, memory_key, current_text, domain, url, task_id)
            return AIPResult.completed(
                f"Première vérification de {domain} — contenu indexé."
            )

        # Aucun changement détecté
        if _content_is_similar(previous_text, current_text):
            return AIPResult.completed(
                f"Aucun changement détecté sur {domain}"
            )

        # ── Changement détecté ───────────────────────────────────────────────
        if ctx.llm is None:
            await _save_content(ctx, memory_key, current_text, domain, url, task_id)
            return AIPResult.completed(
                f"Changement détecté sur {domain} (pas de LLM configuré "
                "pour décrire les modifications)."
            )

        now = datetime.now()
        report_path = (
            f"{REPORTS_DIR}/watch-{_sanitize_domain(domain)}"
            f"-{now.strftime('%Y-%m-%d_%H%M')}.md"
        )
        today = now.strftime("%Y-%m-%d %H:%M")

        result = await self._run_llm(
            task, ctx, url, domain, report_path, today,
            previous_text, current_text,
        )

        # Sauvegarder le nouveau contenu quel que soit le résultat LLM
        await _save_content(ctx, memory_key, current_text, domain, url, task_id)

        if isinstance(result, dict):
            return result
        return AIPResult.completed(result)

    # ── Chemin LLM ──────────────────────────────────────────────────────────

    async def _run_llm(
        self, task, ctx, url, domain, report_path, today,
        old_content, new_content,
    ):
        """Lance la boucle ReAct avec l'ancien et le nouveau contenu."""
        extra_context = (
            f"=== CONTENU PRÉCÉDENT ({domain}) ===\n"
            f"{old_content}\n"
            f"=== FIN CONTENU PRÉCÉDENT ===\n\n"
            f"=== CONTENU ACTUEL ({domain}) ===\n"
            f"{new_content}\n"
            f"=== FIN CONTENU ACTUEL ==="
        )
        user_msg = (
            f"Compare les deux versions ci-dessus et décris les changements.\n"
            f"URL : {url}\n"
            f"Report path : {report_path}\n"
            f"Date du jour : {today}"
        )
        return await self.react(task, ctx, user_msg, extra_context=extra_context)


# ─────────────────────────────────────────────────────────────────────────────
# Fonctions utilitaires
# ─────────────────────────────────────────────────────────────────────────────

async def _fetch_page(ctx, url: str) -> str:
    """Récupère une URL via curl, nettoie les balises HTML, retourne du texte brut.

    Pipeline : curl → suppression <script>/<style> → suppression balises →
    suppression lignes vides → troncature à MAX_EXTRACTED_LINES lignes.
    Aucune dépendance Python externe requise.
    """
    if not ctx.tools:
        return ""
    try:
        result = await ctx.tools.call("bash_executor", {
            "command": (
                f"curl -sL --max-time {CURL_TIMEOUT_SECONDS} "
                f"--user-agent 'Apollia-WebWatcher/1.0' '{url}' "
                f"| sed 's/<script[^>]*>.*<\\/script>//g' "
                "| sed 's/<style[^>]*>.*<\\/style>//g' "
                "| sed 's/<[^>]*>//g' "
                "| sed '/^[[:space:]]*$/d' "
                f"| head -{MAX_EXTRACTED_LINES}"
            ),
            "timeout_seconds": CURL_TIMEOUT_SECONDS + 5,
        })
        stdout = result.get("stdout", "") if isinstance(result, dict) else str(result)
        return stdout.strip()
    except Exception:
        return ""


def _extract_url(task: dict) -> str:
    """Extrait la première URL HTTP(S) depuis le texte d'input de la tâche."""
    for part in task.get("input", {}).get("parts", []):
        if part.get("type") != "text":
            continue
        match = re.search(r"https?://[^\s\"'>]+", part["text"])
        if match:
            return match.group(0).rstrip(".,;:)")
    return ""


async def _recall_previous(ctx, memory_key: str) -> str | None:
    """Récupère le contenu de page précédemment stocké en mémoire."""
    if ctx.memory is None:
        return None
    try:
        return await ctx.memory.recall(memory_key)
    except Exception:
        return None


def _content_is_similar(old: str, new: str) -> bool:
    """Compare deux contenus textuels après normalisation des espaces.

    Retourne True si les textes normalisés sont identiques.
    """
    old_normalized = re.sub(r"\s+", " ", old.strip())
    new_normalized = re.sub(r"\s+", " ", new.strip())
    return old_normalized == new_normalized


def _sanitize_domain(domain: str) -> str:
    """Nettoie un nom de domaine pour l'utiliser dans un nom de fichier."""
    return re.sub(r"[^a-zA-Z0-9.-]", "_", domain)


async def _save_content(
    ctx,
    memory_key: str,
    content: str,
    domain: str,
    url: str,
    task_id: str | None,
) -> None:
    """Persiste le contenu actuel en mémoire sémantique et enregistre un événement."""
    if ctx.memory is None:
        return

    # Mémoire sémantique — stocke le texte de la page pour comparaison future.
    try:
        await ctx.memory.remember(
            key=memory_key,
            value=content,
            source="web-watcher",
        )
    except Exception:
        pass

    # Mémoire épisodique — enregistre l'événement d'indexation pour traçabilité.
    try:
        timestamp = datetime.now().strftime("%Y-%m-%d %H:%M")
        await ctx.memory.record(
            content=f"[{timestamp}] Contenu indexé pour {domain} ({url})",
            importance=IMPORTANCE_INDEXATION,
            task_id=task_id,
        )
    except Exception:
        pass


# ─────────────────────────────────────────────────────────────────────────────
# Instance module-level (requis par le contrat AIP Apollia)
# ─────────────────────────────────────────────────────────────────────────────

agent = WebWatcherAgent()
