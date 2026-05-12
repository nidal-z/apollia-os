# ADR-089 — Client MCP OAuth 2.1 conforme (RFC 9728 + RFC 8707 + CIMD)

**Date :** 2026-05-12
**Statut :** Proposé
**Sprint :** Pré-implémentation (chantier Connecteurs & MCP v0.1.0)

---

## Contexte

La spec MCP 2025-11-25 normalise la sécurité côté transport HTTP autour de OAuth 2.1 draft-13 avec un faisceau de RFCs obligatoires. Pour qu'Apollia puisse consommer **n'importe quel MCP server HTTP officiel** (GitHub, Atlassian Rovo, Cloudflare, Stripe…) sans configuration manuelle, le client MCP doit implémenter le flow de découverte standard et le code flow OAuth 2.1.

Composants normatifs MCP 2025-11-25 :
- **RFC 9728** (Protected Resource Metadata) — well-known `/.well-known/oauth-protected-resource`.
- **RFC 8707** (Resource Indicators) — paramètre `resource=` MUST.
- **RFC 8414** (AS Metadata) ou **OIDC Discovery 1.0**.
- **RFC 7591** (Dynamic Client Registration) en fallback.
- **Client ID Metadata Documents (CIMD)** — voie recommandée MCP "no prior relationship".
- **PKCE S256 MUST**.
- **Bearer token** dans `Authorization` header.

Apollia dispose déjà de `apollia-auth` (ADR-064) avec OAuth2 PKCE + keyring + callback HTTP — base solide mais utilisée uniquement pour les LLM providers.

## Décision

Nous étendons `apollia-auth` avec un module **`mcp_oauth`** qui implémente le flow MCP 2025-11-25 complet, et nous **hébergeons un document CIMD** statique sur `https://apollia.fr/.well-known/mcp-client-metadata` (Cloudflare Pages). L'ordre de priorité pour l'identification client est : (1) pré-enregistrement statique, (2) **CIMD** (recommandé MCP), (3) DCR (RFC 7591) en fallback. Tous les flows utilisent PKCE S256 et envoient toujours `resource=` (RFC 8707 MUST).

Le refresh token est protégé par **singleflight** (`tokio::sync::OnceCell` / pattern `dashmap<key, Shared<Future>>`) pour éviter le burst de N refresh requests concurrents.

## Alternatives considérées

### Option A — Pré-enregistrement statique uniquement (rejetée)
**Pour :** simple.
**Contre :** ne scale pas — chaque nouveau MCP officiel exige une release Apollia.

### Option B — DCR (RFC 7591) systématique (rejetée)
**Pour :** un seul mécanisme.
**Contre :** dépend de la disponibilité du `registration_endpoint` côté AS ; expose Apollia à des AS qui exigent un secret côté client (incompatible client public PKCE).

### Option retenue — CIMD prioritaire + DCR fallback + pré-enregistrement pour les cas connus
**Pour :** recommandé par la spec MCP. Permet l'auto-discovery sans rebuild Apollia. CIMD = un fichier JSON statique, zéro infra.
**Compromis acceptés :** dépendance à la disponibilité de `apollia.fr/.well-known/mcp-client-metadata` (mitigation : Cloudflare Pages free tier + cache local du document).

## Conséquences

**Positives :**
- N'importe quel MCP server HTTP officiel se connecte sans config (auto-discovery RFC 9728).
- Tokens en keyring local, jamais relayés.
- Singleflight évite les rate-limits AS lors d'un burst d'appels agent.

**Négatives / Compromis :**
- Code OAuth client à maintenir conformément aux évolutions spec (juin 2026 : stateless transport, DPoP potentiel).
- CIMD document doit rester stable et hébergé.

**À surveiller :**
- SEP-1932 (DPoP) et SEP-1933 (Workload Identity Federation) — discussions actives roadmap 2026.
- Step-up auth (SEP-835) sur 403 `insufficient_scope`.

## Principes architecturaux impactés

- Principe #1 — Local-first : ✅ tokens en keyring local.
- Principe #4 — Fail fast : erreurs explicites à chaque étape de discovery.
- Principe #7 — Garde-fous non-négociables : PKCE S256 + RFC 8707 audience binding empêchent confused deputy attacks.

## Liens

- ADR-064 — OAuth2 PKCE keyring (étendu par cet ADR)
- ADR-088 — Architecture hybride connecteurs/MCP
- Spec MCP 2025-11-25 — https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization
- Plan : §3.0, §3.8, §5
