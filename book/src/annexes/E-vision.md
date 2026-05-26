# Annexe E. Vision et positionnement

Pourquoi Apollia OS existe, pour qui, et où il se situe dans le paysage des outils IA actuels.

---

## Le problème

Les agents IA autonomes deviennent utiles : un agent qui prépare un RDV commercial, un agent qui trie des emails, un agent qui audite un PDF, un agent qui surveille un sujet de veille. Le code Python pour les écrire existe (LangGraph, CrewAI, AutoGen, ou maison).

Ce qui manque, c'est **la couche d'exécution** entre l'agent et la production :

- Comment isoler un agent buggé pour qu'il ne casse pas les autres ?
- Comment garantir qu'un agent en boucle infinie ne consomme pas $1000 de LLM ?
- Comment garder les données du client chez le client (RGPD, PME, données sensibles) ?
- Comment offrir une UI à un opérateur non-développeur ?
- Comment auditer ce que les agents ont fait, pour la conformité ?

Les solutions actuelles couvrent mal cette couche :

- Les **frameworks** (LangGraph, CrewAI) sont des bibliothèques. Ils n'isolent rien, ne tracent rien, n'offrent pas d'UI.
- Les **SaaS** (LangServe Cloud, Modal, Dify) gèrent l'infra, mais les données transitent par leurs serveurs.
- L'**auto-construction** (Docker + n8n + LiteLLM + base SQL + UI custom) demande un investissement infrastructure que la majorité des prestataires PME n'ont pas le temps de faire.

---

## Le positionnement

**Apollia OS est un runtime Rust local-first pour exécuter des agents IA autonomes sur la machine de l'utilisateur final.** Un seul binaire, aucune dépendance externe, du démarrage à la production.

Trois métriques de différenciation :

1. **Local par construction.** Pas de cloud dans le chemin d'exécution, sauf si l'opérateur configure un backend LLM cloud. Les données utilisateur, la mémoire, l'audit trail vivent sur la machine.
2. **Sandbox sans Docker.** Linux user namespaces natifs. Isolation des outils sans daemon ni installation préalable.
3. **Decorator-first et auto-instancié.** Le SDK Python a la même ergonomie que FastAPI ou Pydantic. Un agent fonctionnel en 30 lignes, testable sans démarrer le runtime.

---

## Pour qui

### Les prestataires d'agents IA sur mesure

Vous facturez la création d'agents personnalisés pour des PME. Vos clients veulent automatiser des workflows métier (compta, support, veille, commercial). Apollia est votre stack d'exécution : vous écrivez la logique, le runtime gère le reste.

Le modèle de monétisation principal d'Apollia est ce segment. La doc, les patterns, le capstone du book sont conçus pour vous.

### Les développeurs Python qui veulent du local

Vous avez un agent Python qui marche en local. Vous voulez le déployer en production sans passer par un cloud, sans installer un cluster, sans devenir SRE. Apollia fait tourner votre agent en quelques commandes.

### Les organisations sensibles aux données

PME européennes (RGPD), cabinets juridiques (secret professionnel), structures de santé (données médicales), entreprises à propriété intellectuelle critique. Le runtime local-first est une réponse architecturale, pas seulement contractuelle.

### Les contributeurs open-source

Apollia est sous licence MIT. Le code est en Rust avec un SDK Python. Les contributions sont les bienvenues : nouveaux outils natifs, nouveaux backends LLM, nouveaux templates, nouveaux connecteurs.

---

## Pour qui ce n'est pas

- **Pas pour les SaaS B2C grand public.** Si votre produit doit servir 1 million d'utilisateurs en simultané, vous voulez un cluster cloud, pas un runtime local.
- **Pas pour les workflows déterministes.** Si vos workflows sont des chaînes de prompts sans raisonnement réel, un orchestrateur classique (Airflow, n8n, Dagster) sera plus adapté. Apollia mise sur l'agent autonome avec ReAct.
- **Pas pour la recherche d'algorithmes d'agents.** Si vous voulez expérimenter des nouvelles boucles de raisonnement, des architectures multi-agents exotiques, des protocoles d'apprentissage en ligne, c'est plutôt LangGraph ou un framework de recherche. Apollia est un runtime d'exécution, pas un framework de raisonnement.

---

## Les 8 principes

Cf. [Annexe C](C-principles.md) pour la version détaillée.

1. **Local-first, toujours.** Aucun octet de données utilisateur ne quitte la machine sans action explicite.
2. **Zéro dépendance externe.** Le binaire fonctionne sur tout Linux sans installation préalable.
3. **Contrat minimal, friction zéro.** Un agent existant doit tourner dans Apollia avec moins de 10 lignes d'adaptation.
4. **Fail fast.** Toute erreur détectable au démarrage est détectée au démarrage.
5. **Un acteur, une responsabilité.** Pattern acteur Tokio, zéro état partagé.
6. **Mémoire à initiative de l'agent.** Jamais d'injection automatique de contexte mémoriel.
7. **Garde-fous non négociables.** StepBudget appliqué par le runtime, non contournable.
8. **CLI humaine, API machine.** `--json` global, TTY auto-détecté.

Ces principes sont les invariants. Toute évolution les respecte.

---

## La vision long terme

Apollia OS doit devenir la couche d'exécution standard des prestations d'agents IA pour PME. À horizon 3 ans :

- Un écosystème de prestataires qui livrent des agents Apollia clé en main à leurs clients.
- Un marketplace d'agents et de templates open-source réutilisables.
- Une communauté qui contribue les connecteurs (HubSpot, Salesforce, NetSuite, Sage, etc.) en natif.
- Une certification facultative (Apollia-Certified Agent) qui valide qu'un agent respecte les bonnes pratiques de sécurité et d'observabilité.

L'objectif n'est pas de remplacer LangGraph, CrewAI, ou les LLM providers. C'est de fournir la couche en dessous que personne ne veut écrire : sandbox, observabilité, persistance, HITL, audit, packaging, UI opérateur.

Si dans 3 ans, un prestataire PME peut livrer un agent sur mesure en 3 jours au lieu de 3 semaines parce qu'Apollia existe, le pari est gagné.
