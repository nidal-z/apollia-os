---
sidebar_position: 7
title: 7. Préoccupations transversales
---

# 7. Préoccupations transversales

Ces préoccupations n'appartiennent à aucun crate en particulier : elles traversent tout le runtime.

## Souveraineté et local-first

<!-- claim:daemon-binds-tcp-by-default -->
Le chemin par défaut ne quitte jamais la machine. L'inférence peut s'exécuter
entièrement en local, sur un modèle que sert le moteur embarqué, le stockage est
du SQLite local, et l'API se lie à une socket Unix et au TCP de bouclage, que le
daemon ouvre à chaque démarrage. C'est le bouclage qui garde le défaut local :
joindre l'API depuis une autre machine demande une adresse de liaison hors
bouclage, configurée volontairement, et le daemon refuse celle qui ne porte
aucun jeton porteur. Un modèle distant est optionnel, atteint à une adresse que
l'utilisateur configure, et même dans ce cas le modèle local reste le choix par
défaut. La
mémoire peut être exportée et importée par l'utilisateur, si bien que les
données lui appartiennent. Ce n'est pas une fonctionnalité rapportée après
coup : c'est la contrainte qui façonne chaque défaut. Voir
[Contraintes](/architecture/constraints).

## Le modèle de responsabilité

<!-- claim:rollback-journal-has-no-writer -->
L'autonomie n'est délégable que si elle est responsabilisée. Chaque action
gouvernée est enregistrée dans un journal signé et chaîné par hachage, et
l'intégrité de cet enregistrement peut être vérifiée. Annuler ce qu'un agent a
écrit n'en fait pas partie : le format du journal et la logique de rejeu
existent dans la base de code, mais rien n'installe ce journal sur les outils
qui écrivent des fichiers, si bien que rien n'est enregistré et que rien ne peut
être annulé. C'est la réponse du runtime aux questions : qu'a fait l'agent, et
peut-on faire confiance à l'enregistrement.

Cette notion a sa propre page, qui met aussi en correspondance les contrôles
avec l'AI Act européen. Cette section ne la duplique pas : lire
[le modèle de responsabilité](/explanation/accountability-model).

## Garde-fous non négociables

Le runtime impose un budget de pas sur chaque exécution autonome : un plafond
sur le nombre de pas de raisonnement, sur le nombre d'appels d'outils, et sur
le temps réel écoulé. Il est imposé par le runtime, pas par l'agent, et ne
peut pas être contourné. Les exécutions directes comme les exécutions
orchestrées sont toutes deux bornées, avec un défaut prudent plutôt qu'un
budget illimité. C'est la limite dure qui empêche une exécution de boucler ou
de dépenser sans borne. Le plafond d'une exécution d'agent est fixé dans le
code, et aucune clé d'`apollia.toml` ne le change ; une exécution de workflow a
ses propres bornes, `[workflows] max_steps` et `wall_clock_secs`.

Un agent ne porte aucun code : ce sont donc ces garde-fous du runtime et la
décision de gouvernance sur chacun de ses appels d'outils qui tiennent la ligne
autour de lui. Le Python qu'un paquet apporte est une autre affaire : il
s'exécute dans le processus comme du code de confiance, sans bac à sable du
système d'exploitation autour de lui. Le
[modèle de confiance de l'agent](/explanation/agent-trust-model) explique
cette posture et ses limites en détail.

## Permissions et paliers d'autonomie

<!-- claim:executor-guard-blocks-command-chaining -->
Avant qu'un appel d'outil d'un agent ne s'exécute, une seule décision dans
`apollia-runtime` y répond, dans un ordre fixe : un refus de la politique de
l'organisation, puis une règle fine `deny`, puis la famille de l'outil (un outil
qui ne fait que lire s'exécute), puis, pour tout ce qui est sensible, un accord
permanent de l'agent pour cette famille, ou une demande à laquelle une personne
répond par une fois, toujours ou refuser. La réponse est enregistrée dans le
journal signé. Une règle fine nomme un outil et un préfixe optionnel de son
premier argument, et pour un exécuteur de code la correspondance passe par
le garde-fou (`is_single_simple_command`) qui refuse une commande shell qui
chaîne, redirige, pipe ou substitue, de sorte qu'une approbation accordée pour
une commande ne puisse pas en faire passer une seconde en douce. Ce que ce
garde-fou surveille est l'injection **shell**, pas l'injection de prompt :
Apollia ne livre aucune défense contre l'injection de prompt. La
[vue d'exécution](/architecture/runtime-view) donne l'ordre ligne par ligne.

Une règle a pour portée un agent ou tous les agents, et un accord permanent un
agent et une famille d'action ; rien n'a pour portée un projet ou une session.
Séparément, un palier d'autonomie s'applique à une exécution :
il gouverne la porte du plan et la passe de vérification après exécution, et il
ne gouverne aucune règle de permission ni aucun point de contrôle humain sur un
appel d'outil. Un palier n'élargit jamais ce qu'un agent a le droit de toucher,
seulement la distance qu'une exécution parcourt avant de s'arrêter pour
demander.

## Mémoire à l'initiative de l'agent

<!-- claim:memory-injection-confined-to-builtin-assistant -->

Le runtime n'injecte jamais automatiquement de contexte mémoire dans le prompt
d'un agent, et n'écrit rien en mémoire à sa place. Un agent se remémore quand il
le décide, avec l'outil `memory_search`, et garde une note avec `memory_write`.
Cocher Mémoire sur la fiche de l'agent est la seule chose qui accorde les deux
outils, et aucun ne demande d'accord. La mémoire d'un agent est un espace nommé
d'après lui, gardé d'une conversation et d'une tâche à l'autre. Un agent dont la
source Profil est cochée peut proposer un fait pour le profil avec
`profile_propose`, et cet appel est soumis à la personne à chaque fois. Le
Python d'un paquet ne reçoit pas de `ctx.memory`. Cela garde l'assemblage du
contexte explicite et auditable plutôt que de le transformer en effet de bord
caché, et c'est un principe délibéré, pas un oubli.

Trois voies échappent à cette règle : un résumé du profil de la conversation, à
tous les paliers d'autonomie tant que l'interrupteur de profil de cette
conversation est allumé, des résumés de sessions passées sur le premier message
d'une session de chat libre, et le rôle, la fonction et les outils de ce profil
lorsque l'opérateur clique sur « Améliorer la demande » dans un composeur de chat. Les
deux premières vivent dans l'assistant conversationnel intégré, dans son
constructeur de prompt et son gestionnaire de chat. La troisième vit en dehors de
l'assistant, dans la commande de réécriture du bureau, qui construit son propre
prompt ponctuel et rend du texte au composeur plutôt qu'à une exécution. Aucun
chemin d'exécution d'agent n'atteint l'une des trois, ce qui explique pourquoi le
principe tient là où il est énoncé.

<!-- claim:rewrite-injects-work-context -->
La troisième mérite d'être située précisément, car elle est la plus récente et
la moins visible : la requête de réécriture ne porte ces trois champs du profil
que lorsque l'opérateur la déclenche, et sa sortie arrive dans le
composeur, où elle peut encore être modifiée ou abandonnée avant tout envoi.
Voir [les huit principes](/explanation/the-8-principles).

## Observabilité

Le runtime émet des événements structurés sur un EventBus avec des champs
typés, et non des chaînes de log en texte libre. Le journal d'audit
s'abonne à ce bus, ce qui explique comment la responsabilisation et
l'observabilité partagent un seul flux d'événements. Le traçage utilise des
champs structurés de bout en bout, si bien qu'une exécution reste inspectable
après coup.
