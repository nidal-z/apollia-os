# Chat Libre vs Chat Agent

Le choix du mode se fait à la création de la session et ne peut pas être changé en cours de route. Ce choix détermine qui traite vos messages — et à quel coût de ressources.

---

## Chat Libre — Rust pur, sans Python

En mode `libre`, c'est le `ChatSessionManager` Rust qui appelle directement `LlmRouter.stream`. Aucun processus Python n'est démarré, aucun agent n'est instancié.

```
Message utilisateur
  │
  ▼
ChatSessionManager
  │
  ├── Construit le contexte :
  │   système + mémoire utilisateur + résumé + N derniers messages
  │
  ▼
LlmRouter.stream(context)
  │
  ├── token par token → SSE
  │
  ▼
ChatResponseCompleted → enregistré en SQLite
```

Les outils natifs (`file_io`, `shell`, `web_search`, etc.) sont disponibles. Chaque appel d'outil déclenche un `ChatApprovalRequired` — en Chat Libre, tous les outils requièrent une approbation, sauf ceux whitelistés avec `AlwaysAccept`.

**Quand choisir le mode libre :**
- Usage opérateur quotidien : questions, recherches, lecture de fichiers
- Pas de logique Python personnalisée nécessaire
- Démarrage plus rapide (pas d'initialisation PyO3)
- Moins de ressources (pas de processus Python)

---

## Chat Agent — boucle ORIA complète

En mode `agent`, le message est délégué à un agent Python via `AIPBridge`. ORIA orchestre la boucle ReAct complète : le LLM raisonne, appelle des outils, itère jusqu'au résultat — et les tokens sont streamés vers la session au fil de l'exécution.

```
Message utilisateur
  │
  ▼
ChatSessionManager
  │
  ▼
AIPBridge.run(agent_name, message, context)
  │
  ├── agent.run(task, ctx)
  │       │
  │       ├── LLM raisonne → appel outil
  │       ├── outil exécuté → résultat → LLM
  │       ├── LLM raisonne → appel outil
  │       └── LLM génère réponse finale
  │
  ├── tokens streamés → SSE
  │
  ▼
ChatResponseCompleted → enregistré en SQLite
```

**Quand choisir le mode agent :**
- L'agent a sa propre logique Python (`run()` non triviale)
- L'agent utilise des outils personnalisés non disponibles en natif
- Vous voulez réutiliser un Worker Agent existant dans un contexte conversationnel

---

## Approbations HITL inline

Le Chat utilise un modèle d'approbation différent des tâches HITL (chapitre 10). Pas de suspension de tâche, pas de `input_required`. L'approbation est inline : la session reste ouverte, la génération est pausée en attente de la décision.

### Les trois décisions

```bash
# Accept — approuver une fois (prochain appel demandera à nouveau)
curl -X POST http://localhost:7771/api/v1/sessions/cs-a1b2c3/authorize \
  -H "Content-Type: application/json" \
  -d '{"tool_name": "file_io", "decision": "accept"}'

# Refuse — injecter un refus et continuer sans l'outil
curl -X POST http://localhost:7771/api/v1/sessions/cs-a1b2c3/authorize \
  -H "Content-Type: application/json" \
  -d '{"tool_name": "file_io", "decision": "refuse"}'

# AlwaysAccept — whitelist pour toute la session
curl -X POST http://localhost:7771/api/v1/sessions/cs-a1b2c3/authorize \
  -H "Content-Type: application/json" \
  -d '{"tool_name": "file_io", "decision": "always_accept"}'
```

| Décision | Effet | Persisté |
|---|---|---|
| `accept` | Outil exécuté une fois | Non |
| `refuse` | Refus injecté dans le contexte, le LLM continue | Non |
| `always_accept` | Outil whitelisté pour toute la session | Oui (SQLite) |

### Timeout automatique

Si l'opérateur ne répond pas dans les 5 minutes, l'approbation est refusée automatiquement (`ChatApprovalTimeout`) et le LLM reçoit un message de refus. La session continue — elle n'est pas bloquée indéfiniment.

---

## Intégration A2A — Worker Agents comme outils

En mode `libre`, les Worker Agents actifs qui déclarent `supports_a2a: True` deviennent automatiquement des **outils virtuels** dans la session chat. Leur `skill_id` est préfixé `a2a:` :

```
Outils disponibles dans la session :
  file_io          (natif)
  shell            (natif)
  web_search       (natif)
  a2a:analyze-csv  (Worker csv-data-worker)
  a2a:read-pdf     (Worker pdf-worker)
  a2a:review-code  (Worker code-worker)
```

Le LLM peut appeler `a2a:analyze-csv` comme n'importe quel outil natif. Le runtime route l'appel au Worker correspondant et injecte le résultat dans le contexte de la session.

**Différence avec les outils natifs :** les outils `a2a:` sont **auto-approuvés** en chat — ils ne déclenchent pas de `ChatApprovalRequired`. Les garde-fous internes du Worker s'appliquent (step budget, guardrails) — c'est lui qui contrôle ce qu'il accepte d'exécuter.

---

## Résumé : choisir son mode

| Critère | Chat Libre | Chat Agent |
|---|---|---|
| Logique Python personnalisée | Non | Oui |
| Outils natifs | Oui | Oui (via manifest) |
| Worker Agents A2A | Oui (auto) | Selon agent |
| HITL inline | Oui | Oui |
| Consommation ressources | Faible | Normale |
| Démarrage | Immédiat | PyO3 init |
| Cas d'usage | Opérateur quotidien | Agent spécialisé |
