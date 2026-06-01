# Documentation Apollia OS

Cette racine `docs/` regroupe les cinq corpus documentaires d'Apollia. Chaque
corpus a une audience claire et un mode d'écriture précis (au sens
[Diátaxis](https://diataxis.fr/)). Pour ne pas dupliquer, chaque corpus se
limite à son rôle et cite les autres.

---

## Matrice Persona × Sujet

| Tu es… | Tu cherches à… | Va dans |
|---|---|---|
| **Opérateur** (utilisateur Desktop, non-développeur) | accomplir une tâche dans l'app | [`docs/help/`](./help/) |
| **Développeur découvrant Apollia** | apprendre en faisant, exemples concrets | [`docs/book/`](./book/) |
| **Développeur expérimenté** | référence technique exhaustive | [`docs/wiki/`](./wiki/) (⚠️ refonte en cours, voir bannière) |
| **Mainteneur** | comprendre une décision passée | [`docs/adr/`](./adr/) |
| **LLM / agent IA** | règles de code, conventions, patterns | [`docs/agents/`](./agents/) |
| **Toi en R&D** (consultation hors-code) | concepts digestibles avec analogies | Notion `PORTAIL TECH` (privé) |

---

## Les cinq corpus

### 🛟 [`docs/help/`](./help/) - Aide opérateur

Comment accomplir une tâche depuis l'application Desktop. Français. Un
article = une tâche. Pas de vocabulaire interne (jamais "acteur",
"EventBus", "mpsc"). Captures d'écran quand elles clarifient.

**Audience** : utilisateur final non-développeur.

### 📘 [`docs/book/`](./book/) - Pédagogique mdBook

"Le Rust Book mais pour Apollia". Apprendre en faisant, 1-2 exemples par
concept, progression chapitres. Français. Build : `mdbook build docs/book/`.

Le book ne **duplique jamais** une table de référence présente dans le
wiki. Il lie. Pattern : `> **Référence technique :**
[Nom-Page](URL-wiki)`.

**Audience** : développeur qui découvre Apollia.

### 📚 [`docs/wiki/`](./wiki/) - Référence technique (refonte en cours)

L'équivalent docs.rs pour Apollia : specs complètes, tables de paramètres,
codes d'erreur, signatures. Pas de tutoriel, pas d'opinion.

> ⚠️ **En cours de refonte intégrale**. Le contenu actuel a été produit
> sprint par sprint et contient des références obsolètes (`STORY-NNN`,
> `sprint-N`, ADRs superseded). Refonte planifiée pour le sprint L2b
> post-stabilisation. Voir [`docs/wiki/README.md`](./wiki/README.md).

**Audience** : développeur expérimenté qui cherche le détail technique.

### 🗝️ [`docs/adr/`](./adr/) - Architecture Decision Records

Les décisions architecturales passées. Une par fichier (`ADR-NNN-...md`).
Format : Context / Decision / Consequences / Alternatives. Statut en tête
(Proposed / Accepted / Deprecated / Superseded). Append-only : on n'édite
pas un ADR accepté, on en écrit un nouveau qui le supersede.

**Audience** : mainteneur, lecteur curieux du "pourquoi".

### 🤖 [`docs/agents/`](./agents/) - Règles pour LLM et agents IA

Le rulebook pour tout LLM qui code Apollia (Claude Code, Codex, Cursor,
Aider, etc.). Anglais. Format AGENTS.md standard (Linux Foundation). Ton
impératif. Voir le point d'entrée [`AGENTS.md`](../AGENTS.md) à la racine
du repo et [`docs/agents/INDEX.md`](./agents/INDEX.md) pour la navigation.

**Audience** : agents IA + développeurs qui veulent connaître les
conventions internes.

---

## Ce qui n'est PAS ici

- **Documentation interne sprint / release** : `docs/internal/`
  (gitignored, reste local).
- **Specs en chantier** : `docs/specs/` (gitignored).
- **Atlas conceptuel R&D personnel** : Notion `PORTAIL TECH` (privé,
  consulté par le mainteneur en sessions R&D).

---

## Convention de mise à jour

| Type de changement | Corpus à mettre à jour |
|---|---|
| Nouvelle API publique | `docs/wiki/` (référence) + doc-comment Rust (rustdoc) + éventuellement `docs/book/` (chapitre) |
| Nouvelle commande CLI | `crates/apollia-cli/AGENTS.md` + `docs/wiki/Reference-CLI.md` + `docs/book/` si user-facing |
| Décision architecturale | `docs/adr/ADR-NNN.md` (nouveau) |
| Nouveau pattern de code | `docs/agents/<thématique>.md` |
| Changement opérateur visible | `docs/help/` + chapitre `docs/book/` |

Détails dans [`docs/agents/DOCS-WRITING.md`](./agents/DOCS-WRITING.md).
