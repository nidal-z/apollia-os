# ADR-033 - Config opérateur SQLite, authentification HMAC-SHA256 webhooks, hot reload sans restart

**Date :** 2026-03-08 (triggers) / 2026-03-20 (config SQLite)
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 9 (triggers) → 17 (config opérationnelle SQLite)

---

## Contexte

### Séparation config structurelle / opérationnelle (Sprint 17)

`apollia.toml` mélange deux types de configuration aux cycles de vie et publics différents :

1. **Config structurelle** - ports, chemins, feature flags, backends LLM. Change au déploiement, éditée par un développeur. Cycle de vie lent (jours/semaines).
2. **Config opérationnelle** - triggers, pipelines, canaux de notification. Change en opération courante, éditée par un opérateur. Cycle de vie rapide (heures/jours).

Trois problèmes découlent de ce mélange : complexité opérateur (édition TOML requise), hot-reload fragile (re-parse du TOML sans validation interactive), séparation des concerns floue (ADR-029 avait rendu les settings lecture seule pour éviter la corruption TOML).

### Triggers : authentification et hot reload (Sprint 9)

Le runtime expose un endpoint `POST /webhooks/{id}` accessible depuis l'extérieur. Sans authentification, n'importe quel process peut déclencher des agents arbitrairement. Par ailleurs, les schedules/secrets changent en opération - un restart complet implique un downtime de tous les agents en cours d'exécution.

**Contraintes communes :**
- Principe #1 (Local-first) : zéro appel réseau pour la config
- Principe #4 (Fail fast) : toute erreur détectable avant le premier fire est détectée au démarrage
- Principe #5 (Un acteur, une responsabilité) : chaque engine gère son propre reload

---

## Décisions

### 1 - Config opérationnelle en SQLite, config structurelle en TOML

`apollia.toml` = config structurelle uniquement (runtime, memory, tools, budget, llm, agents). Lecture seule dans l'app desktop (ADR-029 inchangé). Nécessite un redémarrage.

SQLite = config opérationnelle (`triggers.db`, `notifications.db`). CRUD depuis l'API REST et l'app desktop. Application immédiate via reload acteur.

Le pattern de modification dynamique est **API handler → SQLite → Handle.reload()** :

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

| Question | Décision | Justification |
|---|---|---|
| Pattern de notification acteurs | Option A - handler → SQLite → Handle.reload() | Plus simple que EventBus ou watch file |
| Granularité API | CRUD par domaine (triggers, notifications) | Cohérent avec les routes existantes |
| Organisation DBs | Une DB par sous-système | Déjà le cas, cohérent avec l'architecture |
| Repositories dans AppState | `Arc<Mutex<Repository>>` | rusqlite Connection n'est pas Sync, mutations rares |
| Validation métier | Dans les crates domaine | apollia-triggers, apollia-notifications |

### 2 - HMAC-SHA256 avec comparaison constante-time pour les webhooks

HMAC-SHA256 est le seul mécanisme d'authentification des webhooks. Le secret est déclaré dans `apollia.toml` et n'est jamais exposé dans les logs ou les réponses HTTP. La comparaison de la signature utilise `constant_time_eq` pour éliminer les timing attacks.

Le format suit le standard GitHub Webhooks : `X-Apollia-Signature: sha256=<hex>`.

Ordre de vérification strict : `503` (TriggerEngine indisponible) → `404` (trigger inconnu) → `401` (signature absente ou invalide) → `200`. Le `404` est retourné avant le `401` pour ne pas confirmer l'existence d'un trigger sans authentification.

### 3 - Hot reload par abort+respawn des JoinHandle avec timeout 2s

`TriggerEngineHandle::reload(new_definitions)` :
1. Donne 2 secondes à chaque `JoinHandle<()>` actif pour se terminer proprement (`tokio::time::timeout(2s, handle).await`) avant un drop forcé.
2. Remplace les définitions en mémoire sans toucher aux compteurs SQLite (`fire_count`, `last_fired_at`).
3. Respawn uniquement les sources dont `enabled = true`.
4. Émet `TriggersReloaded { count }` sur l'EventBus.

En cas d'erreur au reload, les triggers actuels continuent de fonctionner - le runtime répond `422 Unprocessable Entity`.

---

## Alternatives considérées

### Config structurelle : TOML reste source de vérité, hot-reload amélioré (rejetée)

Ne résout pas le problème opérateur (éditer du TOML reste requis). Le hot-reload TOML ne peut pas faire de validation interactive. L'app desktop resterait lecture seule pour tout.

### Config structurelle : EventBus pour notifier les acteurs (rejetée)

Complexité accrue : event `ConfigChanged(domain)` + subscriber dans chaque engine + gestion de l'ordre. Le handler ne sait pas si le reload a réussi. Pour des mutations rares (opérateur humain), le couplage direct `Handle.reload()` est plus simple et plus prédictible.

### Webhooks : Token Bearer statique (rejetée)

Un token Bearer n'authentifie pas le body : un intermédiaire peut rejouer la requête avec un body différent. HMAC-SHA256 lie cryptographiquement le secret au body. Le format `sha256=<hex>` est le standard de facto (GitHub, Stripe, GitLab).

### Triggers : Hot reload via SIGHUP (rejetée)

Ne fonctionne pas sur Windows. Pas de retour immédiat sur le résultat. Rompt le pattern ADR-008 (CLI humaine explicite). `apollia-os trigger reload` est plus sûr qu'un signal silencieux.

---

## Conséquences

**Positives :**
- Un non-développeur peut configurer triggers/pipelines/notifications depuis l'app desktop
- Validation interactive : l'UI affiche les erreurs avant soumission, le serveur renvoie 422 avec message
- Les webhooks sont immunisés contre le body tampering et les timing attacks sans dépendance cloud
- Les compteurs d'historique (`fire_count`, `last_fired_at`) sont préservés au reload
- ADR-029 reste intact : le TOML structurel reste lecture seule

**Négatives / Compromis :**
- `apollia.toml` perd ses sections triggers/pipelines/notifications (warning au boot si ancien fichier)
- `Arc<Mutex<>>` pour les repositories dans AppState - pas un acteur Tokio pur, justifié par rusqlite
- Full-replace au reload : une source inchangée est quand même stoppée et respawnée (impact minimal pour Cron/Interval)
- La validation métier doit être dupliquée côté client JavaScript pour le feedback live

**Neutres / À surveiller :**
- `POST /api/v1/triggers/reload` change de sémantique : relit SQLite, plus le TOML
- Si un 4ème sous-système opérationnel apparaît, évaluer si le pattern est toujours tenable

---

## Principes architecturaux impactés

- Principe #1 - Local-first : **Renforcé** - SQLite local, CRUD sans cloud
- Principe #2 - Zéro dépendance externe : **Respecté** - rusqlite bundled, `constant_time_eq` in workspace
- Principe #4 - Fail fast : **Renforcé** - validation au write time avec feedback 422
- Principe #5 - Un acteur, une responsabilité : **Respecté** - chaque engine gère son reload, les repositories sont passifs
- Principe #8 - CLI humaine, API machine : **Renforcé** - `apollia-os trigger reload` est explicite

---

## Liens

- Stories : STORY-065 → STORY-078 (Sprint 9) + STORY-184 → STORY-197 (Sprint 17)
- ADR-002 - SQLite comme seul moteur de persistance (cohérent)
- ADR-029 - Settings lecture seule (reste valide pour le TOML structurel)
- ADR-032 - Agent install persistence (même pattern SQLite + reload)
