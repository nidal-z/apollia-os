# Wiki Apollia OS

> ⚠️ **Ce corpus est en cours de refonte intégrale.**
>
> Le contenu a été produit sprint par sprint et accumulé sans relecture
> systématique. Plusieurs catégories d'imprécisions y subsistent :
>
> - **Références obsolètes** : `STORY-NNN`, `sprint-N`, `[Lot N]` qui ne
>   parlent plus à un lecteur externe.
> - **ADRs supersedés** cités comme actifs (ex. ADR-025 pipelines,
>   superseded fin 2026).
> - **Doublons** : plusieurs pages sur les mêmes sujets (5 pages agents,
>   4 pages MCP, 3 pages installation).
> - **Code path mentions** qui peuvent ne plus correspondre au code
>   actuel.
>
> **Refonte planifiée** dans le sprint L2b, post-stabilisation totale
> (cible été 2026). Détails dans
> `docs/internal/release/DOCS-STATE.md` §2.4.
>
> **En attendant**, traite ce corpus comme une référence à vérifier
> contre le code avant de citer publiquement.

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
