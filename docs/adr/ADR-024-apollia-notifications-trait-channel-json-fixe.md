# ADR-024 — apollia-notifications : trait `NotificationChannel`, 3 canaux (desktop/SSE/webhook), payload JSON fixe Apollia

**Date :** 2026-03-09
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 11

---

## Contexte

Sprint 11 introduit `apollia-notifications`, une nouvelle crate qui abonne l'`EventBus` et dispatche les événements runtime vers des canaux de notification externes. Trois décisions structurantes sont prises :

1. **Architecture de dispatch** : comment les événements runtime sont-ils transformés et routés vers les canaux ? Faut-il un modèle push (abonnement EventBus) ou pull (polling SQLite) ?
2. **Extensibilité des canaux** : les trois canaux initiaux (desktop OS, SSE dashboard, webhook HTTP) partagent-ils un trait commun, ou chaque canal est-il câblé en dur ?
3. **Format du payload webhook** : JSON propriétaire Apollia ou format configurable via template (Handlebars, Tera) pour compatibilité native avec Slack/PagerDuty/etc. ?

Ces décisions engagent l'interface publique de `apollia-notifications` et la configuration `apollia.toml` — difficiles à inverser sans casser les configs existantes.

**Contraintes :**
- Principe #1 (local-first) : les notifications ne doivent pas forcer une dépendance cloud — le canal desktop fonctionne hors ligne.
- Principe #2 (zéro dépendance externe) : minimiser les dépendances. `notify-rust` et `reqwest` sont acceptables (légères, compilées statiquement).
- Principe #4 (fail fast) : un canal qui échoue (timeout webhook, démon libnotify absent) ne doit pas crasher le runtime.
- Principe #5 (un acteur, une responsabilité) : `NotificationEngine` est responsable du dispatch uniquement — il ne stocke pas l'état des tâches.

---

## Décision

### Architecture : trait `NotificationChannel` + abonnement EventBus direct

Nous introduisons un trait `NotificationChannel` (Send + Sync) :

```rust
pub trait NotificationChannel: Send + Sync {
    fn id(&self) -> &str;
    async fn send(&self, notif: &Notification) -> Result<(), NotifError>;
}
```

`NotificationEngine` s'abonne à l'`EventBus` au démarrage, convertit les `RuntimeEvent` en `Notification` via `map_event()`, et itère sur `Vec<Box<dyn NotificationChannel>>`. Les trois canaux initiaux implémentent ce trait : `DesktopChannel` (notify-rust v4), `SseChannel` (bridge EventBus → SSE dashboard existant Sprint 9), `WebhookChannel` (reqwest).

### Format webhook : payload JSON fixe Apollia

Le payload webhook est un objet JSON fixe défini par Apollia OS :

```json
{
  "event":     "task.input_required",
  "timestamp": "2026-03-08T14:23:11Z",
  "runtime":   "apollia-os",
  "version":   "0.2.0",
  "task_id":   "t-0042",
  "agent":     "devis-agent",
  "message":   "Devis #42 — 12 500€ TTC — confirmer ?",
  "metadata": {
    "resume_url":  "http://localhost:7771/api/v1/tasks/t-0042/resume",
    "inspect_url": "http://localhost:7771/dashboard#task-t-0042",
    "severity":    "warning"
  }
}
```

Header `X-Apollia-Event: task.input_required` inclus. Les intégrateurs (Slack, n8n, Zapier) utilisent leurs propres mécanismes de transformation (webhook n8n → message Slack, etc.).

---

## Alternatives considérées

### Polling SQLite au lieu de l'abonnement EventBus (rejetée)

**Architecture :** `NotificationEngine` interroge périodiquement la table `tasks` pour détecter les nouveaux états `input_required` ou `failed`. Aucune dépendance sur `EventBus`.

**Pour :**
- Résilience accrue : si `NotificationEngine` redémarre, il peut rattraper les événements manqués depuis SQLite.
- Découplage total de l'`EventBus` — `apollia-notifications` ne dépend que de `apollia-tools` (SQLite).

**Contre :**
- Latence de notification configurable mais non nulle (intervalle polling 1–5s minimum). Un `task.input_required` peut attendre jusqu'à 5 secondes avant notification — inacceptable pour un HITL interactif.
- Complexité : marquage des lignes déjà notifiées nécessaire (colonne `notified_at` ou table de curseurs). Source de bugs.
- Contredit le pattern EventBus central du projet : tous les acteurs (`TaskRouter`, `Supervisor`, `TriggerEngine`, `SSEChannel`) s'abonnent à l'`EventBus` — le polling crée une exception sans bénéfice net.
- Événements éphémères sans persistance SQLite (ex. : `AgentDegraded`, `LlmModelFailed`) ne sont pas capturables par polling.

### Templates Handlebars/Tera pour le payload webhook (rejetée)

**Architecture :** Le payload webhook est généré depuis un template configurable dans `apollia.toml`. L'utilisateur définit son propre JSON (ou XML, ou format Slack Block Kit) par canal webhook.

**Pour :**
- Compatible nativement avec Slack Incoming Webhooks (format `{"text": "..."}`) et PagerDuty Events API — pas d'étape de transformation chez l'intégrateur.
- Flexibilité maximale : un webhook peut produire n'importe quel format.

**Contre :**
- Ajout de `handlebars = "6"` ou `tera = "1"` (~500 Ko compilé) pour un cas d'usage que les intégrateurs (n8n, Zapier, Make) gèrent déjà côté destination.
- Courbe d'apprentissage : l'utilisateur doit maîtriser la syntaxe Handlebars + la structure `Notification` Rust. Documentation lourde pour un runtime ciblant les non-experts ML.
- Bugs de template silencieux : une erreur de rendu produit une notification malformée sans validation statique. Viole Principe #4.
- Slack, n8n, Zapier et Make proposent tous une transformation entrante native (n8n : Function node, Zapier : Code step). Le format fixe Apollia + transformation côté destination est la séparation des responsabilités correcte.

### Canaux câblés en dur sans trait commun (rejetée)

**Architecture :** `NotificationEngine` contient des champs typés `Option<DesktopChannel>`, `Option<WebhookChannel>`, `Option<SseChannel>` et appelle chaque canal directement via des `if let Some(ch) = ...`.

**Pour :**
- Implémentation plus simple — pas de `Box<dyn NotificationChannel>` ni de dispatch dynamique.
- Pas d'allocation heap par canal.

**Contre :**
- Ajouter un quatrième canal (ex. : `EmailChannel`, `TelegramChannel`) nécessite de modifier `NotificationEngine` — violation du principe ouvert/fermé.
- Tests unitaires impossibles sans instancier chaque canal concret (`DesktopChannel` lance un démon libnotify, `WebhookChannel` fait de vraies requêtes HTTP). Avec le trait, un `MockChannel` suffit.
- Incohérence avec le pattern trait du projet (ADR-015 `ToolExecutor`, ADR-016 `AgentRunner`, ADR-019 `AgentLoader`) — chaque brique extensible utilise un trait.

### Dashboard SSE uniquement sans notifications externes (rejetée)

**Architecture :** Pas de crate `apollia-notifications`. Les événements HITL sont visibles uniquement dans le dashboard localhost:7771 via le flux SSE existant (Sprint 9).

**Pour :**
- Zéro nouvelle crate, zéro nouvelle dépendance. Implémentation la plus simple.
- Le dashboard SSE est déjà opérationnel depuis Sprint 9.

**Contre :**
- Un agent suspendu en `input_required` est invisible si le dashboard n'est pas ouvert. L'utilisateur ne sait pas qu'une approbation l'attend. Objectif HITL du sprint non atteint.
- La valeur principale de HITL (interruption push de l'utilisateur) dépend d'une notification proactive, pas d'une consultation active du dashboard.

---

## Conséquences

**Positives :**
- `NotificationEngine` est testable unitairement via `MockChannel : NotificationChannel`.
- Ajout de nouveaux canaux (email, Telegram) sans modifier le core engine — implémenter le trait et enregistrer dans la config.
- Latence de notification quasi-nulle : abonnement direct EventBus, aucun polling.
- Le payload JSON fixe Apollia est versionné (`"version": "0.2.0"`) — les intégrateurs peuvent tester la compatibilité à la réception.
- Échec d'un canal (timeout 5s sur webhook, libnotify absent) → warning `tracing::warn!` uniquement. Le runtime continue. Les autres canaux ne sont pas affectés.

**Négatives / Compromis :**
- Le format JSON fixe Apollia nécessite une transformation côté intégrateur pour les services avec format propriétaire (Slack Block Kit, PagerDuty API v2). Documenté dans le guide `notify-webhooks.md`.
- `notify-rust v4` requiert libnotify sur Linux (paquet système). Sur les environnements headless/CI, `DesktopChannel.send()` retourne `NotifError::DesktopUnavailable` — non-critique, warning uniquement.
- `SseChannel` réutilise le broadcast de l'`EventBus` existant plutôt qu'un canal SSE dédié — si l'`EventBus` est saturé (> 1024 messages), des notifications SSE peuvent être perdues.
- L'historique des notifications (`notify logs`) nécessite une table SQLite dédiée dans Sprint 11 (`notification_logs`) — la persistance n'est pas gratuite.

**Neutres / À surveiller :**
- `reqwest` est déjà une dépendance de `apollia-llm` (feature `cloud`) — partager la version pour éviter deux compilations. Vérifier `reqwest = "0.12"` cohérent.
- `notify-rust v4` sur macOS utilise `NSUserNotificationCenter` (déprécié macOS 12+) — surveiller la migration vers `UNUserNotificationCenter` dans notify-rust v5.
- La table `notification_logs` grossit sans borne si `events = ["task.completed"]` est activé sur un runtime très actif. Ajouter une rotation automatique (TTL 30 jours) avant Sprint 12.

---

## Principes architecturaux impactés

- **Principe #1 — Local-first** : `DesktopChannel` (libnotify/NSUserNotification) et `SseChannel` fonctionnent sans réseau. `WebhookChannel` est optionnel — sa désactivation n'affecte pas le runtime.
- **Principe #2 — Zéro dépendance externe** : `notify-rust` et `reqwest` sont des dépendances Cargo compilées statiquement dans le binaire. Aucun service externe requis pour le canal desktop.
- **Principe #4 — Fail fast** : la configuration des canaux est validée au démarrage (`url` webhook non vide, canal `type` reconnu). Un canal invalide dans `apollia.toml` → erreur fatale au démarrage, pas à la première notification.
- **Principe #5 — Un acteur, une responsabilité** : `NotificationEngine` dispatche uniquement — il ne stocke pas l'état des tâches ni ne prend de décisions de routage métier. La persistance des logs est déléguée à `apollia-tools` (SQLite).

---

## Liens

- Stories associées : STORY-099, STORY-100, STORY-101, STORY-102, STORY-104, STORY-106 (Sprint 11)
- ADR précédents liés :
  - ADR-015, ADR-016, ADR-019 — pattern trait testable réutilisé pour `NotificationChannel`
  - ADR-021 — apollia-triggers : pattern `[[triggers]]` TOML réutilisé pour `[[notifications.channels]]`
  - ADR-023 — HITL : `TaskInputRequired` est le déclencheur principal de notification
