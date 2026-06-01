# ADR-090 - Abstraction `Connector` trait dans `apollia-connectors`

**Date :** 2026-05-12
**Statut :** Proposé
**Sprint :** Pré-implémentation (chantier Connecteurs & MCP v0.1.0)

---

## Contexte

L'ajout de connecteurs natifs Google Workspace + Microsoft 365 (cf. ADR-088) introduit ~25 nouvelles opérations (`gmail.send`, `gcal.create_event`, `outlook.search`, etc.). Sans abstraction commune, le risque est un patchwork de modules disjoints (`gmail.rs`, `outlook.rs`, …) avec duplication de pattern (OAuth token fetch, retry policy, error mapping, tool registration).

Anticipons la roadmap v0.2+ (Salesforce, HubSpot, Teams, Zendesk…) : ajouter un nouveau connecteur doit être **mécanique**, pas un nouveau design.

## Décision

Nous créons un **nouveau crate `apollia-connectors`** organisé autour d'un trait `Connector` + des types associés (`ConnectorManifest`, `OperationSpec`, `AccountInfo`, `HealthReport`). Chaque service (Google, Microsoft, etc.) implémente le trait et déclare ses `OperationSpec`. Le runtime enregistre ces operations dans `apollia-tools::registry` au démarrage - un seul code path pour exposer un connecteur comme set de tools.

```rust
#[async_trait]
pub trait Connector: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    fn manifest(&self) -> &ConnectorManifest;
    fn auth_provider(&self) -> &'static str;
    async fn check(&self, account_id: &AccountId) -> Result<HealthReport, ConnectorError>;
    fn operations(&self) -> &[OperationSpec];
}
```

Plugin dynamique **explicitement rejeté en v0.1.0** - connectors are build-time only. Voir Option C.

## Alternatives considérées

### Option A - Un module par service, pas de trait commun (rejetée)
**Pour :** simple à démarrer.
**Contre :** duplication massive (token fetch, retry, error, registration). N'évolue pas vers v0.2+.

### Option B - MCP servers internes (chaque connecteur exposé via stdio MCP) (rejetée)
**Pour :** unifie tools natifs et MCP.
**Contre :** overhead protocol pour des appels in-process. Latence injustifiée. Hostile à l'observabilité (tracing direct vs JSON-RPC marshalling).

### Option C - Plugin dynamique (.so / WASM) (rejetée v0.1.0)
**Pour :** tiers contributors pourraient ajouter des connecteurs sans rebuild.
**Contre :** complexité sécurité (sandboxing, ABI stability, plugin signing) hors-scope v0.1.0. À reconsidérer post-v0.2.

### Option retenue - Trait `Connector` build-time, crate dédié
**Pour :** ajout d'un connecteur = un module + une impl du trait + enregistrement build-time. Lifecycle clair. Pas de surcoût runtime.
**Compromis acceptés :** rebuild Apollia requis pour ajouter un connecteur natif (acceptable v0.1.0/v0.2).

## Conséquences

**Positives :**
- Ajout d'un nouveau connecteur en v0.2+ = effort prévisible (~3-5 jours par SaaS Workspace/M365-equivalent).
- Tests unitaires par module isolés via `wiremock`.
- Une seule API d'enregistrement avec `apollia-tools`.

**Négatives / Compromis :**
- Pas d'ajout user-side de connecteurs natifs sans rebuild (mitigé : MCP servers custom restent la voie utilisateur).

**À surveiller :**
- Volume du crate `apollia-connectors` à 10+ providers : envisager split par provider si build time devient pénible.

## Principes architecturaux impactés

- Principe #3 - Contrat minimal : trait `Connector` strictement nécessaire (4 méthodes + types).
- Principe #5 - Un acteur, une responsabilité : `apollia-connectors` = stateless I/O ; tokens vivent dans `apollia-auth` ; registry dans `apollia-tools`.

## Liens

- ADR-088 - Architecture hybride
- ADR-082 - Tool Governance (audit trail des operations)
- Plan : §4
