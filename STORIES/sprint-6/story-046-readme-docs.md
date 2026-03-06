# [SPRINT-6][docs] README + documentation installation

**ID :** STORY-046
**Sprint :** 6
**Crate cible :** racine du projet
**Fichier(s) cible(s) :** `README.md`, `docs/INSTALL.md`
**Taille :** M
**Depend de :** STORY-044 (Agent devis-generator — pour documenter la demo)
**Statut :** ✅ Terminée

---

## User Story

```
En tant que developpeur ou operateur decouvrant Apollia OS,
je veux un README clair et un guide d'installation complet,
afin de pouvoir installer, demarrer et tester le runtime en moins de 10 minutes.
```

---

## Contexte technique

Le projet n'a pas encore de README.md a la racine. Pour la demo client et l'ouverture open-source, un README de qualite est indispensable. Il doit couvrir l'installation, le quickstart avec l'agent de demo, et la reference architecturale.

**Principe(s) architectural(aux) concerne(s) :**
- Principe #2 — Zero dependance externe (le README doit refleter cette simplicite d'installation)
- Principe #8 — CLI humaine, API machine (documenter les deux interfaces)

---

## Criteres d'Acceptation

### AC-1 — README.md a la racine du projet

```
ETANT DONNE un developpeur qui clone le repo
QUAND il ouvre README.md
ALORS il comprend en 30 secondes ce qu'est Apollia OS
ET il trouve les sections : Installation, Quickstart, Architecture, Contributing
```

### AC-2 — Guide d'installation complet

```
ETANT DONNE un developpeur avec Rust et Python installes
QUAND il suit les instructions de docs/INSTALL.md
ALORS il compile le projet et execute l'agent de demo avec succes
```

### AC-3 — Quickstart avec l'agent hello-agent

```
ETANT DONNE le README.md
QUAND le developpeur suit la section Quickstart
ALORS il execute les commandes :
  cargo build --workspace
  apollia-os start
  apollia-os agent start agents/hello_agent.py
  apollia-os run hello-agent "Bonjour"
  apollia-os stop
ET chaque commande fonctionne comme documente
```

### AC-4 — Section architecture avec schema ASCII

```
ETANT DONNE le README.md
QUAND le developpeur lit la section Architecture
ALORS il voit un schema ASCII des 6 briques
ET il comprend le pattern acteur Tokio et le bridge PyO3
```

### AC-5 — Badge de build CI dans le README

```
ETANT DONNE le README.md
QUAND il est affiche sur GitHub
ALORS le badge CI (cargo test + clippy) est visible en haut du fichier
```

---

## Specification technique

### Contenu du README.md

```markdown
# Apollia OS

> Runtime Rust open-source pour agents IA autonomes souverains.
> Local-first. Zero cloud. Un binaire.

[Badge CI] [Badge License]

## Qu'est-ce qu'Apollia OS ?

[3-4 lignes : runtime, agents Python, local-first, PME]

## Quickstart

[5 commandes : build, start, agent start, run, stop]

## Architecture

[Schema ASCII 6 briques, liens vers docs/]

## Ecrire un agent

[Code Python minimal : manifest() + run()]

## CLI Reference

[Tableau des commandes niveau 1 + 2]

## Contributing

[Liens vers CONTRIBUTING.md si existant, ou instructions basiques]

## License

[MIT ou Apache-2.0]
```

### Contenu de docs/INSTALL.md

```markdown
# Installation

## Prerequis
- Rust 1.75+ (rustup)
- Python 3.11+ (pour les agents)
- SQLite 3.35+ (FTS5, generalement inclus)

## Build
cargo build --workspace

## Configuration macOS (PyO3)
export PYO3_PYTHON=/opt/homebrew/bin/python3.13

## Verification
cargo test --workspace
apollia-os --version
```

### Ce que cette story N'implemente PAS

- CONTRIBUTING.md complet (hors scope MVP)
- Documentation API REST detaillee (Swagger/OpenAPI) — hors scope
- Site web de documentation — hors scope
- Traduction en anglais — le README est en anglais, la doc interne en francais

---

## Tests requis

Pas de tests automatises — verification manuelle que les commandes documentees fonctionnent.

---

## Definition of Done

**Documentation :**
- [ ] `README.md` a la racine du projet
- [ ] `docs/INSTALL.md` guide complet
- [ ] Quickstart fonctionnel (commandes testees manuellement)
- [ ] Schema ASCII de l'architecture present

**Commit :**
- [ ] `docs: add README and installation guide`

---

## Liens

- Story precedente : STORY-044 (Agent devis-generator)
- Spec architecture : `docs/Architecture-Vue-Ensemble.md`
- Spec CLI : `docs/Briques-CLI.md`
