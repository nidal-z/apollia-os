# Documentation Apollia OS

Cette racine `docs/` regroupe les cinq corpus documentaires d'Apollia. Chaque
corpus a une audience claire et un mode d'écriture précis (au sens
[Diátaxis](https://diataxis.fr/)). Pour ne pas dupliquer, chaque corpus se
limite à son rôle et cite les autres.

> **Documentation publique canonique : `docs/site/`** (Docusaurus, en + fr, Diátaxis). Les corpus
> historiques `docs/book/`, `docs/wiki/` et `docs/help/` sont supersédés et en cours de retrait :
> ne plus y écrire.

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

## Les cinq corpus

### 🛟 [`docs/help/`](./help/) - Aide opérateur

Comment accomplir une tâche depuis l'application Desktop. Français. Un
article = une tâche. Pas de vocabulaire interne (jamais "acteur",
"EventBus", "mpsc"). Captures d'écran quand elles clarifient.

**Audience** : utilisateur final non-développeur.

### 📘 [`docs/book/`](./book/) - Pédagogique mdBook + guide SDK Python complet

"Le Rust Book mais pour Apollia". 49 chapitres, 9 parties, français. Build :
`mdbook build docs/book/`.

**C'est le guide de référence pour utiliser le SDK Python Apollia AgentKit.**
Au programme :

- **Partie I** : installation + 4 quickstarts (agent conversationnel, worker,
  director, orchestré).
- **Partie II** : les décorateurs (`@agent`, `@skill`, `@on_message`,
  `@orchestrated`).
- **Partie III** : le protocole `Ctx` et ses 14 services
  (llm, memory, tools, a2a, datasources, templates, secrets, events,
  logger, profile, workspace, stt, notify, budget).
- **Partie IV** : design LLM-friendly (`Annotated` descriptions, examples,
  schémas `TypedDict`).
- **Partie V** : gestion des erreurs (`DomainError`, `NeedHumanInput`).
- **Partie VI** : tests (mock, assertions, eval suites).
- **Partie VII** : outillage (`apollia inspect`, `apollia new`).
- **Partie VIII** : vue d'ensemble du runtime Rust côté dev externe.
- **Partie IX** : projet capstone (multi-agent end-to-end).
- **Annexes A-G** : diagrammes, glossaire, principes, roadmap, vision,
  index ADRs, FAQ.

Le book ne **duplique jamais** une table de référence présente dans le
wiki. Il lie. Pattern : `> **Référence technique :**
[Nom-Page](URL-wiki)`.

Point d'entrée pour un dev externe : commence par
[`sdk/README.md`](../sdk/README.md) (quickstart 1 page), puis ouvre le
book.

**Audience** : développeur qui découvre Apollia ou qui code des agents
Python avec le SDK.

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

## Convention de mise à jour

| Type de changement | Corpus à mettre à jour |
|---|---|
| Nouvelle API publique | `docs/wiki/` (référence) + doc-comment Rust (rustdoc) + éventuellement `docs/book/` (chapitre) |
| Nouvelle commande CLI | `crates/apollia-cli/AGENTS.md` + `docs/wiki/Reference-CLI.md` + `docs/book/` si user-facing |
| Décision architecturale | `docs/adr/ADR-NNN.md` (nouveau) |
| Nouveau pattern de code | `docs/agents/<thématique>.md` |
| Changement opérateur visible | `docs/help/` + chapitre `docs/book/` |

Détails dans [`docs/agents/DOCS-WRITING.md`](./agents/DOCS-WRITING.md).
