# ADR-046 - Transport HTTP/SSE pour les serveurs MCP distants

**Date :** 2026-03-30
**Statut :** Accepte
**Sprint :** 27

---

## Contexte

Le Sprint 27 a livre un client MCP Registry et un wizard d'installation. Mais le wizard ne fonctionne que pour les serveurs installes via packages npm/pip (transport stdio, subprocess local). Or, le MCP Registry a massivement migre vers les **transports distants** :

- **~70% des serveurs** utilisent `remotes` (streamable-http ou SSE) au lieu de `packages`
- Les serveurs officiels (com.notion/mcp, com.brave/search) sont 100% remotes
- Le champ `remotes` n'est meme pas parse par notre `RegistryServerDetail`

Resultat : la majorite du catalogue est non-installable. Le wizard affiche "configuration manuelle requise" pour la plupart des serveurs, y compris les officiels.

**Contraintes :**
- Le backend MCP (`apollia-mcp`) ne supporte que `transport: "stdio"` - la validation rejette tout autre transport
- La session MCP (`session.rs`) est couplee a `tokio::process::Command` + stdin/stdout pipes
- L'enum `McpTransport` dans `apollia-tools` definit deja `Http` et `WebSocket` mais ils ne sont jamais utilises
- `reqwest` v0.12 avec `stream` feature est deja dans les dependances workspace

## Decision

Nous adoptons une **architecture de transport abstrait** via un trait `McpTransport` dans `apollia-mcp`, avec trois implementations : `StdioTransport` (existant, refactorise), `StreamableHttpTransport` (nouveau, prioritaire), et `SseTransport` (nouveau, second).

Le transport est selectionne dynamiquement a partir du champ `transport` de `McpServerConfig`, enrichi pour accepter `"stdio"`, `"streamable-http"` et `"sse"`.

## Alternatives considerees

### Option A - Proxy local stdio-to-HTTP (rejetee)

Lancer un processus local qui fait pont entre stdio et le serveur HTTP distant, pour garder la session MCP inchangee.

**Pour :** Zero refactoring du session layer
**Contre :** Ajoute un processus intermediaire, latence supplementaire, complexite de lifecycle management. Defait le principe "zero dependance externe" car il faudrait un binaire proxy.

### Option B - Support HTTP uniquement, pas SSE (rejetee en tant que MVP complet)

Implementer seulement streamable-http et ignorer SSE.

**Pour :** Scope reduit, couvre ~60% des serveurs remotes
**Contre :** Les serveurs SSE-only (com.notion/mcp supporte SSE) seraient exclus. Mais SSE est un fallback pour les clients qui ne supportent pas streamable-http.

### Option retenue - Trait transport avec stdio + streamable-http + SSE

**Pour :** Architecture extensible, couvre 100% des transports du registry, reutilise les dependances existantes (reqwest, tokio), aligne avec l'enum `McpTransport` deja definie dans apollia-tools.
**Compromis acceptes :** Refactoring significatif de `session.rs` (~500 LOC), ajout de ~1500 LOC de code transport.

## Consequences

**Positives :**
- 100% des serveurs du MCP Registry deviennent installables via le wizard
- Les serveurs officiels (Notion, Brave) fonctionnent sans contournement
- Architecture prete pour de futurs transports (WebSocket, custom)
- Le trait transport facilite le testing (mock transport pour les tests unitaires)

**Negatives / Compromis :**
- Session.rs subit un refactoring majeur - risque de regression sur les serveurs stdio existants
- Ajout de code asynchrone complexe (gestion reconnexion SSE, correlation requete/reponse HTTP)
- Le `McpServerConfig` doit accepter un champ `url` optionnel pour les transports distants

**A surveiller :**
- Performance des transports HTTP vs stdio (latence reseau)
- Gestion des timeouts et reconnexions pour les serveurs distants instables
- Coherence du lifecycle : un serveur distant ne peut pas etre "kill" comme un subprocess

## Principes architecturaux impactes

- Principe #1 - Local-first : **respecte** - les donnees restent locales, seuls les appels d'outils transitent vers le serveur distant. L'utilisateur choisit explicitement de connecter un serveur distant.
- Principe #2 - Zero dependance externe : **respecte** - reqwest est deja dans les deps, pas de nouveau binaire externe.
- Principe #5 - Un acteur, une responsabilite : **respecte** - chaque transport est une implementation independante du trait, le session actor reste unique.

## Liens

- Stories associees : STORY-368 a STORY-373
- ADR precedent : ADR-044 (Client MCP), ADR-045 (Page Integrations)
