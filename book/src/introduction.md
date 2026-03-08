# Apollia OS

**Runtime Rust open-source pour l'exécution souveraine d'agents IA autonomes.**

Apollia OS permet à n'importe quel agent Python (LangGraph, CrewAI, custom)
de s'exécuter de manière isolée, locale, et outillée — sans dépendance cloud.

```bash
$ cargo install apollia-os
$ apollia-os start
$ apollia-os agent start ./hello_agent.py
$ apollia-os run hello-agent "Bonjour Apollia"
✓ Terminé en 0.3s — Bonjour Apollia !
```

## Démarrage rapide

- [Installation en 5 minutes](./quickstart/install.md)
- [Votre premier agent](./quickstart/hello-agent.md)

## Architecture

- [Vue d'ensemble](./architecture/overview.md)
- [Modèle acteur Tokio](./architecture/actor-model.md)
- [Diagrammes](./architecture/diagrams/index.md)

## Référence

- [API HTTP](./api/http-reference.md)
- [Décisions architecturales](./decisions/index.md)

---

> **Local-first.** Zéro octet de données utilisateur ne quitte la machine
> sans action explicite. [Licence Apache 2.0](https://github.com/nidal-z/apollia-os)
