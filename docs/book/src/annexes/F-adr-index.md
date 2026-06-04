# Annexe F. Index des ADRs

Les **Architecture Decision Records** (ADRs) documentent chaque décision architecturale significative d'Apollia OS : pourquoi nous avons choisi cette approche, quelles alternatives ont été considérées, et quelles conséquences l'accompagnent.

Au moment de la sortie de la v0.1.0, les ADRs ne sont pas encore publiées en ligne. Elles seront mises en ligne dans une révision proche (cf. l'encadré "ADRs et wiki" en introduction du book). Cette annexe liste les ADRs les plus structurantes, groupées par thème, pour servir de table d'orientation.

---

## Fondations runtime

- `ADR-001` : vision et fondations de la stack (Rust, Tokio, SQLite, contrat AIP).
- `ADR-002` : bridge PyO3 et découplage par traits entre runtime et Python.
- `ADR-003` : sandbox, modèle de confiance des agents et périmètre des plateformes.
- `ADR-004` : conception de la CLI (noun-verb, commande `inspect`).

---

## Moteur et exécution

- `ADR-005` : modèle d'exécution ORIA (modes direct et orchestré).
- `ADR-006` : sous-système d'outils et outils natifs.
- `ADR-007` : runtime d'inférence en sidecar multi-runner.
- `ADR-008` : backends LLM, gestion des modèles et transparence.
- `ADR-009` : moteur de reconnaissance vocale (speech-to-text).

---

## État et gouvernance

- `ADR-010` : architecture mémoire et assemblage de contexte.
- `ADR-011` : profil utilisateur canonique.
- `ADR-012` : observabilité et feedback sur les plans.
- `ADR-013` : human-in-the-loop (HITL).
- `ADR-014` : config opérationnelle, triggers et notifications.
- `ADR-015` : gouvernance des permissions et des outils.
- `ADR-016` : secrets, stockage keyring et authentification de l'API locale.

---

## Connectivité MCP et connecteurs

- `ADR-017` : client MCP, transport et mode serveur.
- `ADR-018` : client OAuth MCP et orchestration.
- `ADR-019` : connecteurs natifs et intégrations.

---

## Desktop et frontend

- `ADR-020` : architecture de l'application desktop.
- `ADR-021` : design system frontend et internationalisation (i18n).
- `ADR-022` : sous-système de chat.

---

## SDK et agents

- `ADR-023` : conception du SDK Python / AgentKit.
- `ADR-024` : contrat runtime du SDK (`ctx`).
- `ADR-025` : worker agents et routing A2A.
- `ADR-026` : installation des agents, format de bundle et distribution.
- `ADR-027` : agent d'onboarding.
- `ADR-028` : distribution de release, updater et signature de code.

---

## Comment lire une ADR

Chaque ADR suit la même structure :

1. **Contexte.** Le problème observé et son ampleur (LOC, agents impactés, etc.).
2. **Décision.** Ce qu'on a choisi de faire, en une phrase, suivi du détail.
3. **Alternatives considérées.** 2 ou 3 options rejetées, avec pour chacune les pour et les contre.
4. **Conséquences.** Positives, négatives, et choses à surveiller.
5. **Principes impactés.** Quels principes architecturaux (parmi les 8) sont touchés.
6. **Liens.** ADRs reliées.

Quand les ADRs seront publiées, vous pourrez les lire directement depuis le dossier `docs/adr/` du repo. En attendant, cette table d'orientation vous indique les thèmes couverts.

---

## Comment proposer une ADR

Cf. [Annexe D (Roadmap)](D-roadmap.md), section "Comment proposer une évolution".
