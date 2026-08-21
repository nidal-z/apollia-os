---
sidebar_position: 4
title: Paliers d'autonomie
---

# Paliers d'autonomie

Jusqu'où un agent va de sa propre initiative n'est pas une propriété fixe de
l'agent. C'est le palier d'autonomie, choisi pour une exécution. Le même agent
peut marquer une pause sur son plan et vous attendre, ou dérouler ce plan sans
interruption, selon le palier dans lequel vous le placez. Cette page explique
les quatre paliers, ce que le palier change réellement, et comment en choisir
un. Pour les garde-fous et le budget de pas qui bornent toute exécution quel que
soit le palier, voir le
[modèle de responsabilité](/explanation/accountability-model) ; cette page
est le complément qui explique le palier, pas les garde-fous.

## Le palier, pas l'agent

Séparer « ce que l'agent peut faire » de « jusqu'où il peut aller sans
surveillance » est l'idée centrale, et le palier ne répond jamais qu'à la
seconde moitié. Un agent déclare les outils dont il a besoin ; le palier décide
si son plan attend une approbation et si son résultat est vérifié ensuite. Il ne
décide rien des permissions : aucun palier n'accorde un outil, n'en refuse un,
ni n'ajoute un point de contrôle humain sur un appel d'outil. Cette séparation
est ce qui permet de déployer le même agent avec prudence dans un contexte
sensible et avec souplesse sur une machine de test isolée, sans le réécrire.

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

La porte est active en Assisté et Supervisé, et contournée en Autonome borné et
Autonome long, mais seulement pour une exécution qui ne porte aucune décision
propre. `apollia-os run` en porte toujours une : `--plan` arme la porte pour
cette exécution et son absence la désarme, quel que soit le palier. Monter dans
les paliers échange de l'interruption contre de l'élan.

## Ce que le palier change réellement

<!-- claim:plan-gate-yields-to-the-per-run-override -->
Cinq choses varient avec le palier, et aucune autre. La porte du plan : dans les
deux paliers les plus bas, un plan à conséquence marque une pause pour attendre
une approbation humaine ; dans les deux paliers les plus hauts, non ; et dans les
deux cas une exécution qui porte sa propre décision l'emporte sur le palier.
L'auto-vérification : la passe de vérification et de critique du runtime est
inactive en Assisté et entre en jeu à partir de Supervisé, si bien qu'une
exécution terminée peut être vérifiée avant que son résultat ne soit accepté.
L'injection mémoire : le palier le plus élevé est le seul à ajouter un brief de
persona utilisateur, et seulement à l'intérieur de l'assistant intégré. Le profil
de prompt système : Assisté prend un prompt intégré, les trois autres paliers en
prennent un plus persévérant. Et le budget de pas suggéré, qui mérite une seconde
lecture.

<!-- claim:tier-sets-budget-runtime-ceiling-caps-it -->
Les quatre paliers déclarent 100, 200, 300 et 500 pas de raisonnement, et cette
table est réelle. Ce qui la lit, c'est le chemin du chat, et le chat libre
s'exécute au palier par défaut sans jamais en changer.

<!-- claim:tier-budget-capped-at-thirty-on-agent-paths -->
Une exécution d'agent ne lit pas cette table du tout. Les deux chemins
d'exécution prennent le budget déclaré par le manifeste de l'agent et le
plafonnent contre un plafond runtime fixe de 30 pas de raisonnement, 60 appels
d'outils et 600 secondes de temps réel. Élargir le palier élargit donc ce qu'un
agent peut tenter bien avant de changer ce que l'agent obtient réellement, et
cela ne supprime jamais la limite dure. La mécanique de la boucle de vérification
relève du
[modèle de plan et d'exécution orchestrée](/explanation/the-plan-model), et les
garde-fous sous lesquels elle s'exécute relèvent du
[modèle de responsabilité](/explanation/accountability-model).

## Choisir un palier

Choisissez en fonction du coût d'une action erronée et de la confiance que la
tâche a gagnée. Commencez un travail nouveau ou sensible en Assisté, où vous
voyez le plan en premier. Passez en Supervisé quand vous voulez la
vérification du moteur sans renoncer à la porte. Ne recourez aux paliers
autonomes que lorsque le travail est bien délimité, les permissions sont
strictes, et les interruptions d'un palier inférieur coûteraient plus
qu'elles ne protègent. Le palier se change facilement, traitez-le donc comme
un jugement propre à chaque exécution, pas comme un réglage permanent.

Le palier se pose pour une exécution, avec `--autonomy` sur `apollia-os run`. Il
n'existe aucune section `[autonomy]` dans `apollia.toml` que le runtime lise, et
le bureau n'offre aucun réglage de palier.

## Voir aussi

- [Le modèle de responsabilité](/explanation/accountability-model)
- [Le modèle de plan et d'exécution orchestrée](/explanation/the-plan-model)
- [Choisir un palier d'autonomie](/operator-help/agents/choisir-un-palier-d-autonomie)
