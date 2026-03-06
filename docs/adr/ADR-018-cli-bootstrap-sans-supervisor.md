# ADR-018 — CLI Bootstrap sans Supervisor

**Date :** 2026-03-06
**Statut :** Accepte
**Decideur :** Nidal (solo)
**Sprint :** 5

---

## Contexte

STORY-037 (CLI commandes niveau 1) depend de STORY-039 (Supervisor) et STORY-040 (Graceful shutdown), qui ne sont pas encore implementees. La commande `apollia-os start` doit demarrer le runtime en foreground, ce qui necessite l'instanciation ordonnee des acteurs (EventBus, AgentRegistry, TaskRouter, APIServer).

Sans Supervisor, il faut un mecanisme temporaire pour demarrer les acteurs et gerer l'arret propre. La commande `apollia-os stop` necessite aussi un endpoint HTTP pour signaler l'arret au runtime.

## Decision

Nous utilisons un bootstrap sequentiel inline dans la commande `start` qui cree les acteurs directement dans l'ordre : EventBus -> AgentRegistry -> TaskRouter -> APIServer. L'arret est gere via un endpoint `POST /api/v1/shutdown` qui emet `RuntimeEvent::ShutdownRequested` sur l'EventBus. La commande `start` ecoute cet evenement et declenche le shutdown de l'APIServer.

Ce bootstrap sera remplace par le Supervisor quand STORY-039 sera implementee.

## Alternatives considerees

### Option A — Attendre STORY-039 avant d'implementer la CLI (rejetee)
**Pour :** Architecture propre des le depart, pas de code temporaire.
**Contre :** Bloque le sprint 5. La CLI est le Sprint Goal et ne peut pas etre reportee. Les commandes `status`, `run`, `stop` ne dependent pas du Supervisor.

### Option B — Implementer le Supervisor dans STORY-037 (rejetee)
**Pour :** Pas de code temporaire, architecture complete.
**Contre :** Augmente la taille de STORY-037 (deja L) de maniere significative. Le Supervisor a ses propres complexites (watchdog, restart policy, timeout par acteur) qui meritent une story dediee.

### Option retenue — Bootstrap inline temporaire
**Pour :** Debloque la CLI immediatement. Code simple et lisible. Facile a remplacer par le Supervisor (une seule fonction a changer). L'endpoint shutdown est reutilisable tel quel par le Supervisor.
**Compromis acceptes :** Code temporaire qui sera remplace. Pas de watchdog ni restart policy en attendant STORY-039.

## Consequences

**Positives :**
- La CLI est fonctionnelle sans attendre le Supervisor
- L'endpoint `/api/v1/shutdown` est reutilisable par STORY-039/040
- Le pattern EventBus pour signaler l'arret est coherent avec l'architecture acteur

**Negatives / Compromis :**
- La fonction `bootstrap_runtime()` dans `start.rs` sera supprimee quand le Supervisor existera
- Pas de recovery automatique si un acteur plante (normal pour un MVP foreground)

**Neutres / A surveiller :**
- Quand STORY-039 est implementee, verifier que le Supervisor reutilise le meme endpoint shutdown
- Le client HTTP Unix socket (`RuntimeClient`) est permanent et ne sera pas impacte

## Principes architecturaux impactes

- Principe #5 — Un acteur, une responsabilite : respecte — chaque acteur est cree separement avec son propre canal
- Principe #8 — CLI humaine, API machine : respecte — le bootstrap affiche la progression en texte

## Liens

- Story associee : STORY-037
- Story Supervisor : STORY-039 (remplacera ce bootstrap)
- Story Shutdown : STORY-040 (reutilisera l'endpoint shutdown)
