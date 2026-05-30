# Annexe D. Roadmap

Cette annexe résume où va Apollia OS après la v0.1.0. Les éléments sont organisés en trois horizons. Les dates sont indicatives et seront raffinées au fil des retours utilisateurs.

---

## Horizon court (v0.2.x, été 2026)

Stabilisation et qualité de vie.

- **Templates `apollia new` raffraîchis.** Les templates actuels portent encore quelques résidus de la v0.4 (cf. [chapitre 28](../part-vii-tooling/28-apollia-new-scaffolding.md)). Un patch dédié alignera les 4 templates sur le canon decorator-first v0.5.
- **Wiki technique publié.** Les pages référencées dans le book (`Briques-SDK`, `Briques-Tool-Registry`, `Briques-MCP-Client`, `Config-apollia-toml`, `API-HTTP-Reference`, `Briques-CLI`, `Briques-ORIA-Engine`, `Briques-HITL-Engine`) seront mises en ligne au fil des semaines suivant la release.
- **ADRs publiées.** Les ADRs 001 à 112 (architecture decisions records) seront accessibles publiquement sur le repo GitHub.
- **`apollia-os migrate` CLI.** Outil pour migrer un agent d'une version mineure à une autre (par exemple `apollia-os migrate v0.1 v0.2 <agent.py>`).
- **`apollia-os eval` framework.** Premier squelette intégré pour les eval suites (cf. [chapitre 26](../part-vi-testing/26-eval-suites.md)). Standardise la structure et le reporting.
- **Vision multimodale.** Le SDK expose déjà des helpers `text()` et `image_from_path()`, mais aucun backend LLM du runtime v0.1 ne sait sérialiser un message contenant une image. Implémentation côté backends Anthropic / OpenAI / Ollama vision en v0.2, puis llama.cpp local quand un modèle GGUF compatible sera packagé.
- **STT amélioré.** Modèles whisper-rs supplémentaires, transcription incrémentale, support de plus de formats audio.

---

## Horizon moyen (v0.3.x à v0.5.x, automne et hiver 2026-2027)

Extensions de surface.

- **Plus de connecteurs natifs.** Gmail, Outlook, Google Drive, OneDrive, Notion, Slack. Aujourd'hui chacun se reconstruit via `web_read` + clés d'API. Demain, intégrations natives avec OAuth flow et cache local.
- **Vector store local.** Aujourd'hui `ctx.memory` est full-text SQLite (FTS5). Ajout d'un index vectoriel local (sqlite-vss ou GGUF embeddings) pour la recherche sémantique sans dépendre d'un service externe.
- **Marketplace d'agents.** Registre communautaire où des auteurs publient des agents, des templates de workspace, des recettes. Installation en une commande : `apollia-os agent install community:<name>`.
- **Multi-utilisateur.** Aujourd'hui chaque installation suppose un opérateur unique. Notion de profils utilisateur dans `governance.db`, permissions par utilisateur, partage d'agents avec ACL.
- **API webhook sortants enrichis.** Notifications via Slack, Discord, Microsoft Teams, en plus des notifications desktop et webhook génériques.
- **Pipeline DAG natif.** Aujourd'hui les pipelines sont déclaratifs côté TOML. Ajout d'une API REST pour les composer dynamiquement, et d'une UI Desktop pour les visualiser.

---

## Horizon long (v1.0 et au-delà, 2027+)

Changements de palier.

- **Clustering multi-nœuds.** Apollia v0.x est single-node. v1.0 introduit la fédération : plusieurs runtimes qui se découvrent et se déléguent des skills. Cas d'usage : un poste de travail développeur + un serveur partagé pour les agents longs.
- **Sandbox renforcée.** Migration vers `nsjail` ou `gVisor` optionnel pour les déploiements production sensibles (PME critiques, organisations sécurité-sensibles).
- **Audit signé.** Chaque entrée d'`audit.db` signée par une clé locale, vérifiable par un tiers (compliance, certifications).
- **LLM local quantifié plus capable.** Quand des modèles GGUF compétitifs (qualité Sonnet sur 32 Go RAM) seront disponibles, le backend par défaut basculera de Haiku-via-API à local-first par défaut.
- **Mode batch.** Aujourd'hui Apollia traite les tâches une par une. Mode batch pour le traitement par lots (mille agents en parallèle, plan d'exécution optimisé).
- **Réplication HA.** Sauvegarde live des bases SQLite vers un nœud de standby, basculement en cas de panne.

---

## Ce qui ne changera pas

- **Local-first et zéro cloud obligatoire.** Le principe fondateur ne bouge pas. Tout ce qui marche aujourd'hui sur une machine isolée continuera de marcher.
- **Zéro dépendance externe runtime.** Le binaire restera autonome. Pas de daemon Postgres / Redis / etc.
- **Le contrat decorator-first du SDK.** Un agent écrit aujourd'hui pour v0.1 fonctionnera en v1.0 (semver respecté). Les évolutions seront additives.
- **Modèle d'erreurs typées.** `DomainError` et `NeedHumanInput` restent les exceptions canoniques.

---

## Comment proposer une évolution

Trois canaux :

1. **GitHub issue** sur le repo `apollia-os` pour les bugs et les feature requests.
2. **GitHub discussion** pour les idées plus ouvertes.
3. **ADR proposal** si l'idée touche à l'architecture : ouvrez une issue avec le préfixe `ADR-proposal:` et un draft d'ADR. Si elle est acceptée, elle devient `ADR-NNN` dans `docs/adr/`.

---

## Suivi des releases

Les releases sont annoncées sur le repo GitHub (tag + release notes). La version est dans `apollia.__version__` côté SDK et dans `apollia-os --version` côté CLI.

Pas de cycle fixe. Une release sort quand un ensemble cohérent de changements est prêt et testé. Tendance : releases mineures toutes les 2-3 semaines après la v0.1.0, jusqu'à stabilisation.
