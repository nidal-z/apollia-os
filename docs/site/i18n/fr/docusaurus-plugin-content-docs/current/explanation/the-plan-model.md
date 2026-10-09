---
sidebar_position: 5
title: Le modèle de plan et d'exécution orchestrée
description: Comment le runtime décide d'exécuter une tâche, plan d'abord ou direct, ce que contient un plan, et où un humain l'approuve avant toute exécution.
---

# Le modèle de plan et d'exécution orchestrée

Quand vous confiez une tâche à un agent autonome, le runtime doit décider comment
l'exécuter, traduire l'intention en étapes concrètes, les exécuter en toute
sécurité, puis vérifier le résultat. Cette page explique ce modèle : comment une
requête est classifiée, ce qu'est un plan, comment la porte d'approbation du plan
maintient un humain dans la boucle, et comment le moteur se vérifie et se corrige
lui-même. Elle explique le modèle plutôt que la procédure ; pour la version
pratique, voir [Exécuter un agent orchestré](/how-to/run-an-orchestrated-agent),
et pour les diagrammes de séquence du runtime, voir la
[Vue runtime](/architecture/runtime-view).

## Direct contre orchestré

Un agent s'exécute selon l'un de deux modes d'exécution. **Direct** : le modèle
répond et appelle des outils au fil de l'eau. **Orchestré** : un plan est
d'abord construit puis ses étapes sont exécutées, pour un travail qui nécessite
plusieurs appels d'outils pilotés par le raisonnement.

Ce qui choisit, c'est le réglage de planification de l'agent : `planning`, sous
`[agents.defaults]` dans le manifeste de son paquet, que la personne peut
remplacer. `never` s'exécute en direct, `always` planifie d'abord, et `auto`, la
valeur quand ni l'un ni l'autre ne dit rien, laisse le runtime juger la requête.
Ce jugement est déterministe et ne fait appel à aucun LLM. Une conversation en
mode plan est à part : son plan est approuvé dans la conversation, et le réglage
ne s'y applique pas.

<!-- claim:execution-mode-classification-weights -->

Sous `auto`, une exécution planifie quand la requête elle-même se lit comme
plusieurs étapes, ce qui est le cas lorsqu'elle contient un mot-clé de
planification (`plan`, `etape`, `step`, `sequence`, `workflow`, `pipeline`), ou
quand un score calculé sur l'agent et la requête atteint un seuil. Le score
additionne des poids indépendants :

| Poids | Ajouté quand |
| --- | --- |
| 0.30 | le budget d'étapes de l'exécution dépasse 15 |
| 0.20 | la requête comporte plus de 3 parties en entrée |
| 0.20 | l'agent se voit accorder plus de 4 outils |
| 0.10 | le texte en entrée dépasse 500 caractères |
| 0.10 | la mission de l'agent contient un mot-clé de planification |

Le seuil est 0.40, fixé dans le code : la section `[oria]` d'`apollia.toml` est
retirée et rien ne la lit. Deux conséquences découlent des chiffres plutôt que
du texte. Un mot-clé de planification dans la requête atteint le seuil à lui
seul. Aucun poids du tableau pris isolément n'y parvient : sans mot-clé, la
planification exige qu'au moins deux signaux concordent. La fonction de score
porte aussi un poids pour un tag `multi-step` et un autre pour la profondeur de
la mémoire épisodique ; l'exécution d'un agent ne fournit ni l'un ni l'autre,
ils ne s'appliquent donc jamais.

## Le cache de plan

<!-- claim:plan-cache-is-consulted-by-the-engine -->

La planification est la partie coûteuse d'une exécution orchestrée, donc un plan
est mis en cache et réutilisé. La clé est une empreinte SHA-256 calculée sur le
nom et la version de l'agent, sa liste d'outils triée, et le texte de la requête
normalisé en minuscules avec les espaces réduits. Une version d'agent
différente, un jeu d'outils différent, ou une formulation nettement différente
ne trouvent pas d'entrée dans le cache ; une requête reformulée mais équivalente
peut, elle, en trouver une.

<!-- claim:plan-cache-has-no-automatic-expiry -->

Les plans mis en cache **n'expirent jamais d'eux-mêmes**. Il n'y a ni éviction en
arrière-plan ni durée de vie limitée : une entrée reste tant qu'un opérateur ne
la supprime pas. Le nettoyage est une commande manuelle, et
`apollia-os plan cache evict` prend en paramètre un âge en jours dont la valeur
par défaut est 7. Cette valeur par défaut est à l'origine de la croyance selon
laquelle le cache expire au bout d'une semaine. Ce n'est pas le cas : rien
n'exécute cette commande à votre place.

Cela compte quand le comportement d'un agent change sans que sa version change.
Le cache continuera de servir le plan construit avant le changement jusqu'à ce
qu'il soit vidé. Voir [Déployer en production](/how-to/deploy-in-production)
pour les commandes.

Le cache est la seule partie d'un plan qui survit au daemon. L'état d'exécution,
lui, ne survit pas : un plan en cours est conservé en mémoire, donc redémarrer
le daemon met fin à l'exécution au lieu de la reprendre. Les plans mis en cache
survivent parce qu'ils résident dans `plan_cache.db` ; l'exécution qui en
utilisait un ne survit pas.

Un détail à connaître, sans détour. Le point d'entrée d'exécution unifié
implémente la branche orchestrée ; sa branche directe est un stub, et
l'exécution directe réelle passe par un point d'entrée séparé. En pratique,
c'est le chemin orchestré que cette page décrit de bout en bout. Ce stub est
documenté dans
[Risques et dette technique](/architecture/risks-and-technical-debt).

## Le plan comme artefact

L'orchestration produit un plan, et ce plan est un véritable artefact, pas un
flux de contrôle caché. C'est un graphe orienté acyclique d'étapes : chaque
étape peut dépendre d'autres, et ces dépendances forment les arêtes du graphe.
Parce que le plan est explicite, il peut être affiché, approuvé, audité et
révisé.

<!-- claim:orchestrated-parallelism-not-active -->
C'est ce graphe qui rend le parallélisme possible. Le moteur parcourt le
plan par niveaux topologiques, et les étapes d'un même niveau s'exécutent
simultanément lorsqu'il y en a au moins deux et que chacune appelle un outil
qui ne fait que lire.

**Tout autre niveau exécute ses étapes l'une après l'autre.** Cela couvre une
étape à laquelle le modèle répond lui-même, un outil qui écrit, et un niveau
d'une seule étape. Savoir si un outil ne fait que lire est demandé au proxy
d'outils de l'exécution, qui répond d'après le descripteur de chaque outil
accordé à l'agent. Les niveaux ordonnent le plan et expriment ce qui dépend de
quoi ; ils n'apportent de la vitesse que là où le travail consiste à lire.

## La porte d'approbation du plan

Pour un plan aux conséquences significatives, le runtime peut suspendre
l'exécution avant de démarrer et émettre une demande d'approbation de plan. Un
humain l'approuve alors, le rejette, ou suspend l'exécution pour injecter des
consignes. Un rejet déclenche une replanification bornée qui prend ce retour en
compte. L'activation de cette porte dépend du palier d'autonomie : elle est
active dans les paliers inférieurs et contournée dans les paliers supérieurs,
comme décrit dans [Paliers d'autonomie](/explanation/autonomy-tiers). La page de
vue runtime montre cela sous forme de séquence, dans son scénario de mode plan
du chat.

## Comment les étapes obtiennent leurs arguments

Une étape de plan nomme un outil, mais l'outil a besoin d'arguments concrets.
Apollia les résout avec un contrat hybride (voir [le modèle d'exécution](/architecture/decisions#execution-model)). Au moment de la
planification, le raisonneur remplit les arguments structurés de l'étape sous
contrainte de grammaire, si bien que le plan porte déjà des arguments typés et
valides au regard du schéma. Si une étape arrive à l'exécution sans arguments
valides, une extraction juste-à-temps contrainte par schéma les remplit à
partir de la description de l'étape. C'est ce qui permet au chemin orchestré de
piloter de vrais outils natifs avec de vrais arguments structurés, plutôt que
de leur passer un bloc de texte en espérant que ça marche.

## Vérifier et corriger le résultat

Une exécution orchestrée terminée n'est pas acceptée sur parole. Le moteur
effectue une passe de vérification : un critique LLM examine le
résultat et produit un verdict, et ce verdict est enregistré comme événement
signé dans le journal d'audit. En cas de verdict négatif, le moteur replanifie
et réexécute, dans la limite d'un petit nombre de tentatives (deux par défaut)
et en puisant dans le même budget partagé, si bien que l'auto-correction ne
peut pas dépasser le plafond de l'exécution.

Deux limites, dans un souci de transparence. Cette passe de vérification est
inactive dans le palier Assisted par défaut et devient active à partir de
Supervised, ce qui en fait une propriété du palier que vous choisissez (voir
[Paliers d'autonomie](/explanation/autonomy-tiers)). Et au sein de cette passe,
le critique LLM est la partie câblée et opérationnelle ; aucune vérification
shell n'est exécutée pour le compte d'un agent. Le volet audit de tout
cela, le journal signé et l'événement de verdict, est couvert par le
[modèle de responsabilité](/explanation/accountability-model) et n'est pas
repris ici.

## Pour aller plus loin

- Pour construire et exécuter un agent orchestré, lire
  [Exécuter un agent orchestré](/how-to/run-an-orchestrated-agent).
- Pour voir l'exécution sous forme de diagrammes de séquence, lire la
  [Vue runtime](/architecture/runtime-view).
- Pour la place du moteur parmi les crates, lire
  [Blocs de construction](/architecture/building-blocks).
- Pour les décisions à l'origine de ce modèle, lire
  [Décisions d'architecture](/architecture/decisions).

## Voir aussi

- [Paliers d'autonomie](/explanation/autonomy-tiers)
- [Le modèle de responsabilité](/explanation/accountability-model)
- [Exécuter un agent orchestré](/how-to/run-an-orchestrated-agent)
- [Vue runtime](/architecture/runtime-view)
