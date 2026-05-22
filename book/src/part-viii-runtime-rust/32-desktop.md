# L'application Desktop

Apollia OS propose une application Desktop native (macOS, Linux) construite avec Tauri v2 et Svelte 5. Elle démarre le runtime complet en interne, un double-clic suffit pour avoir agents et chat interactif dans une interface graphique.

---

## Processus unique, runtime embarqué

L'application Desktop n'est pas un frontend séparé qui parle à un serveur. C'est un processus unique qui embarque le runtime Rust directement.

```
main() Tauri
  │
  ├── init_embedded(config) → RuntimeHandle
  │         └── thread "apollia-runtime"
  │               └── Supervisor.start() → AllReady (≤ 30s)
  │
  ├── setup_tray() : icône + menu contextuel
  │
  └── WebView Svelte ← commandes Tauri IPC + SSE localhost:7771
```

Conséquences pratiques :

- Pas besoin de démarrer `apollia-os start` séparément.
- La CLI reste fonctionnelle en parallèle via `/tmp/apollia.sock`.
- Fermer la fenêtre masque l'interface mais le runtime continue en arrière-plan dans le system tray.

---

## Navigation

La sidebar est un rail vertical fixe de 56px. Sept destinations principales et un accès Paramètres en pied :

| Icône | Vue | Rôle |
|---|---|---|
| Accueil | Dashboard | Vue d'ensemble temps réel |
| Chat | Chat | Sessions conversationnelles |
| Mes assistants | Agents | Démarrer, arrêter, inspecter |
| Projets | Projects | Isolation de contexte par projet |
| Mon travail | Tasks | Liste filtrée, timeline, input et output |
| Boîte de réception | Inbox | Approbations HITL en attente |
| Connexions | Integrations | Serveurs MCP |

Un badge numérique sur **Chat** indique les sessions actives, sur **Mon travail** les tâches in-flight (pulsation animée), sur **Boîte de réception** les approbations en attente. Un tooltip apparaît au survol de chaque icône.

Les vues complètes (LLM, Triggers, Mémoire, Notifications, Observabilité) restent accessibles via **Paramètres** ou la command palette (Cmd+K).

---

## Temps réel via SSE

Le frontend Svelte maintient une connexion SSE permanente vers `localhost:7771/api/v1/dashboard/stream`. Plusieurs stores Svelte se mettent à jour automatiquement :

| Store | Alimenté par |
|---|---|
| `agents` | `AgentRegistered`, `AgentReady`, `AgentStopped` |
| `tasks` | `TaskStarted`, `TaskCompleted`, `TaskFailed` |
| `pendingApprovals` | `TaskInputRequired` |
| `llmBackends` | `LlmModelReady`, `LlmModelFailed` |
| `triggers` | `TriggerFired`, `TriggerEnabled` |
| `connectionStatus` | État de la connexion SSE |

Reconnexion automatique avec backoff exponentiel (1s à 30s) si la connexion SSE est perdue.

---

## System tray

L'application se minimise dans le system tray quand vous fermez la fenêtre.

**Menu contextuel :**

1. "Ouvrir Apollia OS" : affiche et focus la fenêtre.
2. "N approbations en attente" : actif si des tâches attendent HITL.
3. "Quitter" : arrêt graceful avec drain.

**Notification native :** quand une tâche nécessite une approbation HITL et que la fenêtre est masquée, une notification système est envoyée avec un clic pour ouvrir directement la vue Approbations.

---

## Onboarding au premier lancement

Au premier lancement, un wizard 3 étapes guide l'utilisateur :

1. **Vérification environnement :** Python installé ? LLM configuré ? (affichage automatique).
2. **Premier agent :** sélecteur de fichier natif pour choisir un `.py`, démarrage avec feedback visuel.
3. **Première tâche :** zone de saisie, soumission, affichage du résultat via SSE.

Chaque étape peut être passée avec "Passer". L'onboarding ne bloque pas l'accès à l'interface.

---

## Coexistence CLI et Desktop

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
# macOS : .dmg disponible sur GitHub Releases
open Apollia-OS-0.1.0.dmg

# Linux : .AppImage ou .deb
chmod +x Apollia-OS-0.1.0.AppImage
./Apollia-OS-0.1.0.AppImage

# Ou via .deb
sudo dpkg -i apollia-os_0.1.0_amd64.deb
```

Le binaire Desktop embarque le runtime Rust complet. Aucune installation séparée du binaire CLI n'est requise pour utiliser le Desktop.

---

## ADRs

- `ADR-027` : Desktop Tauri processus unique, runtime embarqué
- `ADR-028` : Frontend Svelte, UX-first
- `ADR-030` : EventBus + Tauri events (remplace polling)
- `ADR-031` : i18n svelte-i18n (FR + EN)
- `ADR-065` : Auto-updater distribution

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
