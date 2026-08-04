---
sidebar_position: 4
title: 4. Stratégie de solution
---

# 4. Stratégie de solution

Cinq décisions structurantes traduisent les objectifs de qualité dans le code.
Chacune est consignée sous forme de décision d'architecture ; cette page
énonce le choix retenu et son raisonnement, puis renvoie vers la suite.

## Acteurs Tokio à passage de messages, sans état partagé

Le cœur du runtime est un ensemble d'acteurs Tokio. Chaque acteur possède son
état de façon exclusive et ne communique qu'au travers de canaux `mpsc`
bornés, exposés derrière un handle clonable. Aucun `Arc<Mutex<T>>` n'est
partagé entre acteurs. Cela rend structurellement impossible pour un acteur de
modifier l'état d'un autre, ce qui transforme le principe « un acteur, une
responsabilité » en propriété imposée par la structure du code plutôt qu'en
simple convention. Le prix à payer est davantage de plomberie de messages ; le
gain est l'absence de toute une classe d'interblocages asynchrones et de
courses de données.

Voir ADR-005 pour le modèle d'exécution ORIA qui s'appuie sur cette base.

## L'inférence comme sidecar supervisé

L'inférence LLM locale s'exécute dans un processus séparé, pas dans le
daemon : le `llama-server` embarqué (upstream llama.cpp), que le daemon lance
et avec lequel il dialogue via son API HTTP compatible OpenAI. L'appel
d'outils natif est piloté par le template de chat du modèle (`--jinja`), pas
par un chemin de grammaire personnalisé. Isoler l'inférence évite qu'un
plantage du modèle ou une saturation mémoire n'emporte le runtime, et
cantonne le dialogue entre le runtime et le moteur à une interface étroite.
Le batching continu décode plusieurs requêtes dans une même passe, et suivre
l'upstream llama.cpp élargit la gamme d'architectures de modèles prises en
charge.

La supervision actuelle assume ses limites : le daemon lance le processus
d'inférence, mais la surveillance de santé et le redémarrage automatiques ne
sont pas encore câblés. Voir
[Risques et dette technique](/architecture/risks-and-technical-debt). La
décision elle-même est ADR-007.

## Un pont PyO3 avec des services découplés par traits

Les agents sont écrits en Python ; le runtime est écrit en Rust. Le pont est
PyO3, avec `pyo3-async-runtimes` pour l'interopérabilité asynchrone. Le
runtime ne livre pas ses structures internes brutes à Python : il expose un
ensemble de services (l'objet `ctx`) derrière des traits Rust, ce qui découple
le contrat de l'agent de son implémentation et le rend simulable (mock) pour
les tests. Côté agent, on ne voit qu'un contexte typé regroupant quinze
services ; côté Rust, l'implémentation peut évoluer derrière cette façade.

La décision relative au pont est ADR-002 ; le contrat `ctx` est ADR-024 et
est documenté dans la [référence du SDK](/reference/sdk).

## Un contrat machine pour l'intégration hôte

Le runtime est conçu pour être piloté par un produit hôte : sa surface HTTP
est donc un contrat stable de premier ordre, pas une réflexion après coup. La
spécification OpenAPI est générée à partir du code, servie par le daemon, et
versionnée (`/api/v1`, les changements cassants étant réservés à un futur
`/api/v2`). Des SDK hôtes typés en TypeScript et en Python sont générés à
partir de cette spécification. Un intégrateur pilote un daemon réel sans
avoir à rétro-ingénierer quoi que ce soit.

C'est l'objet d'ADR-037. La surface générée est la
[référence de l'API HTTP](/reference/api/apollia-os-runtime-api) ; le guide
pratique est
[Intégrer via le contrat pilote](/how-to/integrate-via-driving-contract).

## La gouvernance vit dans le runtime, pas dans l'agent

Les permissions, le journal d'audit, les approbations à supervision humaine,
les paliers d'autonomie et le budget d'étapes sont appliqués par le runtime
autour de chaque agent, et non implémentés par chaque agent individuellement.
Un auteur d'agent ne peut pas les oublier, et un opérateur ne peut pas être
surpris par leur absence. C'est ce qui rend l'autonomie délégable ; le sujet
est développé dans
[Concepts transversaux](/architecture/crosscutting-concepts) et dans le
[modèle de responsabilité](/explanation/accountability-model).

Les décisions qui encadrent cela sont ADR-015 (gouvernance des permissions et
des outils), ADR-013 (supervision humaine), et ADR-016 (secrets et
authentification API).
