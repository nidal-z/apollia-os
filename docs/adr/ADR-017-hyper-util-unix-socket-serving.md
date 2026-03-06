# ADR-017 — hyper-util explicite pour Unix socket serving

**Date :** 2026-03-06
**Statut :** Accepte
**Decideur :** Nidal (solo)
**Sprint :** 5

---

## Contexte

STORY-033 (APIServer axum) necessite un serveur HTTP ecoutant simultanement sur TCP et Unix socket. axum 0.7.9 expose `axum::serve()` qui n'accepte que `TcpListener` — la signature est `pub fn serve<M, S>(tcp_listener: TcpListener, ...)`. Le trait `Listener` generique n'existe pas dans axum 0.7.

Pour le listener Unix socket, il faut une boucle accept manuelle convertissant chaque `UnixStream` en connexion hyper via `hyper-util`. `hyper-util` est deja une dependance transitive d'axum 0.7.9, mais elle n'est pas exposee dans le workspace — il faut l'ajouter explicitement.

Par ailleurs, `tower` dans le workspace passait sans features. La feature `util` est necessaire pour `ServiceExt::oneshot()`, utilise dans les tests unitaires axum (pattern standard pour tester un Router sans demarrer de serveur).

## Decision

Nous ajoutons `hyper-util = { version = "0.1", features = ["tokio", "server-auto", "service"] }` aux workspace dependencies et comme dependance directe de `apollia-runtime`.

Les types utilises :
- `hyper_util::rt::TokioIo` — adapte un `UnixStream` tokio en stream hyper
- `hyper_util::rt::TokioExecutor` — executeur pour le builder de connexion
- `hyper_util::server::conn::auto::Builder` — sert la connexion HTTP/1.1 (ou HTTP/2)
- `hyper_util::service::TowerToHyperService` — pont tower::Service vers hyper::service::Service

Nous ajoutons aussi la feature `util` a `tower = "0.4"` pour `ServiceExt`.

## Alternatives considerees

### Option A — Upgrader axum a 0.8+ (rejetee)
**Pour :** axum 0.8 introduit un trait `Listener` generique acceptant `UnixListener` nativement dans `axum::serve()`.
**Contre :** Breaking changes importants entre axum 0.7 et 0.8 (changements d'API Router, middleware, extractors). L'upgrade impacterait tout le code existant du runtime sans benefice fonctionnel au-dela du Unix socket. Risque disproportionne pour le Sprint 5.

### Option B — Proxy TCP interne (rejetee)
**Pour :** Evite toute dependance supplementaire — un deuxieme `TcpListener` sur un port aleatoire servirait de relais entre le Unix socket et axum.
**Contre :** Complexite inutile, latence ajoutee, port supplementaire a gerer, problemes de cleanup. Contre-productif pour un probleme deja resolu par hyper-util.

### Option retenue — hyper-util explicite
**Pour :** Dependance transitive deja presente (zero octet supplementaire dans le binaire), pattern documente dans les exemples axum officiels (`unix-domain-socket`), controle fin sur le lifecycle des connexions Unix socket (shutdown graceful via `tokio::select!`).
**Compromis acceptes :** La boucle accept manuelle pour Unix socket est plus verbeuse que `axum::serve()` pour TCP. Si axum passe a 0.8+ avec `Listener` generique, ce code pourra etre simplifie.

## Consequences

**Positives :**
- Le serveur Unix socket fonctionne avec le meme Router que TCP — memes routes, meme state
- Shutdown graceful via `watch::channel` uniforme pour les deux listeners
- Zero dependance binaire supplementaire (hyper-util est deja liee via axum)

**Negatives / Compromis :**
- Code asymetrique entre TCP (axum::serve, 8 lignes) et Unix socket (boucle manuelle, 25 lignes)
- Couplage explicite a l'API interne de hyper-util (peut changer entre versions mineures)

**Neutres / A surveiller :**
- Quand axum 0.8 sera stable et que le projet envisagera la migration, cette boucle manuelle pourra etre remplacee par un simple `axum::serve(UnixListener, router)`

## Principes architecturaux impactes

- Principe #1 — Local-first : respecte (Unix socket pour la CLI locale, TCP sur localhost uniquement)
- Principe #2 — Zero dependance externe : respecte (hyper-util est deja transitive, pas de nouveau binaire externe)

## Liens

- Story associee : STORY-033
- Documentation axum unix-domain-socket example : https://github.com/tokio-rs/axum/tree/main/examples/unix-domain-socket
