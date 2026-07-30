# ADR-040 - Site de documentation adopters (Docusaurus, Diataxis, references generees)

**Date :** 2026-07-08
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** Pré-implémentation (chantier A3)

---

## Contexte

La documentation d'Apollia est aujourd'hui éclatée sur trois corpus sources (`docs/book` mdBook français, `docs/wiki` 72 pages en migration anglaise, `docs/help` 52 pages françaises) et deux sites VitePress (`web/wiki-site`, `web/help-site`), pour environ 170 pages au total. La cartographie du 2026-07-08 (source de vérité) a acté la consolidation sur un site unique orienté adopters.

Deux problèmes structurels motivent la refonte :

1. **Pas d'architecture adopters.** Le contenu existant est organisé par artefact d'origine (book, wiki, help), pas par intention de lecture. Un intégrateur qui découvre Apollia n'a pas de parcours.
2. **Références recopiées à la main.** L'API HTTP, la CLI et le contrat SDK sont documentés en prose statique qui dérive du code réel. Le contrat de pilotage (ADR-037) a livré une spec OpenAPI générée depuis le code, mais aucune doc ne la consomme ; la CLI a une taxonomie clap complète (ADR-034) qu'aucune page ne reflète fidèlement ; le contrat `ctx` (ADR-024) vit dans `sdk/apollia/types.py` et n'est décrit nulle part de façon dérivée.

Contrainte de souveraineté (principe #2) : tout outil de documentation est build-time. Il ne doit rien ajouter au binaire runtime livré.

## Décision

Nous montons un site **Docusaurus** unique à `docs/site/`, structuré selon **Diataxis** (Tutorials, How-to, Reference, Explanation, Architecture, plus un espace Operator help), i18n-ready (`en` adopters par défaut, `fr` pour le help). Les trois références machine sont **générées depuis la source de vérité, jamais recopiées** :

1. **Référence HTTP API** générée de `clients/openapi.json` (la spec d'ADR-037) via `docusaurus-plugin-openapi-docs` + `docusaurus-theme-openapi-docs`. Régénérée au build, gitignorée.
2. **Référence CLI** générée de l'arbre clap du binaire `apollia-os` via `clap-markdown`. Le générateur est exposé par une sous-commande cachée `gen-cli-docs`, **derrière une feature Cargo `gen-docs` désactivée par défaut** : le binaire livré ne contient pas `clap-markdown`. Page committée.
3. **Référence SDK / ctx** dérivée de `sdk/apollia/types.py` et des protocoles `sdk/apollia/context/*.py` par un script Python d'introspection **stdlib `ast` uniquement** (pas d'import, pas de dépendance tierce). Pages committées.

Un script `docs/site/regen.sh` (sur le modèle de `clients/regen.sh`) régénère les trois. Les pages générées portent un en-tête "GENERATED FILE. Do not edit".

Périmètre de cet ADR : l'infrastructure (site, arborescence, pipelines de génération, i18n, build vert). La migration des ~170 pages, l'arc42 complet et la suppression des anciens corpus sont hors périmètre, traités en phases suivantes.

## Alternatives considérées

### Outil : Docusaurus vs Material for MkDocs vs rester VitePress
Retenu : **Docusaurus**. React, i18n natif (dossier `i18n/`), versioning natif, et surtout un écosystème de plugins OpenAPI mûr qui rend la règle source-unique réaliste. `doc-coverage-map.md` mentionnait encore "Material for MkDocs" (reco périmée) ; la décision autoritative (`doc-architecture-target.md`) est Docusaurus. Rester sur VitePress ne résout ni la génération API/CLI/SDK ni la consolidation.

### Génération CLI : `clap-markdown` (feature) vs walker `--help` vs générateur maison
Retenu : **`clap-markdown` derrière une feature `gen-docs`**. Le walker `--help` traite le binaire en boîte noire mais dépend d'un binaire compilé et produit un markdown moins structuré. Un générateur maison qui parcourt `Cli::command()` réinvente `clap-markdown`. La feature Cargo optionnelle garde `clap-markdown` **hors du binaire par défaut** (souveraineté préservée) sans refactor lib/bin du crate `apollia-cli` (binaire-seul). Ce n'est pas du code mort feature-flaggé : c'est de l'outillage de build, comme la feature `cloud` existante.

### Génération SDK : introspection `ast` stdlib vs Sphinx/mkdocstrings vs docstrings recopiées
Retenu : **`ast` stdlib**. Sphinx/mkdocstrings ajouteraient une chaîne Python tierce et impliqueraient d'importer le SDK. `ast` parse le source sans l'exécuter, sans dépendance, cohérent avec la règle "workers/outillage stdlib par défaut".

### Pages générées : committées vs régénérées au build
Retenu : **API régénérée au build (node-only), CLI et SDK committées**. La génération API ne demande que node et la spec committée, donc elle tourne dans le script de build. La CLI et le SDK demandent cargo et python, absents d'un build node pur ; committer leurs pages garde `npm run build` vert partout, et `regen.sh` les rafraîchit (modèle "review the diff, then commit" de `clients/regen.sh`).

## Conséquences

**Positives :**
- Un parcours adopters cohérent (Diataxis) remplace l'organisation par artefact.
- Les trois références ne peuvent plus diverger du code : elles en dérivent.
- La spec OpenAPI d'ADR-037 obtient enfin un consommateur de documentation.
- i18n prêt dès le départ (`en` + `fr`), ce qui débloque la migration du help français et la fin de la migration anglaise du wiki.

**Négatives / Compromis :**
- Nouvelles dépendances build-time du site doc (Docusaurus + plugin OpenAPI), épinglées, hors runtime.
- Une dépendance Cargo optionnelle (`clap-markdown`) entre dans `apollia-cli`, même absente du binaire par défaut : surface de souveraineté à assumer, justifiée ici (règle ASK FIRST).
- Deux corpus documentaires et deux sites VitePress coexistent avec le nouveau site jusqu'à la réconciliation (phase ultérieure).

**Neutres / À surveiller :**
- Les descriptions rustdoc dans la spec OpenAPI produisent quelques liens cassés (avertissements de build non bloquants) : à nettoyer côté annotations `utoipa` plus tard.
- Choix du plugin OpenAPI (`docusaurus-plugin-openapi-docs`) : `redocusaurus` reste le repli si le plugin pose problème.

## Principes architecturaux impactés

- **Principe #2 - Zéro dépendance externe** : toutes les dépendances ajoutées sont build-time ; `clap-markdown` est exclu du binaire livré par la feature `gen-docs`. Conforme.
- **Principe #8 - Human CLI, machine API** : la référence CLI dérivée et la référence API dérivée documentent fidèlement les deux surfaces.
- **Principe #4 - Fail fast** : une référence générée casse au build si le contrat source change de forme, plutôt que de dériver en silence.

## Liens

- ADR liés : ADR-037 (contrat de pilotage, source de l'OpenAPI), ADR-034 (taxonomie CLI, source de l'arbre clap), ADR-024 (contrat runtime du SDK ctx, source de `types.py`)
- Emplacement : `docs/site/` ; régénération : `docs/site/regen.sh`
