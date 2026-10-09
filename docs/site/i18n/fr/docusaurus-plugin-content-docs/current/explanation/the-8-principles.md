---
sidebar_position: 1
title: Les 8 principes
description: Les huit principes sur lesquels Apollia est construit, du local-first aux garde-fous que le runtime ne peut pas contourner, et ce qu'ils excluent.
---

# Les 8 principes

Apollia est un runtime souverain pour agents IA autonomes. Il exécute des
agents localement, sous une gouvernance qu'il impose, avec des outils, et sans
dépendance au cloud. Huit principes tiennent cette promesse ensemble. Ce ne sont pas des
préférences de style : chacun a été imposé par une contrainte réelle, et chacun
façonne un comportement par défaut sur lequel vous pouvez compter en tant
qu'adoptant. Cette page explique ce qu'est chaque principe et pourquoi il compte
pour vous. Elle ne les reformule pas en règles : pour leur forme normative et
son application, ces mêmes huit principes réapparaissent comme des
[contraintes](/architecture/constraints) d'ingénierie.

## 1. Local-first

Aucun octet de donnée utilisateur ne quitte la machine sans action explicite.
L'inférence peut s'exécuter entièrement en local sur un modèle GGUF, le
stockage est un fichier SQLite local, et il n'y a ni télémétrie ni rappel vers
un serveur distant. Cela existe parce que les garanties contractuelles ne
suffisaient pas aux organisations qu'Apollia vise : la réponse n'a pas été de
promettre que le cloud se comporterait bien, mais de rendre le cloud
techniquement superflu. Pour vous, cela signifie que la posture sûre est le
comportement par défaut, pas un réglage qu'il faut penser à activer. Le
traitement approfondi se trouve dans
[Souveraineté et local-first](/explanation/sovereignty-and-local-first).

## 2. Aucune dépendance externe

Le binaire s'exécute sur une machine Linux vierge sans installation préalable :
pas de Docker, pas de Node, pas de base de données externe, pas de runtime
Python séparé. Toute dépendance optionnelle à un service externe se dégrade
proprement plutôt que de faire échouer l'exécution. Cela compte parce que la
complexité opérationnelle constitue un veto commercial pour les équipes qui
évaluent un runtime souverain. Un seul artefact à déployer, c'est une seule
surface d'attaque à raisonner et une seule chose à maintenir en fonctionnement.

## 3. Contrat minimal

<!-- claim:agent-contract-is-a-manifest-read-strictly -->
Un agent est une fiche : une mission, un modèle, des outils, des sources et des
droits. Un paquet l'écrit comme une entrée `[[agents]]` de son manifeste
`apollia.toml`, et aucun objet Python ne le représente. Le code est optionnel :
un paquet apporte des outils (`@tool`) et des workflows (`@workflow`), deux
décorateurs posés sur des fonctions asynchrones, et rien d'autre. L'objectif est
de garder minimale la surface qu'un auteur doit apprendre, pour que le runtime
porte les parties difficiles (gouvernance, budgets, outils) plutôt que de les
reporter sur chaque agent. Il laisse aussi l'agent entre les mains de la
personne qui s'en sert : ce qu'un paquet déclare est un ensemble de valeurs par
défaut, et un outil, un droit ou un modèle se change depuis l'interface, sans
nouvelle version du code. Pour le contrat exact que voit le Python d'un paquet,
lisez la [référence SDK](/reference/sdk).

Le runtime refuse un paquet dont le manifeste ne valide pas. Une clé que le
schéma ne connaît pas est refusée avec son chemin complet, de même que ce que le
schéma ne sait pas exprimer, par exemple un agent auquel est accordé un outil de
paquet que le paquet ne livre pas. Une clé mal orthographiée arrête le paquet
plutôt que de laisser tourner un agent privé de ce que son auteur a déclaré.
L'ancien contrat, une classe décorée par `@agent` portant des méthodes `@skill`
ou `@on_message`, a disparu (voir
[le contrat d'agent](/architecture/decisions#agent-contract)).

## 4. Échec rapide

Toute erreur détectable au démarrage est détectée au démarrage, pas trois
étapes après le début d'une exécution. Un modèle manquant, un manifeste
malformé ou un backend injoignable remontent avant que le travail ne
commence. Cela garde les échecs peu coûteux et lisibles : vous apprenez ce qui
ne va pas dès le lancement, pas après que l'agent a dépensé une partie de son
budget à se retrouver dans un état cassé.

## 5. Un acteur, une responsabilité

Le runtime est construit comme un ensemble d'acteurs Tokio, chacun possédant
son propre état et communiquant via des canaux de messages, sans verrou
partagé entre eux. Ce n'est pas un choix esthétique : l'état mutable partagé
entre tâches asynchrones est précisément là où vivent les interblocages et les
comportements inexplicables. Le fait que chaque acteur soit responsable d'une
seule chose est ce qui rend le comportement du système raisonnable à
analyser. La stratégie derrière ce choix se trouve dans
[Stratégie de solution](/architecture/solution-strategy).

## 6. Mémoire à l'initiative de l'agent

<!-- claim:memory-injection-confined-to-builtin-assistant -->
Apollia n'injecte jamais de mémoire dans le prompt d'un agent, et n'écrit rien
en mémoire à sa place. Un agent rappelle ce qu'il décide de rappeler, quand il
décide de le faire, avec l'outil `memory_search`, et garde une note avec
`memory_write`. Les deux outils viennent d'un seul geste, cocher Mémoire sur la
fiche de l'agent, et aucun ne demande d'accord : l'espace est celui de l'agent,
un espace nommé d'après lui et gardé d'une conversation et d'une tâche à
l'autre, qui n'atteint ni vos données ni la machine. L'injection automatique de
mémoire est pratique et discrètement
corrosive : elle rend les entrées d'une exécution opaques et son comportement
difficile à attribuer. Laisser le rappel à l'initiative de l'agent garde
honnête la trace de ce qui a nourri une décision.

Votre profil est le cas inverse. Il vous suit dans chaque conversation où il
est activé, donc un agent ne l'écrit jamais. Un agent dont la source Profil est
cochée peut proposer un fait avec `profile_propose`, et la proposition vous est
soumise à chaque fois, sans « toujours ».

Trois exceptions existent, et aucune n'est atteignable depuis un agent
que vous installez. Les deux premières sont confinées à l'assistant
conversationnel intégré, celui qui se trouve derrière la fenêtre de chat, et
vivent dans son générateur de prompt et son gestionnaire de chat. La troisième
vit ailleurs, dans la commande de réécriture du bureau, qui ne touche jamais au
prompt de l'assistant : elle répond au composeur, pas à une exécution. Aucun
chemin d'exécution d'agent ne passe par l'une des trois.

La première est le profil de la conversation, et c'est celle que vous
contrôlez. Une conversation désigne l'un de vos profils, ou suit le profil par
défaut. À chaque tour, à tous les paliers d'autonomie, l'assistant ajoute à son
prompt système un court résumé de ce profil : nom, rôle, fonction, outils,
préférences et notes, ce que vous avez rempli. Elle se trouve sous un
interrupteur, « Apollia connaît mon profil », dans l'onglet Intégrations du
panneau droit de la conversation. L'interrupteur est allumé à la création d'une
conversation, il est enregistré avec elle, et l'éteindre prend effet au message
suivant. La jauge de contexte de la zone de saisie dit si le prompt porte le
profil : ce que vous y lisez est ce que le modèle a reçu. Les sessions Companion et les sessions qui
exécutent un agent installé ne le reçoivent jamais, interrupteur ou non.

<!-- claim:cross-session-recall-injects-summaries -->
La seconde n'a pas d'interrupteur, et mérite d'être énoncée sans
détour. Au **premier message d'une session de chat libre**, le runtime
interroge, avec ce message, un index de résumés de sessions passées et ajoute
jusqu'à trois correspondances au prompt système, sous un intitulé les
désignant comme des conversations précédentes. Les messages de moins de 20
octets y échappent, si bien qu'une simple salutation ne rappelle rien. Seuls
les résumés sont injectés, jamais le contenu des messages passés. Les sessions
Companion en sont exclues d'emblée : elles ne doivent pas hériter d'historique
personnel.

Prenez cette seconde exception pour ce qu'elle est. Dans la fenêtre de chat,
une nouvelle conversation peut démarrer en portant déjà une trace des
précédentes, sans que l'opérateur l'ait demandé pour cette session précise.
Cela achète de la continuité dans une surface produit où un utilisateur
s'attend raisonnablement à être reconnu. Avec le résumé du profil, c'est l'un
des deux endroits du runtime où la règle « à l'initiative de l'agent » ne tient
réellement pas, et le seul que vous ne pouvez pas couper : c'est pourquoi cela
est écrit ici plutôt que laissé implicite.

<!-- claim:rewrite-injects-work-context -->
La troisième exception est également à l'initiative de l'opérateur, et ne
survient que lorsqu'elle est explicitement déclenchée. Lorsque l'opérateur
clique sur le bouton « Améliorer la demande » dans un composeur de chat, la
requête de réécriture inclut trois champs du profil de la conversation
(rôle, fonction, outils) s'ils ont été remplis. Ce n'est
pas une injection automatique : rien ne se produit tant que l'opérateur ne
clique pas, et il voit le résultat réécrit avant de choisir de l'envoyer. Le
contexte aide la réécriture à rester ancrée dans l'environnement réel de
l'opérateur plutôt que d'inventer des outils qu'il n'utilise pas.

La couche mémoire elle-même est exportable et importable, ce qui explique
qu'elle relève autant de la souveraineté que de l'agentivité.

## 7. Garde-fous non négociables

Chaque exécution autonome est bornée par un budget d'étapes que le runtime
impose : un plafond sur les étapes de raisonnement, les appels d'outils, et
le temps d'horloge écoulé. Un agent ne peut ni l'augmenter ni le
supprimer. L'autonomie n'est délégable que si elle possède une
limite dure, et c'est pourquoi cette limite vit dans le runtime plutôt que
dans les bonnes intentions de l'agent. Cette page ne réexplique pas le
mécanisme : c'est l'un des piliers du
[modèle de redevabilité](/explanation/accountability-model). Un détail
honnête : le plafond d'une exécution d'agent est fixé dans le code, et aucune
clé d'`apollia.toml` ne le change. Le registre complet de ce qui
est partiel se trouve dans
[Risques et dette technique](/architecture/risks-and-technical-debt).

## 8. CLI humaine, API machine

Chaque surface est double : un humain lit un terminal, une machine lit du
JSON. Un indicateur global `--json` et la détection de TTY font qu'une même
commande sert à la fois un opérateur devant une invite de commande et un
produit hôte pilotant le runtime via son API. Cela existe parce qu'Apollia est
conçu pour être embarqué, pas seulement utilisé, et un runtime embarquable
doit parler les deux langages sans que l'un ne compromette l'autre. La surface
de commande est la [référence CLI](/reference/cli) ; la surface machine est la
[référence API HTTP](/reference/api/apollia-os-runtime-api).

## Pourquoi ces huit principes, ensemble

Pris isolément, chaque principe est un choix d'ingénierie raisonnable. Pris
ensemble, ils constituent la proposition de valeur : un runtime que vous
pouvez exécuter sans le cloud, déployer sans stack, auquel vous pouvez
déléguer sans perdre le contrôle, et que vous pouvez embarquer sans
rétro-ingénierie. C'est pour cela que l'autonomie, ici, est quelque chose
qu'une équipe régulée peut véritablement adopter, et pas seulement admirer.

## À lire aussi

- [Souveraineté et local-first](/explanation/sovereignty-and-local-first)
- [Le modèle de redevabilité](/explanation/accountability-model)
- [Contraintes](/architecture/constraints)
- [Stratégie de solution](/architecture/solution-strategy)
