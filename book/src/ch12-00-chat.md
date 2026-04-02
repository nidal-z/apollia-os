# Chat interactif

Les chapitres précédents couvrent les **tâches** : un agent reçoit une instruction, exécute, retourne un résultat. Ce modèle fonctionne parfaitement pour l'automatisation — mais pas pour la conversation.

Poser une question, obtenir une réponse, affiner, continuer — c'est un workflow fondamentalement différent. L'état persiste entre les messages, le LLM streame ses tokens au fur et à mesure, les outils peuvent nécessiter une approbation inline. Une tâche ordinaire ne sait pas faire ça.

Le **Chat interactif** d'Apollia OS est un sous-système dédié, indépendant du `TaskRouter`. Il gère les sessions conversationnelles, le streaming token-by-token, et deux modes d'exécution selon que vous voulez un chat direct avec le LLM ou un chat avec un agent Python complet.

---

## Tâches vs Sessions

| Dimension | Tâche (`TaskRouter`) | Session Chat |
|---|---|---|
| Durée de vie | Unique, fire-and-forget | Multiple messages, persistante |
| État | Stateless entre exécutions | Historique accumulé en SQLite |
| Résultat | Retourné en fin d'exécution | Streamé token-by-token via SSE |
| HITL | Suspension + reprise | Approbation inline, sans suspension |
| Mémoire agent | À initiative de l'agent | Mémoire utilisateur globale (`__user__`) |

Ces différences ont motivé la création d'un acteur Tokio séparé — `ChatSessionManager` — plutôt qu'une extension du `TaskRouter` (voir ADR-034).

---

## Deux modes d'exécution

### Chat Libre

Le LLM répond directement, piloté par Rust. Aucun processus Python n'est lancé. Les outils natifs peuvent être utilisés, mais aucun agent Python n'est impliqué.

```
Utilisateur ──► ChatSessionManager ──► LlmRouter.stream() ──► LLM
                                              │
                                         outils natifs
```

Adapté à l'usage opérateur courant : poser des questions, explorer des données, utiliser les outils natifs (file_io, shell, etc.).

### Chat Agent

Le message est délégué à un agent Python. ORIA orchestre la boucle ReAct complète — le LLM raisonne, appelle des outils, itère — et les tokens sont streamés vers la session.

```
Utilisateur ──► ChatSessionManager ──► AIPBridge ──► agent.run()
                                                          │
                                                    LLM + outils
                                                    (boucle ORIA)
```

Adapté aux agents spécialisés qui ont besoin de leur propre logique Python, de mémoire propre, d'outils personnalisés.

---

## Architecture — le ChatSessionManager

`ChatSessionManager` est l'acteur Tokio numéro 13 dans le Supervisor. Il reçoit des commandes via un `mpsc::channel` et expose une handle clonable :

```
Supervisor
  └── ChatSessionManager (acteur 13)
        ├── CreateSession  → ChatSession (mode, history, tools)
        ├── SendMessage    → LlmRouter.stream() ou AIPBridge
        ├── ResolveTool    → validation approbation inline HITL
        ├── ListSessions   → SQLite query
        ├── GetSession     → SQLite query
        ├── CloseSession   → status: closed
        └── Shutdown       → arrêt propre
```

Chaque session est persistée dans `chat.db` (SQLite), séparé de `tasks.db`. Un redémarrage du runtime ne perd pas les sessions.

---

## Ce que vous allez apprendre

- **Section 1 — Sessions et streaming** : créer une session, envoyer un message, suivre le stream SSE, les 7 endpoints API, la persistance SQLite
- **Section 2 — Chat Libre vs Chat Agent** : les deux modes, quand choisir l'un ou l'autre, les approbations HITL inline, l'intégration A2A dans le chat
- **Section 3 — Mémoire utilisateur** : le namespace `__user__`, les sources de confiance, l'injection dans le system prompt, la gestion de l'historique long
