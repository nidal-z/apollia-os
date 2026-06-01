# ADR-006 - REST JSON (pas gRPC) pour l'API locale

**Date :** 2026-03
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** Pré-implémentation

---

## Contexte

Le runtime expose une API consommée par deux types de clients : la CLI `apollia-os` (processus local) et les SDK Python (intégrations tierces). L'API doit être simple à déboguer, compatible avec `curl`, et utilisable sans génération de code client.

## Décision

Nous utilisons REST/JSON avec axum. Deux transports : Unix socket `/tmp/apollia.sock` pour la CLI (latence minimale, pas de TCP overhead), TCP `localhost:7771` pour les SDK et intégrations externes.

## Alternatives considérées

### Option A - gRPC + protobuf (rejetée)
**Pour :** Performances supérieures, typage fort avec protobuf, streaming natif.
**Contre :** Génération de code protobuf dans chaque client. Non debuggable avec `curl`. Complexité client Python. Over-engineered pour une API locale.

### Option B - Unix socket seul (rejetée)
**Pour :** Performances maximales, isolation réseau totale.
**Contre :** Complique les intégrations non-Rust. Les SDK Python ne peuvent pas facilement utiliser un Unix socket sans wrappers.

### Option C - WebSocket (rejetée)
**Pour :** Bidirectionnel, streaming natif.
**Contre :** Over-engineered pour les opérations request/response standard. SSE (Server-Sent Events) est suffisant pour le streaming unidirectionnel des tâches.

### Option retenue - REST/JSON + axum (Unix socket + TCP)
**Pour :** Debuggable avec `curl`. Standard de facto. axum s'intègre nativement avec Tokio. SSE disponible pour le streaming.
**Compromis acceptés :** Légèrement moins performant que gRPC. Non significatif pour le volume de trafic local attendu.

## Conséquences

**Positives :**
- La CLI utilise le socket Unix pour des performances maximales.
- Les SDK Python utilisent TCP 7771 sans complexité.
- `curl localhost:7771/api/v1/health` fonctionne out-of-the-box.
- axum + SSE couvre le streaming des tâches longues.

**Négatives / Compromis :**
- Overhead de sérialisation JSON vs protobuf binaire (non significatif en local).
- Pas de contrat de type statique entre CLI et runtime (compensé par les tests).

**Neutres / À surveiller :**
- Sécurité du socket Unix (permissions fichier) vs TCP (bind uniquement sur 127.0.0.1).

## Principes architecturaux impactés

- Principe #8 - CLI humaine, API machine : `--json` global sur la CLI, REST pour l'API.
- Principe #2 - Zéro dépendance externe : axum est pure Rust, pas de service externe.

## Liens

- Story associée : STORY-033 (APIServer axum Unix socket + TCP)
- ADR précédent sur le même sujet : aucun
