# Guide Onboarding — Apollia OS

> L'onboarding Apollia OS est un **agent conversationnel**, pas un wizard.
> Pas d'étapes numérotées, pas de formulaires fixes.
> L'agent s'adapte à l'utilisateur et persiste chaque information en temps réel.

**Décision architecturale** : [ADR-040 — Onboarding comme agent conversationnel](../adr/ADR-040-onboarding-conversational-agent.md)

---

## Vue d'ensemble

Au premier lancement d'Apollia OS, le runtime détecte l'absence de mémoire utilisateur et émet un événement `OnboardingRequired`. L'interface (desktop ou CLI) propose alors une conversation guidée par un agent dédié — `onboarding-agent` — qui fait connaissance avec l'utilisateur de manière naturelle.

Chaque information collectée est persistée **immédiatement** dans la mémoire utilisateur (`UserMemoryRepository`). L'utilisateur peut quitter à tout moment sans perdre les données déjà collectées. Il peut relancer l'onboarding plus tard, intégralement ou sur un domaine spécifique.

### Flux principal

1. Premier lancement — le runtime détecte une mémoire utilisateur vide
2. L'événement `OnboardingRequired` est émis via l'EventBus
3. Le frontend affiche un écran d'accueil avec deux options : "Configurer" ou "Plus tard"
4. "Configurer" ouvre une session de chat avec `onboarding-agent`
5. Conversation naturelle couvrant 5 domaines
6. Informations persistées en temps réel dans `UserMemoryRepository`
7. Progression affichée via une barre de topics dans l'interface desktop

### Flux alternatif ("Plus tard")

1. L'utilisateur clique "Plus tard" — l'onboarding est marqué comme "skipped"
2. Le dashboard affiche un badge rappelant que l'onboarding est disponible
3. L'utilisateur peut déclencher l'onboarding à tout moment via la CLI : `apollia-os onboard`

---

## Les 5 domaines

L'agent explore 5 domaines au fil de la conversation. L'**ordre n'est pas fixe** — l'agent s'adapte au flux de la discussion et décide quand et comment aborder chaque domaine.

### 1. Identité

Comprendre qui est l'utilisateur — nom, rôle, langues parlées, niveau d'expertise.

| Clé mémoire | Type | Description |
|---|---|---|
| `user.name` | `string` | Prénom ou nom de l'utilisateur |
| `user.role` | `string` | Rôle au quotidien (dev, CTO, data scientist...) |
| `user.languages` | `list[string]` | Langues parlées |
| `user.expertise_level` | `string` | Niveau d'expertise technique (débutant, intermédiaire, senior) |

### 2. Préférences

Adapter le comportement d'Apollia OS au style de l'utilisateur.

| Clé mémoire | Type | Description |
|---|---|---|
| `user.preferences.verbosity` | `string` | Détaillé ou concis |
| `user.preferences.format` | `string` | Format préféré (markdown, texte brut...) |
| `user.preferences.language` | `string` | Langue d'interaction préférée (fr, en) |

### 3. Outils

Connaître l'écosystème de développement de l'utilisateur.

| Clé mémoire | Type | Description |
|---|---|---|
| `user.tools.ide` | `string` | Éditeur de code principal |
| `user.tools.terminal` | `string` | Terminal utilisé |
| `user.tools.cli_favorites` | `list[string]` | Outils CLI favoris |
| `user.tools.package_manager` | `string` | Gestionnaire de paquets (pip, npm, cargo...) |

### 4. Domaine

Comprendre les projets et contraintes techniques.

| Clé mémoire | Type | Description |
|---|---|---|
| `user.domain.type` | `string` | Type de projet (SaaS, embarqué, mobile...) |
| `user.domain.stack` | `list[string]` | Stack technique principale |
| `user.domain.constraints` | `list[string]` | Contraintes (compliance, offline, perf...) |

### 5. Agents & Automatisation

Identifier les workflows à automatiser et les attentes vis-à-vis des agents IA.

| Clé mémoire | Type | Description |
|---|---|---|
| `user.agents.workflows` | `list[string]` | Tâches candidates à l'automatisation |
| `user.agents.pain_points` | `list[string]` | Points de douleur actuels |
| `user.agents.expectations` | `string` | Attentes vis-à-vis d'un agent IA |

---

## Schéma de clés mémoire

Toutes les clés utilisées par l'onboarding sont préfixées par `user.` et stockées dans le namespace `__user__` du `SemanticMemory`.

```
user.
├── name
├── role
├── languages
├── expertise_level
├── preferences.
│   ├── verbosity
│   ├── format
│   └── language
├── tools.
│   ├── ide
│   ├── terminal
│   ├── cli_favorites
│   └── package_manager
├── domain.
│   ├── type
│   ├── stack
│   └── constraints
└── agents.
    ├── workflows
    ├── pain_points
    └── expectations
```

### Scores de confiance

Chaque entrée mémoire est associée à un score de confiance (0.0 à 1.0) qui reflète la fiabilité de l'information :

| Score | Signification | Exemple |
|---|---|---|
| `0.95` | Validée par l'utilisateur | L'utilisateur confirme une information déduite |
| `0.9` | Déclarée explicitement | "Je m'appelle Nidal" |
| `0.5` | Déduite du contexte | L'utilisateur écrit en français → probablement francophone |

L'agent utilise les tags `[REMEMBER key=value]` pour les déclarations explicites (confiance 0.9) et `[INFER key=value]` pour les déductions (confiance 0.5). Ces tags sont extraits du texte LLM puis retirés avant affichage à l'utilisateur.

---

## Utilisation CLI

### Onboarding complet

```bash
$ apollia-os onboard
```

Soumet une tâche à `onboarding-agent` et démarre la conversation complète. Le runtime doit être lancé au préalable.

### Onboarding partiel (un domaine)

```bash
$ apollia-os onboard --topic preferences
$ apollia-os onboard --topic tools
```

Relance l'onboarding sur un domaine spécifique. Utile pour mettre à jour des informations existantes ou compléter un onboarding interrompu.

**Topics valides** : `identity`, `preferences`, `tools`, `domain`, `agents`.

### Flag `--json`

```bash
$ apollia-os onboard --json
$ apollia-os onboard --topic tools --json
```

Sortie structurée JSON pour intégration machine.

---

## Utilisation Desktop

### Écran d'accueil

Au premier lancement, l'application affiche un écran de bienvenue avec deux options :

- **"Configurer"** — Ouvre une conversation d'onboarding en plein écran avec une barre de progression par domaine
- **"Plus tard"** — Ferme l'écran et affiche un badge de rappel dans le dashboard

### Barre de progression

Pendant la conversation, un indicateur visuel `TopicProgressBar` montre l'avancement par domaine :

- **Gris** — Domaine pas encore abordé
- **Bleu (pulsant)** — Domaine en cours d'exploration
- **Violet (coche)** — Domaine couvert

Le statut est rafraîchi toutes les 4 secondes via le Tauri command `get_onboarding_status`.

### Re-déclenchement

Depuis **Settings**, l'utilisateur peut relancer l'onboarding complet ou cibler un domaine spécifique.

---

## Enrichissement passif

Après chaque session de chat (minimum 4 messages), un extracteur LLM analyse automatiquement la conversation pour identifier de nouvelles informations sur l'utilisateur. Ces informations sont persistées avec la source `chat_inference` et un score de confiance de 0.5.

L'extraction est fire-and-forget (timeout 30s, cooldown 1h entre extractions) et ne bloque jamais la fermeture de session.

Les informations extraites passivement ne remplacent jamais celles déclarées explicitement pendant l'onboarding (score de confiance supérieur).

---

## Gestion des mémoires

L'utilisateur peut gérer ses mémoires à tout moment :

- **Via l'API REST** : `GET /api/v1/user/memory`, `DELETE /api/v1/user/memory/:key`
- **Via le desktop** : Settings > Mes Mémoires — valider, corriger, supprimer chaque entrée
- **Via la CLI** : `apollia-os memory inspect` pour explorer le contenu mémoire

La validation d'une entrée augmente son score de confiance à 0.95. La suppression est immédiate et définitive.

---

## Architecture technique

### Agent

**Fichier** : `agents/onboarding-agent.py`

`OnboardingAgent` hérite de `ConversationalAgent` (SDK Sprint 21). Il utilise le même contrat que tout autre agent — `manifest()` + `run()` async.

Le system prompt est bilingue (FR/EN). La langue est détectée automatiquement sur le premier message de l'utilisateur via une heuristique lexicale.

### Événements runtime

| Événement | Émis par | Signification |
|---|---|---|
| `OnboardingRequired` | Supervisor (boot) | Mémoire utilisateur vide détectée |
| `OnboardingStarted { session_id, mode, topic }` | Tauri / CLI | Session d'onboarding démarrée |

### Commandes Tauri IPC

| Commande | Description |
|---|---|
| `get_onboarding_status()` | Retourne `OnboardingStatus` (completed, topics_covered, completion_pct, skipped) |
| `trigger_onboarding(topic?)` | Crée une session chat agent-backed, retourne `TriggerResult` |
| `dismiss_onboarding()` | Marque l'onboarding comme "skipped" |

---

## Diagrammes

- [seq-onboarding-flow.puml](https://github.com/nidal-z/apollia-os/blob/main/docs/diagrams/seq-onboarding-flow.puml) — Flux d'onboarding complet (premier lancement + re-déclenchement)

---

## Liens

- [ADR-040 — Onboarding comme agent conversationnel](../adr/ADR-040-onboarding-conversational-agent.md)
- [Brique — Mémoire Utilisateur Globale](Briques-User-Memory.md)
- [Brique — CLI](Briques-CLI.md)
- [Guide RuntimeContext agents Python](Agents-RuntimeContext-Guide.md)
