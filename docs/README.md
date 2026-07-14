# Documentation Apollia OS

Cette racine `docs/` regroupe la documentation d'Apollia. La documentation
publique canonique vit dans `docs/site/` (Docusaurus, en + fr), structurée
selon [Diátaxis](https://diataxis.fr/). Deux corpus committés l'accompagnent :
les décisions (`docs/adr/`) et le rulebook pour agents IA (`docs/agents/`).

---

## Matrice Persona × Sujet

| Tu es… | Tu cherches à… | Va dans |
|---|---|---|
| **Opérateur** (utilisateur Desktop, non-développeur) | accomplir une tâche dans l'app | [`docs/site/` operator-help](./site/docs/operator-help/) |
| **Développeur découvrant Apollia ou utilisant le SDK Python** | apprendre en faisant, exemples concrets | [`docs/site/` tutorials + how-to](./site/docs/tutorials/) |
| **Développeur expérimenté** | référence technique | [`docs/site/` reference](./site/docs/reference/) |
| **Mainteneur** | comprendre une décision passée | [`docs/adr/`](./adr/) |
| **LLM / agent IA** | règles de code, conventions, patterns | [`docs/agents/`](./agents/) |
| **Toi en R&D** (consultation hors-code) | concepts digestibles avec analogies | Notion `PORTAIL TECH` (privé) |

---

## Les corpus

### 🌐 [`docs/site/`](./site/) - Documentation publique (Docusaurus)

La doc publique canonique, en + fr, structurée Diátaxis :

- **operator-help** : comment accomplir une tâche depuis l'app Desktop. Un
  article = une tâche, sans vocabulaire interne.
- **tutorials + how-to** : apprendre le SDK Python en faisant (quickstarts,
  déploiement, tests, HITL, packaging d'agents).
- **reference** : specs complètes générées depuis le code (CLI clap, API
  OpenAPI, contrat `Ctx` SDK).
- **explanation** : concepts et décisions de fond (les 8 principes, etc.).

Build : `cd docs/site && npm run build` (garde-fou `onBrokenLinks: throw`).
Les références machine (CLI / API / SDK) se régénèrent via `docs/site/regen.sh`.

### 🗝️ [`docs/adr/`](./adr/) - Architecture Decision Records

Les décisions architecturales passées. Une par fichier (`ADR-NNN-...md`).
Format : Context / Decision / Consequences / Alternatives. Statut en tête
(Proposed / Accepted / Deprecated / Superseded). Append-only : on n'édite pas
un ADR accepté, on en écrit un nouveau qui le supersede. Anglais.

**Audience** : mainteneur, lecteur curieux du "pourquoi".

### 🤖 [`docs/agents/`](./agents/) - Règles pour LLM et agents IA

Le rulebook pour tout LLM qui code Apollia (Claude Code, Codex, Cursor, Aider,
etc.). Anglais. Format AGENTS.md standard. Ton impératif. Point d'entrée
[`AGENTS.md`](../AGENTS.md) à la racine + [`docs/agents/INDEX.md`](./agents/INDEX.md)
pour la navigation.

**Audience** : agents IA + développeurs qui veulent connaître les conventions
internes.

---

## Convention de mise à jour

| Type de changement | Où mettre à jour |
|---|---|
| Nouvelle API publique | `docs/site/docs/reference/` + doc-comment Rust (rustdoc) |
| Nouvelle commande CLI | `crates/apollia-cli/AGENTS.md` + `docs/site/docs/reference/cli/` (régénéré) |
| Décision architecturale | `docs/adr/ADR-NNN.md` (nouveau) |
| Nouveau pattern de code | `docs/agents/<thématique>.md` |
| Changement opérateur visible | `docs/site/docs/operator-help/` + tutorial |

Détails dans [`docs/agents/DOCS-WRITING.md`](./agents/DOCS-WRITING.md).
