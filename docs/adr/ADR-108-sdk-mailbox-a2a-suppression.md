# ADR-108 — Suppression de la mailbox A2A `ctx.send/receive`

**Date :** 2026-05-19
**Statut :** Accepté
**Sprint :** Phase release v0.1.0 (refonte SDK)

---

## Contexte

`ctx.send(to_agent, message)` et `ctx.receive(timeout)` ont été introduits
au sprint 22 pour offrir un canal A2A async fire-and-forget, en
complément de `ctx.delegate()` (synchrone, request-response). L'idée
était de modéliser des agents qui poussent des messages dans la
mailbox d'un autre sans bloquer.

**État observé au 2026-05-19** (audit du repo) :

- `sdk/apollia/stubs/context.py:155-179` — méthodes typées (`send`,
  `receive`) avec docstrings ambiguës :
  - `send` : "post a message to another agent's mailbox" — pas clair
    si l'agent récepteur traite immédiatement ou plus tard.
  - `receive` : "block until a message arrives or timeout" — bloque le
    handler async, pas viable pour des skills qui doivent répondre vite.
- **Aucun agent bundled** ne fait `ctx.send(...)` ou `ctx.receive(...)`
  (recherche `grep -r "ctx\.send\|ctx\.receive" agents/` : 0 résultats).
- **Aucun test SDK** ne couvre ces méthodes (`grep "send\|receive" sdk/
  tests/`: 0 hits sur les versions modernes).
- **Aucune documentation** docs/book/wiki n'explique quand utiliser
  `ctx.send/receive` vs `ctx.a2a_invoke` — la frontière est floue.
- Sémantique douteuse : si A `send`-e à B, est-ce que B doit avoir un
  `on_message` ? Que se passe-t-il si B n'est pas démarré ? Persistance ?
  TTL ? Aucune réponse claire dans le code.
- Conflit conceptuel avec `ctx.a2a.invoke()` (ADR-102) : si invoke est
  request-response, à quoi sert un fire-and-forget en plus ? Le cas
  cité historiquement (event bus inter-agents) est mieux servi par
  `ctx.events` ou la queue triggers (ADR-triggers crate).

Bref : API à coût de maintenance non nul (PyO3 binding + tests + doc) +
zéro usage en production + sémantique non-spec'd = candidate à
suppression.

## Décision

**Nous supprimons sans remplacement les méthodes `ctx.send(to_agent,
message)` et `ctx.receive(timeout)`. Pas de shim, pas de deprecation
window, pas d'équivalent. `ctx.a2a.invoke` (ADR-102) couvre 100 % des
cas synchrones. Les usages asynchrones fire-and-forget sont reportés à
v2.0 sous la forme d'une vraie spec event bus.**

Concrètement :

1. **Suppression côté SDK** — `sdk/apollia/stubs/context.py` : retirer
   les méthodes `send` et `receive`.
2. **Suppression côté Rust** — `crates/apollia-aip/src/context.rs` :
   retirer les méthodes PyO3 `send` et `receive` du `RuntimeContext`.
3. **Suppression côté runtime** — l'acteur mailbox (`apollia-runtime/
   src/mailbox.rs` si présent) est supprimé. EventBus existant reste.
4. **Plan-cache & event bus** : si un usage légitime émerge (un agent A
   qui doit signaler "j'ai fini" à un agent B sans attendre une
   réponse), il passe par `ctx.events.emit_progress(...)` (ADR-105)
   pour l'observabilité humaine, ou par `ctx.a2a.invoke(...)` côté
   business logic (avec timeout court si vraiment fire-and-forget).
5. **Documentation** — le book mentionne explicitement la suppression
   avec un encadré "Vous cherchez `ctx.send` ? Utilisez `ctx.a2a.invoke`."

## Alternatives considérées

### Option A — Garder mais documenter (rejetée)

**Pour :** zéro effort suppression.
**Contre :** maintient une API trompeuse. La doc ne réparera pas la
sémantique floue. Coût de maintenance non nul.

### Option B — Renommer en `ctx.a2a.notify(to_agent, payload)` avec
sémantique fire-and-forget claire (rejetée)

**Pour :** garde un canal asynchrone légitime.
**Contre :** ré-introduit une notion de "mailbox" qu'on ne veut pas
spec'er en v1.0 (persistance ? TTL ? au cas où le destinataire crash ?).
Reporter au moment où on a un cas d'usage concret.

### Option C — Conserver comme alias deprecated de `ctx.a2a.invoke`
sans `await` côté caller (rejetée)

**Pour :** rétrocompat minimale.
**Contre :** sémantique différente (invoke = request-response,
fire-and-forget ≠). Aliasing serait fallacieux.

### Option retenue — Suppression sèche

**Pour :** réduit la surface API. Élimine une zone de confusion. Aligne
avec la philosophie "rien ne pré-existe sans cas d'usage validé".
**Compromis acceptés :** zéro pivot ergonomique si quelqu'un quelque
part comptait s'en servir (mais aucun signal). Documenté breaking
change.

## Conséquences

**Positives :**

- Surface API de `ctx` réduite (2 méthodes en moins).
- Élimination d'une zone de confusion sémantique pour les auteurs.
- Suppression de ~80 LOC côté SDK + ~120 LOC côté Rust (binding,
  mailbox acteur si présent, tests).
- Force le pattern correct : "tu veux notifier ? `ctx.events`. Tu veux
  inter-agent ? `ctx.a2a.invoke`."
- Aligne avec ADR-102 (`ctx.a2a` API unifiée).

**Négatives / Compromis :**

- Si un usage caché existait dans un agent client externe (zéro signal
  aujourd'hui), il casse. Pas de window de deprecation.
- Pas de canal fire-and-forget natif jusqu'à v2.0. Workaround :
  `asyncio.create_task(ctx.a2a.invoke(...))` sans `await` —
  techniquement faisable mais non recommandé (le task survit-il ?).

**À surveiller :**

- Émergence post-release d'un besoin fire-and-forget (signal côté
  community Slack/GitHub). Si > 3 demandes, spec'er proprement un event
  bus en v1.x.
- Mauvaises pratiques tentantes : auteurs qui font `asyncio.create_task`
  d'un `ctx.a2a.invoke` pour simuler fire-and-forget. Documenter
  "déconseillé" et fournir un pattern alternatif si besoin émerge.

## Principes architecturaux impactés

- **Principe #3 — Contrat minimal** : renforcé. Une API qui ne sert
  personne est une dette ; on l'enlève.
- **Principe #4 — Fail fast** : un code legacy qui appelle `ctx.send`
  va lever `AttributeError` à l'import — visible immédiatement, pas en
  prod.

## Liens

- ADR-102 — API A2A unifiée (alternative pour invoke synchrone)
- ADR-105 — `ctx.events` (alternative pour observabilité)
- ADR-101 — `ctx` Protocol (réduction de surface)
- ADR-049 — A2A skill-based dispatch (si présent — concept préservé)
