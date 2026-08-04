---
sidebar_position: 4
title: Paliers d'autonomie
---

# Paliers d'autonomie

Ce qu'un agent peut faire de sa propre initiative n'est pas une propriété fixe
de l'agent. C'est un cadran que l'opérateur règle, appelé le palier
d'autonomie. Le même agent peut s'exécuter avec prudence, en demandant une
décision avant toute action à conséquence, ou librement, en agissant sans
interruption, selon uniquement le palier dans lequel vous le placez. Cette
page explique les quatre paliers, ce que le palier change réellement, et
comment en choisir un. Pour les garde-fous et le budget de pas qui bornent
toute exécution quel que soit le palier, voir le
[modèle de responsabilité](/explanation/accountability-model) ; cette page
est le complément qui explique le cadran, pas les garde-fous.

## Le cadran, pas l'agent

Séparer « ce que l'agent peut faire » de « jusqu'où il peut aller sans
surveillance » est l'idée centrale. Un agent déclare les outils dont il a
besoin ; le palier détermine le niveau de supervision humaine placé entre
l'intention de l'agent et ses actions. Cette séparation est ce qui permet de
déployer le même agent avec prudence dans un contexte sensible et avec
souplesse sur une machine de test isolée, sans le réécrire. Le palier reflète
la confiance que vous accordez pour une tâche donnée, et la confiance est une
décision qui relève du contexte, pas du code.

## Les quatre paliers

Apollia définit quatre paliers, du plus supervisé au plus autonome :

- **Assisté.** Le palier par défaut. La porte est active : un plan proposé et
  les actions à conséquence attendent une décision humaine avant de se
  poursuivre. C'est le palier pour les agents que l'on connaît mal, les
  données sensibles, ou toute exécution où l'on veut voir le plan avant qu'il
  ne s'exécute.
- **Supervisé.** La porte reste active, un humain reste donc dans la boucle
  sur les étapes à conséquence, mais c'est le palier à partir duquel la passe
  de vérification propre au runtime s'active (voir plus bas). À utiliser
  quand on veut la supervision en plus de l'auto-vérification du moteur.
- **Autonome borné.** La porte est contournée : l'agent agit sans marquer de
  pause pour l'approbation du plan, dans les limites du budget non
  contournable imposé par le runtime. À utiliser pour un travail de
  confiance, bien délimité, où l'interruption coûterait plus qu'elle ne
  protège.
- **Autonome long.** La porte y est également contournée ; ce palier est
  destiné aux exécutions non surveillées plus longues. C'est le palier le
  plus large, adapté uniquement quand la tâche est bien comprise et que le
  budget et les permissions bornent déjà le rayon d'impact possible.

La porte est active en Assisté et Supervisé, et contournée en Autonome borné
et Autonome long. Monter dans les paliers échange de l'interruption contre de
l'élan.

## Ce que le palier change réellement

Deux choses varient avec le palier. D'abord, la porte du plan : dans les deux
paliers les plus bas, un plan à conséquence marque une pause pour attendre
une approbation humaine ; dans les deux paliers les plus hauts, non. Ensuite,
l'auto-vérification : la passe de vérification et de critique du runtime est
inactive en Assisté et entre en jeu à partir de Supervisé, si bien qu'une
exécution terminée peut être vérifiée avant que son résultat ne soit accepté.
La mécanique de cette boucle de vérification relève du
[modèle de plan et d'exécution orchestrée](/explanation/the-plan-model), et
les garde-fous sous lesquels elle s'exécute relèvent du
[modèle de responsabilité](/explanation/accountability-model). Ce qui compte
ici, c'est que le palier est le cadran unique qui gouverne les deux.

<!-- claim:tier-sets-budget-runtime-ceiling-caps-it -->
Notez ce que le palier ne change pas : le plafond du runtime. Chaque palier
porte son propre budget, de 100 pas de raisonnement au plus prudent à 500 au
plus autonome, et ce budget est toujours plafonné par le plafond du runtime,
qu'aucun palier ne peut dépasser. Le plafond sur les pas de raisonnement, les
appels d'outils et le temps réel écoulé est appliqué par le runtime à chaque
palier, y compris le plus autonome. Élargir l'autonomie élargit ce qu'un
agent peut tenter ; cela ne supprime jamais la limite dure.

## Choisir un palier

Choisissez en fonction du coût d'une action erronée et de la confiance que la
tâche a gagnée. Commencez un travail nouveau ou sensible en Assisté, où vous
voyez le plan en premier. Passez en Supervisé quand vous voulez la
vérification du moteur sans renoncer à la porte. Ne recourez aux paliers
autonomes que lorsque le travail est bien délimité, les permissions sont
strictes, et les interruptions d'un palier inférieur coûteraient plus
qu'elles ne protègent. Le palier se change facilement, traitez-le donc comme
un jugement propre à chaque exécution, pas comme un réglage permanent. Pour
savoir où le palier se configure, voir la
[Référence de configuration](/reference/configuration).

## Voir aussi

- [Le modèle de responsabilité](/explanation/accountability-model)
- [Le modèle de plan et d'exécution orchestrée](/explanation/the-plan-model)
- [Référence de configuration](/reference/configuration)
