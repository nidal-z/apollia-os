# ADR-041 - Messagerie inter-agents durable et auditable

**Date :** 2026-07-10
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Chantier :** #5 (messagerie inter-agents)

---

## Contexte

Le runtime possède déjà un actor mailbox fonctionnel (`crates/apollia-runtime/src/mailbox.rs`),
spawné en permanence au démarrage (`supervisor.rs:1181`) et branché dans `AppState`
(`api/server.rs:179`) et le chemin embarqué (`embedded.rs:88`). Il gère des files bornées par
destinataire, émet déjà `RuntimeEvent::AgentMessageSent` (`mailbox.rs:213`), et une route
HTTP en lecture seule l'expose (`GET /api/v1/agents/{name}/messages`, `routes_messages.rs:60`).
Mais aucun agent ne peut l'utiliser : la surface `ctx` n'a jamais été posée. Les helpers
`send_inner`/`receive_inner` (`context.rs:1891-1909`) et le champ `RuntimeContext.mailbox`
(`context.rs:1095`) sont morts, référencés uniquement par des tests. C'est une capacité à
moitié construite : l'infra existe, l'API agent manque.

Historique décisif : **ADR-024 a explicitement retiré** l'ancien `ctx.send`/`ctx.receive`
(`ADR-024:82-89`) en citant quatre objections non résolues :

1. persistance non spécifiée,
2. TTL non spécifié,
3. remise sur destinataire arrêté non spécifiée,
4. absence de frontière claire face à `ctx.a2a.invoke`.

ADR-024 a différé "un vrai bus asynchrone jusqu'à ce qu'un cas d'usage concret justifie une
spécification propre". Le chantier #5 apporte ce cas d'usage (six scénarios professionnels de
coordination multi-agents documentés en D1) et cette spécification propre (D2). Cet ADR lève
les quatre objections une à une et supersède la posture de retrait d'ADR-024.

Pourquoi maintenant : la messagerie asynchrone débloque des patrons que le RPC synchrone
`ctx.a2a.invoke` (ADR-025) ne peut pas exprimer sans bloquer l'appelant (fan-out agrégé au fil
de l'eau, notification producteur/consommateur, handoff de tâche longue, supervision hôte,
annulation hors-bande, progression non bloquante). Elle constitue aussi un différenciateur
produit aligné sur le beachhead : une messagerie inter-agents auditable et pilotable par
l'hôte, argument direct pour l'EU AI Act (record-keeping, oversight) et pour "l'intégration est
le produit" (ADR-037).

Contrainte : les huit principes (notamment #5 un actor une responsabilité, #7 garde-fous non
contournables, #6 à l'initiative de l'agent, #1 local-first, #8 API machine), et le format de
contrat `Ctx` vérifié au chargement (ADR-024).

## Décision

Nous adoptons une messagerie inter-agents **durable, auditable et pilotable par l'hôte**,
exposée aux agents sous un nouveau service dédié `ctx.mail`, distinct de `ctx.a2a`. Elle
répond point par point aux quatre objections d'ADR-024.

### Un service dédié `ctx.mail` (lève l'objection 4)

La messagerie devient le 15e service du contrat `Ctx` (`sdk/apollia/types.py`), pas une facette
de `ctx.a2a`. La frontière mentale est nette et documentée : `ctx.a2a.invoke` appelle une skill
et attend un résultat typé (RPC synchrone, ADR-025) ; `ctx.mail.send` poste un message dans la
boîte d'un agent et continue (asynchrone, non bloquant). L'API agent est
`send`/`receive`/`poll`/`pending`/`list`/`ack`/`nack`, adossée à une pyclass Rust
`MailInterface` miroir de `A2AInterface` (`a2a.rs:38-50`), branchée dans `RuntimeContext` selon
le même patron (champ `Option<Py<...>>`, construction sous `with_gil`, `#[getter]`). Ajout d'un
service = bump mineur SemVer du contrat SDK.

### Un store durable SQLite (lève les objections 1 et 3)

Les messages sont persistés dans une table SQLite possédée en exclusif par l'actor mailbox
(une connexion, un actor, principe #5, sur le patron de l'audit journal actor). Ils survivent
au redémarrage tant qu'ils ne sont pas accusés ; un destinataire arrêté retrouve ses messages
à son retour. La remise est **at-least-once** : `receive` loue le message (état in-flight,
délai de visibilité, défaut 60 s) au lieu de le supprimer ; l'accusé (ack) le supprime ; un
crash avant accusé laisse le lease expirer, ce qui réexpédie le message. L'accusé est
automatique quand le contexte consommateur se termine avec succès ; `ack`/`nack` explicites
restent disponibles. Ce choix at-least-once est tranché dès la spécification (pas différé), car
il façonne le schéma SQLite et l'API : le rajouter après serait un refactor.

L'ordre est **FIFO best-effort par destinataire**, pas strict : un message dont le lease expire
(ou refusé par `nack`) est réexpédié après des messages plus récents déjà livrés. C'est
inhérent aux files at-least-once à délai de visibilité ; l'ordre strict n'est pas garanti sous
réexpédition, et cette limite est assumée explicitement plutôt que promise à tort.

La taille du payload d'un message est bornée par une limite configurable
(`mailbox_max_payload_bytes`) rejetée à l'envoi, pour empêcher un agent de gonfler le store
durable.

### Un TTL et une éviction bornée (lève l'objection 2)

Chaque message porte `sent_at` ; un balayage évince les messages jamais relevés au-delà d'un
TTL configurable (`mailbox_message_ttl`, défaut 24 h) et réexpédie les messages loués dont le
lease a expiré (`mailbox_visibility_timeout`, défaut 60 s). L'éviction émet `AgentMessageDropped`
{ reason: expired } et une entrée d'audit. Le store est ainsi borné, l'objection TTL levée.

### Adressage, scoping et garde-fous

- Adressage unicast par nom d'agent enregistré, auto-adressage autorisé ; le fan-out se fait
  par N envois unicast. Broadcast/topics/groupes reportés en extensions futures (sobriété).
- Scoping : capability `mailbox` déclarée au manifest, opt-in obligatoire (comme
  secrets/datasources), avec allowlist de destinataires optionnelle. Sans déclaration, aucun
  accès.
- Destinataire inconnu : `send` valide contre l'`AgentRegistry` et rend
  `MailboxError::UnknownRecipient` (fail-fast, principe #4), corrigeant le comportement actuel
  de création silencieuse de file (`mailbox.rs:200`).
- Anti-spam : quota d'envois par run appliqué dans l'actor (défaut prudent, de l'ordre de 50
  envois par run, configurable), émettant `MailboxGuardTriggered` sur le patron de
  `A2AGuardTriggered` (`events.rs:1050`). Le `StepBudget` n'est pas surchargé (un message n'est
  pas un pas de raisonnement). Non contournable depuis Python (principe #7).
- HITL : non gaté par défaut (messagerie locale, principe #1) ; gate opt-in via
  `PermissionEngine` (nom d'outil synthétique `mailbox:send`) ou `tools_requiring_approval`,
  imposable par l'hôte.

### Auditabilité prouvable

Chaque envoi, remise, accusé et abandon émet un `RuntimeEvent` et, via le subscriber, une
entrée de journal d'audit signée HMAC-SHA256 et chaînée. Prérequis dur : les évènements
mailbox doivent porter un `run_id`, faute de quoi le subscriber les ignore
(`subscriber.rs:483`). Pour un message envoyé par un agent, c'est le `run_id` de l'émetteur.
Pour un message **injecté par l'hôte** (qui n'a pas de run agent), l'injection alloue un
`run_id` synthétique de portée hôte, de sorte que les messages injectés sont journalisés sur
leur propre chaîne d'audit et que l'invariant "tout ce qui est journalisé porte un `run_id`"
tient sans cas particulier dans le subscriber. Sans ce `run_id` synthétique, l'injection hôte
troue la promesse "l'hôte injecte et tout est auditable". Les entrées portent `from`, `to`,
`message_id`, `payload_hash`, `sent_at` ; le payload complet n'est journalisé que si un flag
runtime l'active (`mailbox_audit_full_payload`, off par défaut). Preuve de non-répudiation sans
stocker le contenu au repos. Tout reste fire-and-forget (aucun impact sur le chemin d'envoi).

### Contrôle par l'hôte (contrat de pilotage)

L'API `/api/v1` (ADR-037) expose, en additif et non cassant : l'observation
(`GET .../messages` existant + un flux SSE `GET /api/v1/mailbox/stream`), la preuve
(`GET /api/v1/mailbox/audit`), l'injection (`POST /api/v1/agents/{name}/messages`, émetteur
`host:<id>`, avec allocation d'un `run_id` synthétique de portée hôte pour l'auditabilité), et
le gate (routage via `PermissionEngine`, policy hold-for-approval). Tout annoté utoipa, donc
propagé automatiquement aux SDK hôte TS et Python (`clients/regen.sh`). L'hôte reste maître de
la chorégraphie.

## Alternatives considérées

### Retirer définitivement le mailbox (rejetée)

**Pour :** cohérent avec la posture d'ADR-024, zéro nouvelle surface.
**Contre :** gaspille une infra déjà construite et branchée, et abandonne un différenciateur
produit (coordination asynchrone auditable) désormais justifié par des cas d'usage concrets.
ADR-024 avait explicitement conditionné le retrait à l'absence de cas d'usage et de spec ; les
deux existent maintenant.

### File volatile en mémoire (statu quo) ou hybride (rejetée)

**Pour :** le plus simple ; l'actor existant est déjà in-memory.
**Contre :** perd les messages au redémarrage, incompatible avec le handoff durable et la
promesse "fiable + auditable". L'hybride (file volatile + audit persisté) prouve mais ne
garantit pas la remise. Écarté par arbitrage produit au profit de la garantie complète.

### Livraison at-most-once (suppression à la relève) (rejetée)

**Pour :** schéma et API plus simples (pas de lease ni d'accusé).
**Contre :** incohérent avec un store durable choisi pour la fiabilité ; un crash de l'agent
entre `receive` et la fin de traitement perdrait le message. La garantie de traitement est
précisément ce que le produit vend. Le rajouter en v2 serait un refactor du schéma et de l'API.

### Étendre `ctx.a2a` avec des méthodes de messagerie (rejetée)

**Pour :** pas de nouveau service.
**Contre :** réintroduit exactement le flou de frontière qu'ADR-024 a voulu supprimer. Un
service séparé garde le modèle mental propre.

### Compter les envois dans le `StepBudget` (rejetée)

**Pour :** réutilise un garde-fou non contournable existant.
**Contre :** pollue la comptabilité du budget de raisonnement (un envoi n'est pas un pas). Une
garde dédiée sur le patron A2AGuard est plus juste sémantiquement.

### Option retenue - messagerie durable, auditable, pilotable

**Pour :** lève les quatre objections d'ADR-024, débloque les use-cases asynchrones, sert le
beachhead (auditabilité + contrôle hôte), réutilise l'infra et les patrons existants (actor,
A2AInterface, audit journal, PermissionEngine, contrat de pilotage).
**Compromis acceptés :** migration de l'actor de `VecDeque` vers un store SQLite (contention et
GC à gérer), surface de contrat élargie (15e service, endpoints et évènements additifs),
complexité du lease/accusé assumée dès la v1.

## Conséquences

**Positives :**
- La capacité à moitié construite devient un produit complet et cohérent.
- Différenciateur EU AI Act concret : prouver ce que les agents se sont dit, et donner à l'hôte
  une prise sur leur coordination.
- Frontière `mail` vs `a2a` nette, dette de conception d'ADR-024 résorbée proprement.
- Additif et non cassant : les agents existants et `ctx.a2a` ne sont pas affectés.

**Négatives / Compromis :**
- L'actor mailbox devient stateful sur SQLite (schéma, migration, GC, contention) au lieu d'un
  simple `VecDeque`.
- Le contrat `Ctx` passe de 14 à 15 services, imposant une régénération documentaire (rulebook,
  wiki, book, SDK hôte).
- Le lease et l'accusé at-least-once ajoutent de la complexité au chemin de consommation.

**Neutres / À surveiller :**
- Croissance du store durable sous fort trafic (TTL, quota et borne de payload comme limites).
- Contention de l'actor mailbox si de nombreux agents consomment simultanément.
- Valeurs retenues, à ajuster à l'usage : délai de visibilité 60 s, TTL 24 h, quota d'envois par
  run de l'ordre de 50, accusé automatique sur succès plus `ack`/`nack` explicites optionnels.
- L'ordre FIFO best-effort (non strict sous réexpédition) : à surveiller si un cas d'usage
  exige un ordre strict, qui relèverait alors d'une extension.

## Principes architecturaux impactés

- **Principe #1 - Local-first :** toute la messagerie reste in-process et locale ; aucun message
  ne traverse la frontière machine.
- **Principe #5 - Un actor, une responsabilité :** le store durable est possédé en exclusif par
  l'actor mailbox (mpsc borné + handle clonable), jamais d'`Arc<Mutex<T>>` partagé.
- **Principe #6 - Mémoire à l'initiative de l'agent :** le modèle pull ; le destinataire relève
  ses messages quand il le décide, jamais d'injection automatique.
- **Principe #7 - Garde-fous non contournables :** quota anti-spam, cap par destinataire, gating
  de capability et de permission sont appliqués par le runtime, non contournables depuis Python.
- **Principe #4 - Fail fast :** destinataire inconnu, capability non déclarée, et config invalide
  échouent tôt.
- **Principe #8 - API machine :** l'exposition hôte étend le contrat de pilotage versionné.

## Liens

- Cartographie et spécification : `docs/internal/cartography/mailbox-spec/01-besoin-usecases.md`,
  `docs/internal/cartography/mailbox-spec/02-specification.md`,
  `docs/internal/cartography/mailbox-spec/03-plan-implementation.md`
- ADR liés : ADR-024 (contrat runtime `ctx`, qui avait retiré le mailbox ; superseded sur ce
  point), ADR-025 (workers et routage A2A synchrone, complément du mailbox), ADR-037 (contrat
  de pilotage hôte, étendu ici), ADR-023 (décorateurs AgentKit, capability manifest),
  ADR-015 (gouvernance des permissions), ADR-033 (journal d'audit signé et `JournalEntryKind`,
  étendu ici avec les kinds Message*), ADR-012 (observabilité et EventBus)
- Story associée : à créer (chantier #5, phase 2)
