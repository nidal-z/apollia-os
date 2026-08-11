---
sidebar_position: 7
title: 7. Préoccupations transversales
---

# 7. Préoccupations transversales

Ces préoccupations n'appartiennent à aucun crate en particulier : elles traversent tout le runtime.

## Souveraineté et local-first

Le chemin par défaut ne quitte jamais la machine. L'inférence peut s'exécuter
entièrement en local sur un modèle GGUF, le stockage est du SQLite local, et
l'API se lie à une socket Unix sauf activation explicite du TCP. L'inférence
cloud est optionnelle, sur la clé propre de l'utilisateur, et même dans ce cas
le modèle local reste le choix par défaut, avec une escalade sous contrôle. La
mémoire peut être exportée et importée par l'utilisateur, si bien que les
données lui appartiennent. Ce n'est pas une fonctionnalité rapportée après
coup : c'est la contrainte qui façonne chaque défaut. Voir
[Contraintes](/architecture/constraints).

## Le modèle de responsabilité

L'autonomie n'est délégable que si elle est responsabilisée. Chaque action
gouvernée est enregistrée dans un journal signé et chaîné par hachage ;
l'intégrité de cet enregistrement peut être vérifiée ; et les modifications du
système de fichiers effectuées dans une session de chat peuvent être annulées.
C'est la réponse du runtime aux questions : qu'a fait l'agent, peut-on faire
confiance à l'enregistrement, et peut-on l'annuler.

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
de dépenser sans borne. Un point reste à câbler : lire le plafond depuis
`apollia.toml` au moment de l'exécution, ce qui dispose aujourd'hui d'un
défaut prudent en attendant.

Le code de l'agent lui-même s'exécute in-process comme du code de confiance,
si bien que ce sont ces garde-fous du runtime et le verrou d'approbation
humaine, et non un bac à sable du système d'exploitation, qui tiennent la
ligne autour d'un agent. Le
[modèle de confiance de l'agent](/explanation/agent-trust-model) explique
cette posture et ses limites en détail.

## Permissions et paliers d'autonomie

<!-- claim:permission-engine-not-wired -->
<!-- claim:executor-guard-blocks-command-chaining -->
Avant qu'une action ne s'exécute, le chemin de dispatch du chat dans
`apollia-runtime` la classe en trois étapes. La première porte est un
**ensemble d'autorisation par nom d'outil** : des règles d'autorisation
persistées, portant uniquement sur le nom, l'amorcent, et les exécuteurs de
code en sont exclus sur toutes les routes. En cas d'absence de correspondance,
la boucle consulte les **règles de préfixe** par invocation : l'argument de
l'appel est comparé aux règles permanentes de l'opérateur, préfixe le plus
long d'abord, et pour un exécuteur de code la correspondance passe en plus par
le garde-fou (`is_single_simple_command`) qui refuse une commande shell qui
chaîne, redirige, pipe ou substitue, de sorte qu'une approbation accordée pour
une commande ne puisse pas en faire passer une seconde en douce. Ce qui reste
déclenche une **approbation humaine dans la boucle** que l'opérateur résout,
et cette décision est enregistrée.

<!-- claim:injection-detector-is-shell-not-prompt -->
`apollia-permissions` contient aussi un `PermissionEngine` agrégeant une liste
sûre et un détecteur d'injection shell. **Il n'est pas actif dans
l'application livrée.** `ToolDispatcher` détient un `Option<PermissionEngine>`
qu'aucun appelant de production ne peuple, si bien que ces deux composants ne
s'exécutent jamais. Ils sont conservés pour un intégrateur qui choisirait de
les activer, et le crate le précise dans sa propre documentation de module. À
noter aussi que le détecteur surveille l'injection **shell**, pas l'injection
de prompt : Apollia ne livre aucune défense contre l'injection de prompt.

Les permissions se portent sur toute l'installation, sur un projet, ou sur une
seule session. Par-dessus, un palier d'autonomie est un curseur que
l'opérateur règle pour déterminer ce qu'un agent peut faire sans demander. Un
même agent peut s'exécuter avec prudence ou avec liberté selon la confiance
que l'opérateur lui accorde.

## Mémoire à l'initiative de l'agent

<!-- claim:memory-injection-confined-to-builtin-assistant -->

Le runtime n'injecte jamais automatiquement de contexte mémoire dans le prompt
d'un agent. Un agent se remémore quand il le décide, via `ctx.memory`. Cela
garde l'assemblage du contexte explicite et auditable plutôt que de le
transformer en effet de bord caché, et c'est un principe délibéré, pas un
oubli.

L'assistant conversationnel intégré échappe à cette règle, de trois façons :
une note de profil utilisateur au palier `long_autonomous`, des résumés de
sessions passées sur le premier message d'une session de chat libre, et la
section Travail du profil utilisateur lorsque l'opérateur clique sur « Améliorer
la demande » dans un composeur de chat. Les deux premières vivent dans le
constructeur de prompt propre à l'assistant et dans le gestionnaire de chat. La
troisième vit en dehors des deux, dans la commande de réécriture du bureau, qui
construit son propre prompt ponctuel et rend du texte au composeur plutôt qu'à
une exécution. Aucun chemin d'exécution d'agent n'atteint l'une des trois, ce
qui explique pourquoi le principe tient là où il est énoncé.

<!-- claim:rewrite-injects-work-context -->
La troisième mérite d'être située précisément, car elle est la plus récente et
la moins visible : la requête de réécriture ne porte la section Travail de
l'opérateur que lorsque celui-ci la déclenche, et sa sortie arrive dans le
composeur, où elle peut encore être modifiée ou abandonnée avant tout envoi.
Voir [les huit principes](/explanation/the-8-principles).

## Observabilité

Le runtime émet des événements structurés sur un EventBus avec des champs
typés, et non des chaînes de log en texte libre. Le journal d'audit
s'abonne à ce bus, ce qui explique comment la responsabilisation et
l'observabilité partagent un seul flux d'événements. Le traçage utilise des
champs structurés de bout en bout, si bien qu'une exécution reste inspectable
après coup.
