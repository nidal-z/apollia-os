---
sidebar_position: 3
title: Souveraineté et local-first
description: "Ce que la souveraineté veut dire en pratique : ce qui reste sur votre machine, ce qui n'en sort que sur une action explicite, et comment c'est tenu."
---

# Souveraineté et local-first

La souveraineté est l'engagement qu'Apollia est construit pour tenir : vos
données, vos modèles et votre runtime vous appartiennent. « Local-first » est
ce qui rend cet engagement concret plutôt qu'aspirationnel. Cette page
explique ce que ces mots signifient en pratique, et pourquoi ils sont au
cœur de la proposition de valeur, et non une fonctionnalité ajoutée après
coup.

## Par défaut, rien ne quitte la machine

<!-- claim:tcp-listener-is-loopback-with-token -->
La posture de départ est la posture souveraine. Le stockage est une base de
données SQLite locale. L'API du runtime se lie à une socket Unix, accessible
uniquement par le propriétaire du fichier de socket, ainsi qu'en TCP sur
`127.0.0.1:7771`, où chaque requête doit porter un jeton Bearer. Loopback
signifie que l'écouteur n'est joignable depuis le réseau en aucune façon ;
l'ouvrir est une modification délibérée de `[api] bind`. Il n'y a ni
télémétrie ni envoi automatique de quoi que ce soit.
Voilà ce que signifie « local-first » sur le plan opérationnel : le chemin
qui s'exécute quand vous ne faites rien de particulier est celui qui garde
tout sur votre matériel. Une souveraineté qu'il faudrait configurer pour
l'obtenir n'en serait pas une ; ici, elle est le défaut, et c'est en sortir
qui constitue l'acte délibéré.

## L'inférence s'exécute en local

Apollia embarque sa propre inférence locale. Un modèle au format GGUF
s'exécute via un moteur local supervisé (le `llama-server` embarqué, fondé
sur llama.cpp en amont), de sorte que le raisonnement qui pilote un agent
peut se dérouler entièrement sur votre machine, sans aucun appel à un LLM
externe. C'est cette pièce qui donne du sens au reste : un stockage local ne
garantit pas grand-chose si chaque pensée de l'agent implique un
aller-retour vers le serveur de quelqu'un d'autre.

Deux limites honnêtes sur l'inférence locale disponible aujourd'hui. Le
moteur local charge un modèle GGUF en fichier unique. Et la capacité locale
livrée est la génération de texte : Apollia ne présente pas de pipeline
d'embeddings local comme fonctionnalité livrée. Énoncé clairement pour que
vous puissiez planifier en fonction de ce qui existe réellement, plutôt
qu'en fonction de ce qu'une feuille de route laisse entendre.

## Zéro dépendance externe

Le local-first ne tient que si faire tourner l'ensemble en local n'exige pas
discrètement toute une flotte de services. Ce n'est pas le cas. Le runtime
ne nécessite ni Docker, ni Node, ni base de données externe, ni installation
séparée de Python : l'interpréteur est embarqué. Un seul binaire constitue
l'unité de déploiement. Chaque connexion optionnelle à un service externe se
dégrade proprement plutôt que de devenir une exigence stricte. C'est le
deuxième principe, et c'est ce qui empêche « local » de vouloir dire « local
plus cinq autres choses à faire tourner en plus ».

## Le cloud est un choix, jamais un défaut

L'inférence cloud existe, et son activation est volontaire, avec votre
propre clé. Rien n'atteint un fournisseur cloud tant que vous n'en avez pas
configuré un, et le modèle local reste le défaut.

Ce que cette activation volontaire autorise mérite d'être énoncé
précisément, car c'est le seul endroit où le runtime décide de lui-même. Si
vous activez le routage hybride (`[llm.routing.hybrid]`), le runtime peut
faire remonter un tour de conversation vers le backend frontière sans
redemander confirmation au moment où cela se produit : aujourd'hui, le
déclencheur est trois échecs d'outil consécutifs au sein de la même
exécution. C'est la forme que prend cette activation volontaire. Ce n'est
pas « le cloud reste intouché sauf si vous appuyez sur quelque chose à
chaque fois », c'est « vous décidez une fois, par configuration, quelles
conditions sont autorisées à solliciter l'extérieur ». Laissez le routage
hybride désactivé, et aucune requête ne part jamais d'elle-même.

Cette conception traite le cloud comme une capacité à laquelle recourir, pas
comme une dépendance dont vous hériteriez.

## Votre mémoire vous appartient

La mémoire d'un agent, ses couches épisodique, sémantique et procédurale,
peut être exportée et importée par vos soins. Ce n'est pas une
fonctionnalité de confort ; c'est la forme concrète de la propriété. Des
données que vous pouvez extraire, inspecter et déplacer sont des données qui
vous appartiennent d'une manière qu'une politique de confidentialité ne peut
pas promettre. La portabilité, c'est ce qui transforme « nous le stockons en
local » en « vous le détenez ».

## Pourquoi c'est le cœur, pas une fonctionnalité

Chacune des autres forces d'Apollia repose sur celle-ci. La
responsabilisation ne veut pas dire grand-chose si le journal d'audit vit
sur un serveur que vous ne contrôlez pas. L'autonomie est difficile à
déléguer si la déléguer signifie expédier vos données hors de chez vous. La
souveraineté et le local-first sont la contrainte qui façonne chaque
défaut, et c'est exactement pour cela qu'ils apparaissent comme les deux
premiers des [huit principes](/explanation/the-8-principles), et non comme
des options dans une page de réglages. Pour situer cela dans l'architecture,
voir la section souveraineté des
[Concepts transversaux](/architecture/crosscutting-concepts) et la page
[Contraintes](/architecture/constraints). Pour les leviers de configuration
concrets (socket contre TCP, sélection du backend), voir la
[Référence de configuration](/reference/configuration).

## Voir aussi

- [Les 8 principes](/explanation/the-8-principles)
- [Contraintes](/architecture/constraints)
- [Concepts transversaux](/architecture/crosscutting-concepts)
- [Référence de configuration](/reference/configuration)
