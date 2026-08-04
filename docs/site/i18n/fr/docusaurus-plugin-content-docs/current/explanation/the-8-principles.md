---
sidebar_position: 1
title: Les 8 principes
---

# Les 8 principes

Apollia est un runtime souverain pour agents IA autonomes. Il exécute n'importe
quel agent Python en isolation, localement, avec des outils, et sans dépendance
au cloud. Huit principes tiennent cette promesse ensemble. Ce ne sont pas des
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

<!-- claim:agent-contract-is-decorators-not-manifest-run -->
Un agent est une classe décorée par `@agent`, comportant au moins une méthode
asynchrone décorée par `@skill` ou `@on_message`. Il n'y a aucune classe de
base à hériter ni aucun framework à adopter. L'objectif est de garder minimale
la surface qu'un auteur doit apprendre, pour que le runtime porte les parties
difficiles (gouvernance, budgets, outils) plutôt que de les reporter sur chaque
agent. Pour le contrat exact que voit un agent, lisez la
[référence SDK](/reference/sdk).

L'ancien contrat, une méthode `manifest()` suivie d'un `run()` asynchrone, a
disparu (ADR-023). Le pont refuse tout objet dépourvu de
`__apollia_dispatch__`, l'attribut que les décorateurs installent, si bien
qu'un agent écrit à l'ancienne manière ne se charge pas du tout plutôt que de
fonctionner à moitié.

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
Apollia n'injecte jamais de mémoire dans le prompt d'un agent. Un agent
rappelle ce qu'il décide de rappeler, quand il décide de le faire, via
`ctx.memory`. L'injection automatique de mémoire est pratique et discrètement
corrosive : elle rend les entrées d'une exécution opaques et son comportement
difficile à attribuer. Laisser le rappel à l'initiative de l'agent garde
honnête la trace de ce qui a nourri une décision.

Deux exceptions existent, toutes deux confinées à l'assistant conversationnel
intégré, celui qui se trouve derrière la fenêtre de chat. Aucune n'est
atteignable depuis un agent Python que vous installez : elles vivent dans le
générateur de prompt et le gestionnaire de chat propres à l'assistant, et
aucun chemin d'exécution d'agent ne passe par l'une ou l'autre.

La première est une décision de l'opérateur. Au palier d'autonomie le plus
élevé uniquement, `long_autonomous`, l'assistant ajoute un court brief de
persona utilisateur à son prompt système. Les trois paliers inférieurs ne le
font pas.

<!-- claim:cross-session-recall-injects-summaries -->
La seconde n'est pas conditionnée par un palier, et mérite d'être énoncée sans
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
s'attend raisonnablement à être reconnu. C'est aussi le seul endroit du
runtime où la règle « à l'initiative de l'agent » ne tient réellement pas, et
c'est pourquoi cela est écrit ici plutôt que laissé implicite.

La couche mémoire elle-même est exportable et importable, ce qui explique
qu'elle relève autant de la souveraineté que de l'agentivité.

## 7. Garde-fous non négociables

Chaque exécution autonome est bornée par un budget d'étapes que le runtime
impose : un plafond sur les étapes de raisonnement, les appels d'outils, et
le temps d'horloge écoulé. Un agent ne peut ni l'augmenter ni le supprimer
depuis son propre code. L'autonomie n'est délégable que si elle possède une
limite dure, et c'est pourquoi cette limite vit dans le runtime plutôt que
dans les bonnes intentions de l'agent. Cette page ne réexplique pas le
mécanisme : c'est l'un des piliers du
[modèle de redevabilité](/explanation/accountability-model). Un détail
honnête : le plafond est livré avec une valeur par défaut sûre, et la lecture
d'un plafond personnalisé depuis `apollia.toml` au moment de l'exécution reste
un chantier à venir plutôt qu'un chemin achevé. Le registre complet de ce qui
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
