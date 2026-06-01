# Wiki Apollia OS

> Référence technique d'Apollia OS : tables exhaustives, signatures
> complètes, codes d'erreur. Le wiki suit le code, il ne le précède pas.
> En cas de doute, le code et les ADRs font foi.

---

## Audience cible

- Développeur expérimenté qui cherche le détail technique d'une brique
  Apollia.
- Mainteneur qui cherche les spécifications complètes (paramètres,
  schémas, codes d'erreur).

## Format

Référence pure (au sens [Diátaxis](https://diataxis.fr/)) : pas de
tutoriels, pas d'explications pédagogiques, pas d'opinions. Tables
exhaustives, signatures complètes.

Les tutoriels et exemples vivent dans [`../book/`](../book/). Les
décisions vivent dans [`../adr/`](../adr/).

## Navigation

Le wiki contient actuellement 72 pages. Sujets principaux :

- **Architecture** : `Architecture-Vue-Ensemble.md`,
  `Architecture-Principes.md`, `Architecture-Acteurs.md`.
- **Briques** : `Briques-*.md` (Runtime Core, Memory Engine, ORIA, Tool
  Registry, etc.).
- **Agents** : `Agents-*.md` (SDK, manifest, RuntimeContext, etc.).
- **Référence CLI** : `Reference-CLI.md`.
- **Decisions log** : `Decisions-Log.md` (index commenté des ADRs).
- **Specs intégrations** : MCP, OAuth, connecteurs SaaS.

Aucun index automatique pour l'instant. Browse via le système de fichiers.
