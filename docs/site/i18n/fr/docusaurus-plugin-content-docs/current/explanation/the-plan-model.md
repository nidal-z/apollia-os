---
sidebar_position: 5
title: Le modèle de plan et d'exécution orchestrée
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

Chaque requête est d'abord classifiée selon l'un de deux modes d'exécution.
**Direct** est une exécution en une seule étape pour un travail simple.
**Orchestré** est une exécution multi-étapes avec planification, pour un travail
qui nécessite plusieurs appels d'outils pilotés par le raisonnement.

La classification est déterministe et ne fait appel à aucun LLM : c'est une
fonction pure de ce que l'agent et la requête déclarent, ce qui la rend
prévisible.

<!-- claim:execution-mode-classification-weights -->

Le manifeste d'un agent peut fixer `execution_mode` à `direct` ou `orchestrated`,
ce qui tranche directement. Seule la valeur `auto`, ou une valeur non reconnue,
atteint l'heuristique. L'heuristique additionne sept poids indépendants et
compare le total à un seuil :

| Poids | Ajouté quand |
| --- | --- |
| 0.40 | le manifeste porte le tag `multi-step` |
| 0.30 | le budget d'étapes déclaré dépasse 15 |
| 0.20 | la requête comporte plus de 3 parties en entrée |
| 0.20 | l'agent nécessite plus de 4 outils |
| 0.10 | le texte en entrée dépasse 500 caractères |
| 0.10 | l'instantané de mémoire épisodique contient plus de 5 épisodes |
| 0.10 | le prompt système contient des mots-clés de planification |

Le seuil est `[oria] orchestrated_threshold`, et vaut 0.40 par défaut. Deux
conséquences découlent des chiffres plutôt que du texte. Le tag `multi-step` à
lui seul atteint le seuil par défaut, donc étiqueter un agent `multi-step`
équivaut à le déclarer orchestré. Aucun autre facteur pris isolément n'y
parvient : en dessous de ce tag, l'orchestration exige qu'au moins deux signaux
concordent.

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
C'est ce graphe qui rendrait le parallélisme possible. Le moteur parcourt le
plan par niveaux topologiques, et les étapes d'un même niveau peuvent
s'exécuter simultanément lorsqu'il s'agit d'appels d'outils en lecture seule ne
nécessitant aucune approbation.

**Dans le runtime livré, elles ne le font jamais.** Décider qu'une étape est en
lecture seule est délégué au proxy d'outils, et l'unique implémentation en
production conserve le comportement par défaut du trait, qui répond non pour
chaque outil. Chaque étape s'exécute donc de façon séquentielle. Les niveaux
gardent leur utilité : ils ordonnent le plan et expriment ce qui dépend de quoi,
mais ils n'apportent aucun gain de vitesse aujourd'hui. Traitez le plan comme un
graphe de dépendances, pas comme un ordonnanceur.

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
le critique LLM est la partie câblée et opérationnelle ; l'exécution des propres
vérifications shell déclarées par l'agent sous gouvernance n'est pas encore
câblée, cet invocateur est aujourd'hui un no-op. L'auto-vérification est réelle ;
les vérifications shell déterministes restent à faire. Le volet audit de tout
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
