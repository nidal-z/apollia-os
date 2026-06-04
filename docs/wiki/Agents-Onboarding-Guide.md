# Guide Onboarding - Apollia OS

> L'onboarding Apollia OS est un **agent conversationnel**, pas un wizard.
> Pas d'étapes numérotées, pas de formulaires fixes.
> L'agent s'adapte à l'utilisateur et persiste chaque information en temps réel.

**Décision architecturale** : [ADR-027 - Onboarding comme agent conversationnel](../adr/ADR-027-onboarding-agent.md)

---

## Vue d'ensemble

Au premier lancement d'Apollia OS, le runtime détecte l'absence de mémoire utilisateur et émet un événement `OnboardingRequired`. L'interface (desktop ou CLI) propose alors une conversation guidée par un agent dédié - `onboarding-agent` - qui fait connaissance avec l'utilisateur de manière naturelle.

Chaque information collectée est persistée **immédiatement** dans la mémoire utilisateur (`UserMemoryRepository`). L'utilisateur peut quitter à tout moment sans perdre les données déjà collectées. Il peut relancer l'onboarding plus tard, intégralement ou sur un domaine spécifique.

### Flux principal

1. Premier lancement - le runtime détecte une mémoire utilisateur vide
2. L'événement `OnboardingRequired` est émis via l'EventBus
3. Le frontend affiche un écran d'accueil avec deux options : "Configurer" ou "Plus tard"
4. "Configurer" ouvre une session de chat avec `onboarding-agent`
5. Conversation naturelle couvrant 5 domaines
6. Informations persistées en temps réel dans `UserMemoryRepository`
7. Progression affichée via une barre de topics dans l'interface desktop

### Flux alternatif ("Plus tard")

1. L'utilisateur clique "Plus tard" - l'onboarding est marqué comme "skipped"
2. Le dashboard affiche un badge rappelant que l'onboarding est disponible
3. L'utilisateur peut déclencher l'onboarding à tout moment via la CLI : `apollia-os onboard`

---

## Les 5 domaines

L'agent explore 5 domaines au fil de la conversation. L'**ordre n'est pas fixe** - l'agent s'adapte au flux de la discussion et décide quand et comment aborder chaque domaine.

### 1. Identité

Le profil utilisateur est organisé en **deux tiers** :

- **Tier 1** : 4 clés strictement obligatoires, collectées en ≤ 4 tours par l'agent onboarding au premier lancement. Le desktop ne se déverrouille qu'une fois ces 4 clés écrites.
- **Tier 2** : enrichissement progressif via le bouton **« Compléter mon profil »** dans `Paramètres → Profil`, ou via saisie manuelle dans les onglets de la page Profil.

### 1. Identité (Tier 1 + Tier 2)

| Clé mémoire | Type | Tier | Source(s) | Description |
|---|---|---|---|---|
| `user.name` | `string` | **Tier 1** | onboarding | Prénom ou alias |
| `user.role` | `string` | **Tier 1** | onboarding | Rôle au quotidien (CTO, comptable, secrétaire…) |
| `user.goals` | `string` | Tier 2 | Profil | Objectifs en 1-2 phrases |
| `user.domain.sector` | `string` | Tier 2 | Profil | Secteur d'activité (fintech, santé, RH…) |
| `user.domain.team_size` | `string` | Tier 2 | Profil | Taille d'équipe (solo / 2-5 / 6-20 / 20+) |

### 2. Supervision des agents (Tier 1 + Tier 2)

Pilote le niveau de validation et l'autonomie laissée aux agents.

| Clé mémoire | Type | Tier | Description |
|---|---|---|---|
| `user.agents.hitl` | `string` | **Tier 1** | `always` / `critical-only` / `never` |
| `user.agents.domains` | `string` | Tier 2 | Domaines où les agents agissent en autonomie (liste, séparés par virgules) |
| `user.agents.trigger` | `string` | Tier 2 | Mode de déclenchement par défaut : `manuel` / `planifie` / `evenementiel` |

### 3. Outils & contexte métier (Tier 2)

Onglet générique applicable à tous les profils (dev, métier, opérateur).

| Clé mémoire | Type | Description |
|---|---|---|
| `user.tools.daily` | `string` | Outils du quotidien, liste libre (Excel, Notion, Salesforce, VS Code…) |
| `user.tech.proficiency` | `string` | Aisance technique : `debutant` / `a-laise` / `expert` |
| `user.tools.integrations` | `string` | Intégrations Apollia activées (GitHub, Slack, Notion, Gmail) - séparées par virgules |

### 4. Contraintes (Tier 1 + Tier 2)

| Clé mémoire | Type | Tier | Description |
|---|---|---|---|
| `user.constraints.sovereignty` | `string` | **Tier 1** | `local-only` / `local-preferred` / `cloud-ok` |
| `user.constraints.compliance` | `string` | Tier 2 | Conformités requises (RGPD, HIPAA, SOC2…), séparées par virgules |

### 5. Préférences (Tier 2)

| Clé mémoire | Type | Description |
|---|---|---|
| `user.preferences.language` | `string` | Langue d'interaction (`fr` / `en`) |
| `user.preferences.llm` | `string` | Backend LLM préféré (`local` / `ollama` / `anthropic`…) |

### 6. Métadonnées onboarding (écrites par l'agent)

| Clé | Description |
|---|---|
| `onboarding.profile_type` | `operator` ou `builder`, inféré depuis `user.role` |
| `onboarding.version` | Version du flux d'onboarding (`2.2`) |
| `onboarding.suggested_agents` | Liste JSON d'agents recommandés |
| `onboarding.completed_at` | ISO 8601, signal de complétion Tier 1 (déverrouille le desktop) |
| `onboarding.tier2_completed_at` | ISO 8601, signal de complétion Tier 2 (optionnel) |

---

### Matrice des règles de permissions proposées

À la fin du Tier 1 (et après Tier 2 si compliance/integrations changent), l'agent dérive automatiquement des règles de permissions et les soumet une à une via des **cartes d'approbation** dans la conversation. Chaque carte appelle `permission_rule_add` (HITL-gated, `created_by="onboarding-agent"`).

| Critère | Règles proposées (action, tool, arg_prefix, scope) |
|---|---|
| `sovereignty=local-only` | `deny http_fetch https://` (global), `deny http_fetch http://` (global) |
| `sovereignty=local-preferred` | `deny http_fetch https://api.openai.com` (global), `deny http_fetch https://api.anthropic.com` (global) |
| `sovereignty=cloud-ok` | aucune règle réseau |
| `hitl=critical-only` ou `never` | `allow file_read` (global) + `allow shell_exec` sur `ls`/`cat`/`grep`/`pwd`/`head`/`tail` (global) |
| `hitl=always` | aucune règle d'allow (statu quo, friction maximale) |
| `tools.integrations` contient `GitHub` | `allow http_fetch https://api.github.com` (global) |
| `tools.integrations` contient `Slack` | `allow http_fetch https://slack.com/api/` (global) |
| `tools.integrations` contient `Notion` | `allow http_fetch https://api.notion.com` (global) |
| `tools.integrations` contient `Gmail` | `allow http_fetch https://gmail.googleapis.com` (global) |

L'idempotence est assurée par un `permission_rule_list(created_by="onboarding-agent")` préalable : si des règles existent déjà, l'agent ne re-propose rien (pour réinitialiser, l'utilisateur doit révoquer les règles existantes ou faire un reset onboarding).

---

## Schéma de clés mémoire

Toutes les clés utilisées par l'onboarding sont préfixées par `user.` et stockées dans le namespace `__user__` du `SemanticMemory`.

```
user.
├── name                          (Tier 1)
├── role                          (Tier 1)
├── goals                         (Tier 2)
├── domain.
│   ├── sector                    (Tier 2)
│   └── team_size                 (Tier 2)
├── tech.
│   └── proficiency               (Tier 2)
├── tools.
│   ├── daily                     (Tier 2)
│   └── integrations              (Tier 2)
├── agents.
│   ├── hitl                      (Tier 1)
│   ├── domains                   (Tier 2)
│   └── trigger                   (Tier 2)
├── constraints.
│   ├── sovereignty               (Tier 1)
│   └── compliance                (Tier 2)
└── preferences.
    ├── language                  (Tier 2)
    └── llm                       (Tier 2)

onboarding.                       (méta, écrites par l'agent)
├── profile_type
├── version
├── suggested_agents
├── completed_at                  (signal Tier 1)
└── tier2_completed_at            (signal Tier 2)
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

- **"Configurer"** - Ouvre une conversation d'onboarding en plein écran avec une barre de progression par domaine
- **"Plus tard"** - Ferme l'écran et affiche un badge de rappel dans le dashboard

### Barre de progression

Pendant la conversation, un indicateur visuel `TopicProgressBar` montre l'avancement par domaine :

- **Gris** - Domaine pas encore abordé
- **Bleu (pulsant)** - Domaine en cours d'exploration
- **Violet (coche)** - Domaine couvert

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
- **Via le desktop** : Settings > Mes Mémoires - valider, corriger, supprimer chaque entrée
- **Via la CLI** : `apollia-os memory inspect` pour explorer le contenu mémoire

La validation d'une entrée augmente son score de confiance à 0.95. La suppression est immédiate et définitive.

---

## Architecture technique

### Agent

**Fichier** : `agents/onboarding-agent.py`

`OnboardingAgent` hérite de `ConversationalAgent` (SDK). Il utilise le même contrat que tout autre agent - `manifest()` + `run()` async.

Le system prompt est bilingue (FR/EN). La langue est détectée automatiquement sur le premier message de l'utilisateur via une heuristique lexicale.

### Événements runtime

| Événement | Émis par | Signification |
|---|---|---|
| `OnboardingRequired` | Supervisor (boot) | Mémoire utilisateur vide détectée |
| `OnboardingStarted { session_id, mode, topic }` | Tauri / CLI | Session d'onboarding démarrée |

### Commandes Tauri IPC

| Commande | Description |
|---|---|
| `get_onboarding_status` | Retourne `OnboardingStatus` (completed, topics_covered, completion_pct, skipped) |
| `trigger_onboarding(topic?)` | Crée une session chat agent-backed, retourne `TriggerResult` |
| `dismiss_onboarding` | Marque l'onboarding comme "skipped" |

---

## Diagrammes

- [seq-onboarding-flow.puml](https://github.com/Apollia-OS/apollia-os/blob/main/docs/diagrams/seq-onboarding-flow.puml) - Flux d'onboarding complet (premier lancement + re-déclenchement)

---

## Liens

- [ADR-027 - Onboarding comme agent conversationnel](../adr/ADR-027-onboarding-agent.md)
- [Brique - Mémoire Utilisateur Globale](Briques-User-Memory.md)
- [Brique - CLI](Briques-CLI.md)
- [Guide RuntimeContext agents Python](Agents-RuntimeContext-Guide.md)
