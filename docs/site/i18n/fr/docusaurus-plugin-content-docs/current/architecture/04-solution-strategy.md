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

Voir [le modèle d'exécution](/architecture/decisions#execution-model), qui s'appuie sur cette base.

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

Le daemon supervise ce processus : il sonde la vie de chaque instance du moteur
et relance celle qui est morte, avec un délai d'attente qui croît d'une seconde
à trente. Un modèle évincé volontairement n'est pas relancé.
Voir [l'inférence locale](/architecture/decisions#local-inference).

## Un pont PyO3 avec des services découplés par traits

Le code qu'un paquet apporte est en Python ; le runtime est écrit en Rust. Le
pont est PyO3, avec `pyo3-async-runtimes` pour l'interopérabilité asynchrone. Le
runtime ne livre pas ses structures internes brutes à Python : il expose un
ensemble de services (l'objet `ctx`) derrière des traits Rust, ce qui découple
le contrat de son implémentation et le rend simulable (mock) pour les tests. Le
contrat de types nomme dix-sept services, et ce qu'un appel reçoit dépend de ce
qu'il est : un outil de paquet s'exécute avec un contexte qui ne porte ni
modèle, ni proxy d'outils, ni mémoire, et un workflow de paquet y ajoute
`ctx.llm`, `ctx.agents`, `ctx.workflows` et `ctx.user`.

Voir [socle technique et runtime](/architecture/decisions#stack-and-runtime) et [le contrat d'agent](/architecture/decisions#agent-contract) ; `ctx` est
est documenté dans la [référence du SDK](/reference/sdk).

## Un contrat machine pour l'intégration hôte

Le runtime est conçu pour être piloté par un produit hôte : sa surface HTTP
est donc un contrat stable de premier ordre, pas une réflexion après coup. La
spécification OpenAPI est générée à partir du code, servie par le daemon, et
versionnée (`/api/v1`, les changements cassants étant réservés à un futur
`/api/v2`). Des SDK hôtes typés en TypeScript et en Python sont générés à
partir de cette spécification. Un intégrateur pilote un daemon réel sans
avoir à rétro-ingénierer quoi que ce soit.

Voir [intégration hôte](/architecture/decisions#host-integration). La surface générée est la
[référence de l'API HTTP](/reference/api/apollia-os-runtime-api) ; le guide
pratique est
[Intégrer via le contrat pilote](/how-to/integrate-via-driving-contract).

## La gouvernance vit dans le runtime, pas dans l'agent

Le budget d'étapes est appliqué par le runtime autour de chaque agent, et non
implémenté par chaque agent individuellement, et il borne chaque appel d'outil
quel que soit le chemin sur lequel il s'exécute. L'enregistrement va dans un seul
registre : un appel d'outil qu'une exécution annonce est écrit, par ses
événements rattachés à l'exécution, dans le journal chaîné par hachage. Le
palier d'autonomie atteint une
exécution orchestrée de la même façon que le budget, par le moteur qui la
planifie et l'exécute.

<!-- claim:hitl-wired-in-chat-path-only -->
Les règles de permission et l'approbation humaine vivent au même endroit. Chaque
appel d'outil d'un agent est tranché avant de s'exécuter par une seule décision
de gouvernance, dans une conversation, dans une tâche, dans une étape de plan
comme dans un élément de lot : la politique de l'organisation et une règle de
refus le refusent, un outil qui ne fait que lire s'exécute, et tout ce qui est
sensible s'exécute sur un accord permanent de l'agent ou demande à une
personne. Un agent est une fiche et ne porte aucun code : il n'a donc aucun
chemin pour contourner cette décision. Ce que la décision ne voit pas, c'est le
Python d'un paquet : une fois qu'un outil de paquet est autorisé à s'exécuter,
son code tourne dans le processus du runtime avec les droits de ce processus,
et c'est pourquoi installer un paquet est l'acte de confiance. Le
[modèle de confiance des agents](/explanation/agent-trust-model) énonce cette
frontière. Le tableau complet se trouve dans
[Concepts transversaux](/architecture/crosscutting-concepts) et dans le
[modèle de responsabilité](/explanation/accountability-model).

Les décisions qui encadrent cela sont [le modèle de permissions](/architecture/decisions#permission-model),
[l'humain dans la boucle](/architecture/decisions#human-in-the-loop), et [secrets et
authentification API](/architecture/decisions#secrets-and-api-auth).
