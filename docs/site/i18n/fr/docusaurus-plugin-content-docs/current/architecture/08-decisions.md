---
sidebar_position: 8
title: 8. Décisions d'architecture
---

# 8. Décisions d'architecture

Les choix structurants sont consignés sous forme de fiches de décision
d'architecture numérotées. Cette page synthétise celles qui façonnent le plus
le système ; chacune est citée par son identifiant nu. Les fiches complètes
vivent dans le journal des décisions du projet.

## Les décisions les plus structurantes

- **ADR-001, fondations et pile technique.** Rust associé à Tokio pour le
  runtime, PyO3 pour le pont Python, `llama.cpp` pour l'inférence locale,
  SQLite pour la persistance. Cela fixe dès la base la posture de
  souveraineté et de zéro dépendance.
- **ADR-002, pont PyO3 et découplage par traits.** Les agents sont en Python
  derrière un pont qui expose les services via des traits Rust, ce qui
  découple le contrat de l'agent de son implémentation et le rend simulable
  en test.
- **ADR-005, modèle d'exécution ORIA.** Le moteur autonome : une boucle
  ReAct en modes direct et orchestré, sur le noyau d'acteurs Tokio.
- **ADR-007, inférence en sidecar multi-runner.** L'inférence locale
  s'exécute dans un processus runner séparé et supervisé, ce qui isole les
  plantages du modèle du démon.
- **ADR-015, gouvernance des permissions et des outils.** Le moteur de
  permissions, les scopes, et le chemin d'outil gouverné par lequel passe
  chaque appel d'outil.
- **ADR-037, contrat de pilotage hôte.** Une surface OpenAPI générée et
  versionnée, plus des SDK hôtes TypeScript et Python, pour qu'un produit
  hôte pilote le runtime sans avoir à le rétro-ingénierer. C'est le produit
  d'intégration dont la tête de pont avait besoin.
- **ADR-038, arguments d'étape orchestrés.** Un contrat hybride : le
  raisonneur remplit les arguments d'étape structurés en GBNF au moment de
  la planification, avec une extraction juste-à-temps en repli à
  l'exécution. C'est ce qui permet au chemin orchestré de piloter de vrais
  outils natifs avec des arguments structurés.
- **ADR-039, vérification et critique sur le chemin orchestré.** Une
  exécution orchestrée terminée est vérifiée par un critique, le verdict
  est audité comme un événement runtime, et un verdict d'échec déclenche
  une replanification bornée sous le budget partagé, conditionnée par le
  palier d'autonomie.

## Une décision de ne pas construire

- **Le replay a été abandonné (2026-07-08).** Réexécuter une exécution et
  la comparer a été jugé sans valeur fonctionnelle ou réglementaire
  suffisante au regard de son coût. L'imputabilité repose sur le journal
  signé et la vérification, pas sur le replay. Cette décision est
  consignée ici pour que son absence se lise comme un choix, pas comme un
  manque. L'audit de construction de plan associé est ADR-033.

## Décisions complémentaires

La taxonomie CLI et la surface native pour l'IA (ADR-034, ADR-035,
ADR-036), l'architecture de mémoire et de contexte (ADR-010), l'humain
dans la boucle (ADR-013), les secrets et l'authentification API
(ADR-016), le client MCP et ses transports (ADR-017, ADR-018), les
connecteurs (ADR-019), l'application desktop (ADR-020), le SDK et le
routage A2A (ADR-023, ADR-024, ADR-025), ainsi que le modèle de plan
unifié et le moteur de plan natif pour le chat (ADR-031, ADR-032)
complètent le journal des décisions.
