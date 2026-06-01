# ADR-091 - Catalogue MCP : statique → registry → marketplace + override user-side

**Date :** 2026-05-12
**Statut :** Proposé
**Sprint :** Pré-implémentation (chantier Connecteurs & MCP v0.1.0)

---

## Contexte

Le catalogue MCP actuel est un fichier statique `crates/apollia-desktop/src/mcp/enrichments.json` de 5 entrées hard-codées. L'UI promet "filtrez par catégorie, cherchez" - promesse cassée. À moyen terme, Apollia vise une parité partielle avec Claude (375+ connecteurs) sans tout reconstruire en v0.1.0.

Trois trajectoires possibles, qui doivent être **compatibles entre elles** (le schéma d'entrée doit rester stable cross-versions pour éviter de réécrire le catalogue à chaque palier).

## Décision

Nous adoptons une **roadmap en 3 paliers** avec un schéma d'entrée stable dès v0.1.0 :

- **v0.1.0 (livré)** - Catalogue **statique enrichi** : 18 entrées curées dans `enrichments.json`. Chaque entrée porte `cost_model`, `trust_level`, `auth`, `transport`. Override user-side via `~/.apollia/mcp-overrides.json` (`add` / `disable` / `override`).
- **v0.3 (roadmap)** - **Registry remote dynamique** : repo public `Apollia-OS/apollia-mcp-registry` contenant `registry.json` au même schéma. Desktop sync au démarrage + cache local + SHA-256.
- **v0.4+ (roadmap)** - **Marketplace communautaire signé** : submissions via formulaire → review humain → signature Apollia → publication.

Le schéma JSON d'entrée est versionné et stable cross-paliers (cf. plan §6.1).

## Alternatives considérées

### Option A - Marketplace dès v0.1.0 (rejetée)
**Pour :** parité Claude immédiate.
**Contre :** infrastructure review + signing + governance hors-scope v0.1.0. Distrait de la priorité (connecteurs natifs).

### Option B - Catalogue dynamique fetch GitHub dès v0.1.0 (rejetée)
**Pour :** zéro release Apollia pour ajouter une entrée.
**Contre :** dépendance réseau au boot. Pas de fallback hors-ligne maîtrisé. Hors-scope sans clarifier la gouvernance du registry.

### Option C - Catalogue statique sans override user-side (rejetée)
**Pour :** simple, pas d'I/O fichier au boot.
**Contre :** un power user ne peut pas patcher sans attendre une release Apollia. Anti-cible (cf. ADR positioning power user).

### Option retenue - Statique enrichi + override user-side, schéma versionné pour migration
**Pour :** livrable v0.1.0 propre. Override permet patches power user immédiats. Schéma stable = pas de réécriture en v0.3.
**Compromis acceptés :** ajout d'une entrée officielle exige une release Apollia (acceptable v0.1.0).

## Conséquences

**Positives :**
- Power users peuvent ajouter MCP internes via `~/.apollia/mcp-overrides.json` (trust level `self_hosted`).
- Migration v0.3 vers registry remote = `IntegrationCatalogProvider` interface, swap d'implémentation.
- Catalogue v0.1.0 honnête : 18 entrées vérifiées factuellement, badge `cost_model` visible.

**Négatives / Compromis :**
- Ajout d'une nouvelle entrée officielle = PR + release Apollia jusqu'en v0.3.

**À surveiller :**
- Charge sur le mainteneur du catalogue (Nidal) à mesure que les MCP officiels SaaS s'ajoutent.

## Principes architecturaux impactés

- Principe #3 - Contrat minimal : schéma entrée minimal mais extensible.
- Principe #8 - CLI humaine, API machine : `apollia mcp catalog list` exposé.

## Liens

- ADR-088 - Architecture hybride
- Plan : §6
