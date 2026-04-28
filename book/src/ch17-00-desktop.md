# L'application Desktop

Apollia OS propose une application desktop native (macOS et Linux) construite avec Tauri v2 et Svelte 5. Elle démarre le runtime complet en interne — un double-clic suffit pour avoir agents, pipelines, et chat interactif dans une interface graphique.

---

## Processus unique — runtime embarqué

L'application desktop n'est pas un frontend séparé qui parle à un serveur. C'est un **processus unique** qui embarque le runtime Rust directement (ADR-027).

```
main() Tauri
  │
  ├── init_embedded(config) → RuntimeHandle
  │         └── thread "apollia-runtime"
  │               └── Supervisor.start() → AllReady (≤ 30s)
  │
  ├── setup_tray()   → icône + menu contextuel
  │
  └── WebView Svelte ← commandes Tauri IPC + SSE localhost:7771
```

**Conséquences pratiques :**
- Pas besoin de démarrer `apollia-os start` séparément
- La CLI reste fonctionnelle en parallèle via `/tmp/apollia.sock`
- Fermer la fenêtre masque l'interface mais le runtime continue en arrière-plan dans le system tray

---

## Navigation — rail d'icônes

La sidebar est un rail vertical fixe de 56px. Sept destinations principales + Paramètres en pied :

| Icône | Vue | Rôle |
|---|---|---|
| Accueil | Dashboard | Vue d'ensemble temps réel |
| Chat | Chat | Sessions conversationnelles |
| Mes assistants | Agents | Démarrer, arrêter, inspecter |
| Projets | Projects | Isolation de contexte par projet |
| Mon travail | Tasks | Liste filtrée, timeline, input/output |
| Boîte de réception | Inbox | Approbations HITL en attente |
| Connexions | Integrations | Serveurs MCP |

Un badge numérique sur **Chat** indique les sessions actives, sur **Mon travail** les tâches in-flight (pulsation animée), sur **Boîte de réception** les approbations en attente. Un tooltip apparaît au survol de chaque icône.

Les vues complètes (LLM, Triggers, Pipelines, Mémoire, Notifications, Observabilité) restent accessibles via **Paramètres** ou la command palette (Cmd+K).

---

## Temps réel via SSE

Le frontend Svelte maintient une connexion SSE permanente vers `localhost:7771/api/v1/dashboard/stream`. 7 stores Svelte se mettent à jour automatiquement :

| Store | Mis à jour par |
|---|---|
| `agents` | Événements `AgentRegistered`, `AgentReady`, `AgentStopped` |
| `tasks` | Événements `TaskStarted`, `TaskCompleted`, `TaskFailed` |
| `pendingApprovals` | Événement `TaskInputRequired` |
| `llmBackends` | Événements `LlmModelReady`, `LlmModelFailed` |
| `triggers` | Événements `TriggerFired`, `TriggerEnabled` |
| `pipelineRuns` | Événements `PipelineStarted`, `PipelineCompleted` |
| `connectionStatus` | État de la connexion SSE elle-même |

Reconnexion automatique avec backoff exponentiel (1s → 30s) si la connexion SSE est perdue.

---

## System tray

L'application se minimise dans le system tray quand vous fermez la fenêtre.

**Menu contextuel :**
1. "Ouvrir Apollia OS" — affiche/focus la fenêtre
2. "N approbations en attente" — actif si des tâches attendent HITL
3. "Quitter" — arrêt graceful avec drain

**Notification native :** quand une tâche nécessite une approbation HITL et que la fenêtre est masquée, une notification système est envoyée avec un clic pour ouvrir directement la vue Approbations.

---

## Onboarding — premier lancement

Au premier lancement, un wizard 3 étapes guide l'utilisateur :

1. **Vérification environnement** — Python installé ? LLM configuré ? (affichage ✓/✗ automatique)
2. **Premier agent** — sélecteur de fichier natif pour choisir un `.py`, démarrage avec feedback visuel
3. **Première tâche** — zone de saisie, soumission, affichage du résultat via SSE

Chaque étape peut être passée avec "Passer" — l'onboarding ne bloque pas l'accès à l'interface.

---

## Coexistence CLI + Desktop

Un seul runtime, deux interfaces d'accès :

```bash
# Depuis le terminal, pendant que le Desktop tourne
apollia-os agent list       # lit via /tmp/apollia.sock
apollia-os task list        # même runtime que le Desktop
apollia-os trigger fire xyz # déclenché dans le même runtime
```

Les deux interfaces voient le même état en temps réel. Un agent démarré via la CLI apparaît immédiatement dans la liste du Desktop.

---

## Installation

```bash
# macOS — .dmg disponible sur GitHub Releases
open Apollia-OS-0.x.x.dmg

# Linux — .AppImage ou .deb
chmod +x Apollia-OS-0.x.x.AppImage
./Apollia-OS-0.x.x.AppImage

# Ou via .deb
sudo dpkg -i apollia-os_0.x.x_amd64.deb
```

Le binaire embarque le runtime Rust complet — aucune installation séparée de `apollia-os` CLI n'est requise pour utiliser le Desktop.
