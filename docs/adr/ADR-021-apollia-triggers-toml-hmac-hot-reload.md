# ADR-021 — apollia-triggers : configuration TOML-only, authentification HMAC-SHA256 webhooks, hot reload sans restart

**Date :** 2026-03-08
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 9

---

## Contexte

Sprint 9 introduit `apollia-triggers`, le moteur de déclenchement automatique d'agents.
Trois décisions structurantes doivent être arrêtées avant l'implémentation, car elles
engagent des interfaces publiques difficiles à inverser :

1. **Comment l'opérateur configure-t-il les triggers ?**
   Le runtime doit lire des règles de déclenchement (cron, file watch, webhook) depuis
   une source de configuration. Plusieurs formats et emplacements sont possibles.

2. **Comment authentifie-t-on les appels webhook entrants ?**
   Le runtime expose un endpoint `POST /webhooks/{id}` accessible depuis des systèmes
   externes. Sans authentification, n'importe quel process local ou réseau peut déclencher
   des agents arbitrairement.

3. **Comment met-on à jour les triggers sans interrompre le runtime ?**
   Les règles de déclenchement (schedules, secrets, paths) changent en opération. Un
   restart complet du runtime implique un downtime de tous les agents en cours d'exécution
   — inacceptable pour un runtime de production local.

Les trois contraintes non-négociables qui encadrent ces choix :

- **Principe #1 — Local-first** : toute configuration et authentification doit fonctionner
  hors ligne, sans service tiers.
- **Principe #4 — Fail fast** : toute erreur de configuration détectable avant le premier
  fire (schedule cron invalide, secret vide, path absent) doit être détectée au démarrage.
- **Principe #5 — Un acteur, une responsabilité** : `TriggerEngine` est un acteur Tokio
  autonome — ses sources sont des `JoinHandle<()>` indépendants, abortables et remplaçables.

---

## Décision

### Décision 1 — Configuration TOML-only via `[[triggers]]` dans `apollia.toml`

Nous utilisons exclusivement la section `[[triggers]]` du fichier `apollia.toml` existant
pour déclarer les triggers. Aucune base de données, aucun endpoint REST de création,
aucun fichier de config séparé.

La validation sémantique complète (schedule cron via `cron::Schedule::from_str`, secret
webhook non vide, path FileWatch résolvable) est effectuée dans `ApolliaConfig::load()`
au démarrage — pas au premier fire. Un trigger avec `enabled = false` est parsé mais
sa source n'est pas validée.

### Décision 2 — HMAC-SHA256 avec comparaison constante-time pour les webhooks

Nous utilisons HMAC-SHA256 comme seul mécanisme d'authentification des webhooks.
Le secret est déclaré dans `apollia.toml` et n'est jamais exposé dans les logs ou les
réponses HTTP. La comparaison de la signature est effectuée via `constant_time_eq` pour
éliminer les timing attacks.

Le format de signature suit le standard GitHub Webhooks : `X-Apollia-Signature: sha256=<hex>`.
L'ordre de vérification est strict : `503` (TriggerEngine indisponible) → `404` (trigger
inconnu) → `401` (signature absente ou invalide) → `200`. Le `404` est retourné avant
le `401` pour ne pas confirmer l'existence d'un trigger sans authentification.

### Décision 3 — Hot reload par abort+respawn des JoinHandle avec timeout 2s

Nous implémentons le hot reload via `TriggerEngineHandle::reload(new_definitions)` qui :
1. Donne 2 secondes à chaque `JoinHandle<()>` actif pour se terminer proprement
   (`tokio::time::timeout(2s, handle).await`) avant un drop forcé (abort implicite Tokio).
2. Remplace les définitions en mémoire sans toucher aux compteurs SQLite (`fire_count`,
   `last_fired_at` dans `trigger_state`).
3. Respawn uniquement les sources dont `enabled = true`.
4. Émet `TriggersReloaded { count }` sur l'EventBus.

En cas d'erreur de parsing TOML au reload, les triggers actuels continuent de fonctionner
sans interruption — le runtime répond `422 Unprocessable Entity` avec le détail de l'erreur.

---

## Alternatives considérées

### Option A — Configuration via API REST + stockage SQLite (rejetée)

Permettre la création de triggers via `POST /api/v1/triggers` avec persistance en base.
L'`apollia.toml` resterait la config initiale, mais les triggers pourraient être modifiés
à chaud sans fichier.

**Pour :**
- Interface cohérente avec les autres ressources (agents, tasks).
- Pas besoin de recharger un fichier pour modifier un trigger.
- Permets des modifications programmatiques (scripts, CI).

**Contre :**
- Viole le principe "single source of truth" : la config existerait dans deux endroits
  (TOML initial + base de données). Un redémarrage après modification directe SQL serait
  incohérent.
- Complexité accrue : migrations supplémentaires, endpoints CRUD complets, résolution
  des conflits TOML vs base.
- Contredit le pattern établi pour les LLM backends (`[[llm.backends]]` dans TOML) et
  les agents (`apollia.toml` comme référence déclarative).
- Pas de validation "fail fast" naturelle : les erreurs apparaissent lors de la création
  via API, pas au démarrage.

### Option B — Fichier de config séparé `triggers.toml` (rejetée)

Isoler la configuration des triggers dans un fichier `~/.apollia/triggers.toml` distinct
de `apollia.toml`, permettant un `inotify/kqueue` auto-reload.

**Pour :**
- Séparation claire des responsabilités au niveau fichier.
- Auto-reload possible via `notify` sans commande CLI explicite.
- `apollia.toml` reste plus court.

**Contre :**
- Fragmentation de la configuration : l'opérateur doit gérer deux fichiers pour un seul
  runtime.
- Un watch automatique sur le fichier de config violerait le Principe #4 (une modification
  partielle d'un fichier en cours d'écriture pourrait déclencher un reload avec une config
  invalide).
- Rupture de cohérence avec l'approche `apollia.toml` unifiée établie aux sprints 6-8
  (`[[llm.backends]]`, `[runtime]`).
- Hot reload sans commande explicite = comportement surprenant (cf. ADR-008 : CLI humaine
  explicite).

### Option C — Token Bearer statique pour les webhooks (rejetée)

Utiliser un `Authorization: Bearer <token>` statique au lieu de HMAC-SHA256.

**Pour :**
- Implémentation triviale (comparaison de string).
- Standard HTTP bien connu.

**Contre :**
- Un token Bearer n'authentifie pas le *body* : un intermédiaire peut rejouer la requête
  avec un body différent (pas de protection contre la manipulation du payload).
- HMAC-SHA256 lie cryptographiquement le secret au body — une signature valide prouve que
  l'émetteur connaît le secret *et* a généré ce body exact.
- Format `sha256=<hex>` est le standard de facto des webhooks GitHub, Stripe, GitLab —
  les utilisateurs savent déjà comment l'implémenter côté émetteur.
- Pas de protection contre les timing attacks avec une comparaison `==` sur string.

### Option D — Hot reload via SIGHUP sans endpoint REST (rejetée)

Déclencher le reload par le signal UNIX `SIGHUP` (convention `nginx`, `postgresql`),
sans commande CLI ni endpoint HTTP.

**Pour :**
- Convention UNIX respectée et connue des administrateurs système.
- Pas d'ajout d'endpoint REST.

**Contre :**
- Ne fonctionne pas sur Windows (hors scope actuel, mais prévu roadmap).
- Pas de retour immédiat sur le résultat du reload (succès ou erreur de parsing).
- Rompt le pattern ADR-008 : "CLI humaine" — `apollia-os trigger reload` est plus explicite
  et plus sûr qu'un signal silencieux.
- Pas de cohérence avec l'endpoint `POST /api/v1/shutdown` existant (STORY-037) qui suit
  le même pattern REST.

### Option retenue — TOML déclaratif + HMAC-SHA256 + reload REST explicite

**Pour :**
- **Cohérence** : même pattern que `[[llm.backends]]` — déclaratif, validé au démarrage.
- **Sécurité** : HMAC lie le secret au body, `constant_time_eq` élimine les timing attacks.
- **Explicite** : `apollia-os trigger reload` retourne un résultat clair, sans comportement
  surprenant de fond.
- **Fail fast** : toute erreur TOML est détectée avant le premier fire, pas à l'exécution.
- **Principe #5 respecté** : `JoinHandle` par source = un acteur par responsabilité,
  remplacement atomique au reload.

**Compromis acceptés :**
- Le hot reload est full-replace (pas de diff) : toutes les sources sont stoppées et
  respawnées, même celles qui n'ont pas changé.
- Le timeout 2s par source peut allonger un reload avec de nombreux triggers actifs
  (N triggers × 2s dans le pire cas — en pratique parallélisable).
- La validation `enabled = false` ne valide pas la source : une erreur dans un trigger
  désactivé n'est pas détectée au démarrage (compromis explicite).

---

## Conséquences

**Positives :**
- `apollia.toml` est la source de vérité unique pour la configuration du runtime complet :
  runtime, LLM backends, agents, et maintenant triggers.
- Les webhooks sont immunisés contre le body tampering et les timing attacks sans
  dépendance cloud.
- `apollia-os trigger reload` permet de mettre à jour schedules/secrets/paths en production
  sans downtime d'agents en cours d'exécution.
- Les compteurs d'historique (`fire_count`, `last_fired_at`) sont préservés au reload —
  la continuité opérationnelle est assurée.

**Négatives / Compromis :**
- Full-replace au reload : une source inchangée est quand même stoppée et respawnée.
  Impact minimal pour les sources Cron/Interval mais potentiellement perturbant pour
  `FileWatchTrigger` si un `create` arrive pendant les 2s de transition.
- La crate `apollia-triggers` dépend de `cron = "0.12"`, `notify = "6"`, `chrono = "0.4"` —
  trois nouvelles dépendances workspace.
- La route `POST /webhooks/:id` utilise le secret stocké en clair dans `apollia.toml`
  (chiffrement hors scope Sprint 9).

**Neutres / À surveiller :**
- Compatibilité `hmac = "0.12"` + `sha2 = "0.10"` déjà dans le workspace : les deux
  utilisent `digest = "0.10"` — vérifier avec `cargo tree -p apollia-runtime | grep digest`
  avant STORY-069.
- Comportement de `notify::Watcher` au drop après un `JoinHandle` timeout : `Drop`-safe
  sur macOS/Linux selon la doc, à confirmer sous charge.
- Limite de débit `FileWatchTrigger` si events inotify/kqueue s'accumulent (buffer
  `std::sync::mpsc::Receiver::recv_timeout(50ms)` documenté dans STORY-068).

---

## Principes architecturaux impactés

- **Principe #1 — Local-first** : configuration TOML locale, authentification HMAC sans
  service tiers, hot reload depuis le disque local. Aucune donnée ne quitte la machine.
- **Principe #4 — Fail fast** : `ApolliaConfig::load()` valide schedules, secrets et paths
  avant le démarrage des sources. Erreur TOML au reload → `422` sans interrompre les
  triggers actifs.
- **Principe #5 — Un acteur, une responsabilité** : chaque source de trigger est un
  `JoinHandle<()>` indépendant. `TriggerEngine` est l'acteur central qui coordonne, sans
  état partagé entre les sources.
- **Principe #8 — CLI humaine, API machine** : `apollia-os trigger reload` est explicite,
  retourne un résultat lisible, et suit le pattern noun-verb (ADR-008).

---

## Liens

- Stories associées : STORY-065 → STORY-078 (Sprint 9)
- STORY-069 : implémentation `POST /webhooks/{id}` + `verify_hmac()`
- STORY-071 : parsing et validation `[[triggers]]` dans `apollia.toml`
- STORY-073 : implémentation `TriggerEngineHandle::reload()` + commande `trigger reload`
- ADR précédent lié : ADR-002 — SQLite (même pattern : config déclarative, données
  séparées du code)
- ADR précédent lié : ADR-008 — Pattern noun-verb CLI (reload est une commande CLI
  explicite, pas un signal implicite)
- ADR précédent lié : ADR-018 — CLI bootstrap sans Supervisor (`POST /api/v1/shutdown`
  comme précédent du pattern REST pour les opérations système)
