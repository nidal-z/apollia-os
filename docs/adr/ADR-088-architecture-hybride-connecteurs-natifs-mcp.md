# ADR-088 - Architecture hybride : connecteurs natifs + MCP officiels

**Date :** 2026-05-12
**Statut :** Proposé
**Sprint :** Pré-implémentation (chantier Connecteurs & MCP v0.1.0)

---

## Contexte

Apollia OS v0.1.0 doit matérialiser sa promesse local-first ("vos données restent sur votre machine, l'agent s'y connecte vraiment") avec un parc de connecteurs utilisables en production par les power users. État actuel : zéro connecteur natif, catalogue MCP statique de 5 entrées hard-codées.

Deux stratégies absolutes sont possibles : (a) tout passer par MCP communautaire (modèle Cursor / Cline), (b) tout coder en natif Rust (modèle Dust 55 connecteurs propriétaires). La frontière qui pilote ce choix est économique : certains SaaS maintiennent **leur propre MCP officiel** (Notion, Slack, Atlassian, Linear, GitHub, Stripe, Figma, Sentry, Cloudflare) → utilisables gratuitement avec maintenance externalisée ; d'autres **n'ont pas de MCP officiel maintenu par le SaaS** (Google Workspace, Microsoft 365, Salesforce, HubSpot) → la seule voie est un connecteur natif maison.

## Décision

Nous adoptons une **architecture hybride** : connecteurs **natifs** Rust pour Google Workspace + Microsoft 365 (cf. ADR-090 pour l'abstraction `Connector` trait), et **MCP officiels** intégrés au catalogue (cf. ADR-091) pour tous les SaaS qui en publient un. Salesforce/HubSpot reportés post-v0.1.0.

## Alternatives considérées

### Option A - Tout MCP communautaire (rejetée)
**Pour :** zéro code Rust à maintenir.
**Contre :** dépendance totale qualité MCP communautaire (variable). Aucun MCP officiel SaaS pour Google/Microsoft → workflow "mail / agenda" non-tenable de bout en bout.

### Option B - Tout natif Rust (rejetée)
**Pour :** contrôle total qualité.
**Contre :** maintenance prohibitive (15-20 SaaS × API breaking changes). Re-inventer ce que les éditeurs SaaS publient déjà gratuitement (Notion MCP, Slack MCP, etc.).

### Option C - Aggregator cloud (Composio) (rejetée)
**Pour :** 500+ apps via un endpoint unique managé.
**Contre :** dépendance cloud propriétaire payante. Anti-local-first par construction (relai externe).

### Option retenue - Hybride natif + MCP officiel
**Pour :** maximise gratuité (user et Apollia), externalise la maintenance là où le SaaS la prend en charge, garde le contrôle sur les workflows critiques (mail / agenda / fichiers).
**Compromis acceptés :** maintenance OAuth Rust côté Google + Microsoft (dette long-terme).

## Conséquences

**Positives :**
- Crate `apollia-connectors` reste minimal (2 providers actifs en v0.1.0).
- Catalogue MCP grandit librement (16/18 SaaS officiels en v1, jusqu'à 50+ via registry v2).
- Promesse local-first tenue : aucun relai cloud propriétaire.

**Négatives / Compromis :**
- API Google + Microsoft Graph subissent des breaking changes : tests intégration mensuels requis.
- Asymétrie scope Google (CASA Tier 2 hors v0.1.0) → Gmail send-only en gratuit.

**À surveiller :**
- Apparition d'un MCP Google officiel ou MCP Microsoft officiel → permettrait d'externaliser la maintenance et retirer le code natif.

## Principes architecturaux impactés

- Principe #1 - Local-first : ✅ renforcé (zéro relai cloud).
- Principe #2 - Zéro dépendance externe : ✅ maintenu (pas d'aggregator).
- Principe #3 - Contrat minimal : trait `Connector` doit rester thin.

## Liens

- ADR-090 - Abstraction `Connector` trait
- ADR-091 - Stratégie catalogue MCP
- ADR-064 - OAuth2 PKCE keyring (étendu)
- Plan : `~/.claude/plans/j-aimerai-que-tu-m-aides-melodic-wirth.md` §1 et §4
