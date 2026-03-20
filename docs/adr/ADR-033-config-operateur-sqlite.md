# ADR-033 — Config opérateur SQLite : séparation structurel (TOML) / opérationnel (SQLite)

**Date :** 2026-03-20
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 17

---

## Contexte

`apollia.toml` mélange deux types de configuration aux cycles de vie et publics différents :

1. **Config structurelle** — ports, chemins, feature flags, backends LLM. Change au déploiement, éditée par un développeur. Cycle de vie lent (jours/semaines).
2. **Config opérationnelle** — triggers, pipelines, canaux de notification. Change en opération courante, éditée par un opérateur (potentiellement non-développeur). Cycle de vie rapide (heures/jours).

Trois problèmes concrets découlent de ce mélange :

- **Complexité opérateur** : un non-développeur ne peut pas configurer un trigger sans éditer un fichier TOML et comprendre sa syntaxe (indentation, guillemets, tableaux `[[]]`).
- **Hot-reload fragile** : le seul hot-reload existant (`POST /api/v1/triggers/reload`) re-parse le TOML depuis le disque — sujet aux erreurs de syntaxe, sans validation interactive.
- **Separation of concerns floue** : le fichier unique `apollia.toml` est source de vérité pour des éléments que l'app desktop devrait pouvoir modifier (ADR-029 l'avait explicitement rendu lecture seule pour éviter la corruption TOML).

**Contraintes :**
- Principe #1 (Local-first) : SQLite local, zéro cloud
- Principe #2 (Zéro dépendance externe) : rusqlite bundled, déjà dans le workspace
- Principe #4 (Fail fast) : validation au write time, pas au boot/fire time
- ADR-002 (SQLite seul moteur de persistance) : cohérent
- ADR-029 (Settings lecture seule) : reste valide pour le TOML structurel

## Décision

Nous séparons la configuration en deux couches :

1. **`apollia.toml` = config structurelle uniquement** — runtime, memory, tools, budget, llm, agents. Lecture seule dans l'app desktop (ADR-029 inchangé). Nécessite un redémarrage.

2. **SQLite = config opérationnelle** — triggers (`triggers.db`), pipelines (`pipelines.db`), notifications (`notifications.db`). CRUD depuis l'API REST et l'app desktop. Application immédiate via reload acteur.

Le pattern de modification dynamique est **Option A : API handler → SQLite → Handle.reload()** :

```
Opérateur (UI/CLI)
    │
    ▼
POST /api/v1/triggers (axum handler)
    ├─ 1. Valider le payload (types Rust + règles métier)
    ├─ 2. Écrire dans SQLite (TriggerDefinitionRepository)
    ├─ 3. trigger_engine.reload() → relit toutes les définitions depuis SQLite
    └─ 4. Retourner 201/200
```

Pas de watch, pas de polling, pas de cache invalidation. L'API handler est le seul point d'entrée.

**Sous-décisions :**

| Question | Décision | Justification |
|---|---|---|
| Pattern de notification acteurs | Option A — handler → SQLite → Handle.reload() | Plus simple que EventBus (Option B) ou watch file (Option C) |
| Granularité API | CRUD par domaine (triggers, pipelines, notifications) | Pas de god-endpoint, cohérent avec les routes existantes |
| Organisation DBs | Une DB par sous-système | Déjà le cas (triggers.db, pipelines.db), cohérent avec l'architecture |
| Backward compat TOML | Non requise | Pas d'utilisateurs en production |
| Repositories dans AppState | `Arc<Mutex<Repository>>` | rusqlite Connection n'est pas Sync, mutations rares (opérateur humain) |
| Validation métier | Déplacée dans les crates domaine | apollia-triggers, apollia-pipelines, apollia-notifications — pas dans apollia-cli |

## Alternatives considérées

### Option A — TOML reste source de vérité, hot-reload amélioré (rejetée)

**Pour :** Pas de migration, un seul format de config, simplicité conceptuelle.
**Contre :** Ne résout pas le problème opérateur — éditer du TOML reste requis. Le hot-reload TOML ne peut pas faire de validation interactive (l'erreur est découverte après l'écriture). L'app desktop resterait lecture seule pour tout.

### Option B — EventBus pour notifier les acteurs après mutation (rejetée)

**Pour :** Découplage total — le handler ne connaît pas les acteurs, il émet un event et chaque acteur réagit.
**Contre :** Complexité accrue : il faut un event `ConfigChanged(domain)` + subscriber dans chaque engine + gestion de l'ordre. Le handler ne sait pas si le reload a réussi. Pas de feedback synchrone. Pour des mutations rares (opérateur humain), le couplage direct `Handle.reload()` est plus simple et plus prédictible.

### Option C — Watch file SQLite (rejetée)

**Pour :** Découplage encore plus fort — les acteurs détectent les changements automatiquement.
**Contre :** `notify` sur un fichier SQLite est fragile (WAL, journaux). Polling requis. Latence imprévisible. Complexité sans bénéfice réel quand le handler peut appeler reload() directement.

### Option retenue — SQLite + Handle.reload() synchrone (Option A)

**Pour :** Simple, prédictible, synchrone. Le handler sait si le reload a réussi. Pas de watch, pas de polling. Cohérent avec le modèle acteur (le Handle est l'interface vers l'acteur). Le pattern est déjà utilisé pour `POST /api/v1/triggers/reload`.
**Compromis acceptés :** Couplage direct handler → engine handle. Si un jour on ajoute 10 engines, il faudra appeler 10 reload(). Acceptable pour les 3 engines actuels.

## Conséquences

**Positives :**
- Un non-développeur peut configurer triggers/pipelines/notifications depuis l'app desktop
- Validation interactive : l'UI affiche les erreurs avant soumission, le serveur renvoie 422 avec message
- Le hot-reload TOML fragile est remplacé par un reload fiable depuis SQLite
- ADR-029 reste intact : le TOML structurel reste lecture seule
- Cohérent avec ADR-032 (agent install persistence) : même pattern SQLite + auto-reload

**Négatives / Compromis :**
- `apollia.toml` perd ses sections triggers/pipelines/notifications — un ancien fichier nécessite un warning au boot
- `Arc<Mutex<>>` pour les repositories dans AppState — pas un acteur Tokio pur, mais justifié car rusqlite Connection n'est pas Sync et les mutations sont rares
- Les données opérationnelles existantes dans un ancien TOML sont perdues (pas de migration automatique) — acceptable car pas d'utilisateurs en production
- La validation métier doit être dupliquée côté client (JavaScript DAG cycle detection) pour le feedback live

**Neutres / À surveiller :**
- Si un 4ème sous-système opérationnel apparaît, évaluer si le pattern est toujours tenable
- Les tests doivent couvrir le boot avec DB vide (cas nominal première installation)
- Le `POST /api/v1/triggers/reload` existant change de sémantique : il relit SQLite, plus le TOML

## Principes architecturaux impactés

- Principe #1 — Local-first : **Renforcé** — SQLite local, CRUD sans cloud
- Principe #2 — Zéro dépendance externe : **Respecté** — rusqlite bundled déjà dans le workspace
- Principe #4 — Fail fast : **Renforcé** — validation au write time avec feedback 422
- Principe #5 — Un acteur, une responsabilité : **Respecté** — chaque engine gère son reload, les repositories sont passifs
- Principe #8 — CLI humaine, API machine : **Renforcé** — l'API CRUD est consommée par la CLI et l'app desktop

## Liens

- Stories associées : STORY-184 → STORY-197 (Sprint 17 complet)
- ADR-002 — SQLite comme seul moteur de persistance (cohérent)
- ADR-021 — Triggers TOML-only (partiellement remplacé : les triggers migrent en SQLite, l'authentification HMAC-SHA256 webhooks reste)
- ADR-029 — Settings lecture seule (reste valide pour le TOML structurel)
- ADR-032 — Agent install persistence (même pattern SQLite + reload)
