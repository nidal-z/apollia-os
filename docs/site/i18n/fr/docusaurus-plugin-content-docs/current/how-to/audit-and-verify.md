---
sidebar_position: 3
title: Auditer et vérifier une exécution
---

# Auditer et vérifier une exécution

Ce guide couvre le processus de responsabilisation autour d'une exécution
d'agent : lire ce qui a été enregistré et vérifier son intégrité. Il suppose
que vous avez déjà exécuté au moins une tâche ou une session de chat face à un
daemon.

Le runtime tient **deux registres distincts**, et chaque commande ci-dessous en
lit un seul :

| Registre | Ce qu'il contient | Commandes qui le lisent |
|---|---|---|
| la piste d'invocations d'outils | une table SQLite plate des appels d'outils : ce qui a été exécuté, une empreinte de ses entrées, son succès, sa durée. Ni chaînage par hachage, ni signature. Les mises à jour comme les suppressions sont refusées par un déclencheur | `audit list`, `audit stats`, `audit export` |
| le journal chaîné par hachage | des entrées rattachées à une exécution, chaînées deux fois, à l'entrée précédente de leur exécution et à l'entrée précédente de n'importe quelle exécution, signées au moment de l'ajout, avec une ancre de tête exportable | `audit journal`, `audit show`, `audit verify`, `audit anchor`, `audit replay` |

Pour comprendre la logique de ce modèle et son lien avec les exigences
réglementaires, voir [Le modèle de responsabilisation](/explanation/accountability-model).

Une commande de cette seconde liste fait moins que son nom ne le laisse croire.
`audit replay` redérive une exécution depuis sa propre trace capturée, en
consommant les réponses du modèle et les sorties d'outils enregistrées dans
l'ordre de `step_ordinal` ; elle ne rejoue pas l'agent réel, parce que le journal
n'a jamais capturé la question d'origine. Elle répond à la question de savoir si
une trace est complète et cohérente avec elle-même, pas si le code d'aujourd'hui
se comporterait pareil.

## Lire le journal d'audit

`audit list` et `audit stats` lisent la piste d'invocations d'outils : ce
qu'elles affichent est l'enregistrement plat des appels d'outils, pas le journal
chaîné. Lister les événements récents :

```sh
apollia-os audit list --limit 20
apollia-os audit stats
```

Le point de terminaison derrière `audit list` ne sert au maximum que 500
événements par requête, et cette commande ne parcourt pas les pages au-delà :
`--limit 2000` rend les 500 événements les plus récents, présentés comme la
réponse entière et sans aucun avertissement. Utilisez `audit export`, qui
parcourt les pages, quand il vous en faut plus de 500.

Pour lire les entrées d'une exécution donnée dans le journal chaîné par
hachage, y compris les complétions du modèle capturées, résolvez-la par un
identifiant d'exécution ou par un identifiant de tâche qui s'y rattache :

```sh
apollia-os audit show <run-or-task-id>
```

Cette lecture demande d'avoir un identifiant d'exécution sous la main. Pour
parcourir le journal sans en connaître un, de l'entrée la plus récente à la plus
ancienne et toutes exécutions confondues :

```sh
apollia-os audit journal --limit 20
apollia-os audit journal --limit 20 --offset 20
```

C'est la seule lecture du journal chaîné qui ne nomme pas une exécution à
l'avance. Elle affiche une ligne par entrée, avec son exécution, sa position
dans la chaîne de celle-ci, et si l'entrée porte une signature. Un même appel
d'outil apparaît en deux entrées, une au démarrage et une à la fin, parce que le
journal enregistre des événements et non des invocations.

Pour extraire la piste d'invocations d'outils à des fins d'archivage ou de revue
externe :

```sh
apollia-os audit export --output audit.json --limit 100000
```

<!-- claim:audit-export-pages-past-the-server-ceiling -->
**Le point de terminaison ne sert au maximum que 500 événements par
requête**, et la commande parcourt les pages jusqu'à ce qu'une page plus
courte revienne, de sorte que `--limit` borne l'export plutôt que
l'historique atteignable. Elle avertit sur stderr lorsque l'export s'est
arrêté sur votre `--limit` plutôt qu'à la fin du journal, ce qui est le
signal pour l'augmenter.

Les mêmes enregistrements sont disponibles via l'API HTTP pour une
intégration hôte ; voir les opérations d'audit dans la
[référence de l'API HTTP](/reference/api/apollia-os-runtime-api).

Deux choses méritent d'être connues avant de s'appuyer sur l'un ou l'autre
registre. Un appel d'outil effectué dans une session de **chat** n'atteint pas la
piste d'invocations d'outils : cette piste est écrite depuis le `ctx.tools` d'un
agent, et le chemin du chat n'y passe pas, donc `audit list` et `audit export` ne
montreront pas cet appel. Il atteint en revanche le journal chaîné par hachage :
la boucle de chat émet chaque sortie d'outil et chaque complétion du modèle comme
un événement rattaché à une exécution, et le souscripteur du journal en fait des
entrées chaînées. Lisez-les avec `audit show` sur l'exécution, ou avec `audit
journal`. Les deux sont stockées telles quelles, ce qui rend un chat auditable et
met aussi son contenu sur le disque. Et la piste est écrite en pose-et-oublie :
un enregistrement est jeté, avec un avertissement dans les logs, lorsque son
canal est saturé.

## Vérifier l'intégrité d'une exécution

Le journal est une chaîne de hachage avec signatures, ce qui rend toute
altération détectable. `audit verify` a deux formes, et elles ne contrôlent pas
la même chose.

Avec un identifiant d'exécution, elle recalcule la chaîne propre à cette
exécution et ses signatures :

```sh
apollia-os audit verify <run-id>
```

Une vérification réussie de cette forme indique que les entrées de cette
exécution n'ont pas été modifiées, et qu'elles portent une signature valide
**lorsque le journal en est un signé**. La signature n'est pas garantie :
lorsque la clef HMAC ne peut être ni lue ni écrite, le runtime ouvre le journal
non signé et journalise `audit.journal.unsigned_fallback`. La vérification
n'exige alors aucune signature, et une chaîne non signée rend quand même un
résultat correct. Cherchez cet événement dans les logs du runtime, ou la colonne
de signature d'`audit journal`, avant de lire une vérification verte comme une
preuve d'origine. La signature est un HMAC-SHA256, symétrique : elle montre que
la chaîne a été écrite par un détenteur de la clef sur cette machine, et ce n'est
pas quelque chose qu'un tiers peut contrôler de son côté.

La vérification ne dit pas non plus que l'exécution est complète : une troncature
qui retire les dernières entrées laisse une chaîne plus courte qui se vérifie
toujours.

Sans argument, elle parcourt la chaîne globale de toutes les exécutions et
compare la tête terminale à l'ancre persistée, ce qui détecte une suppression
intérieure, une exécution entière supprimée, et une queue tronquée :

```sh
apollia-os audit verify
```

Utilisez la forme sans argument quand la question est de savoir si quelque chose
manque, et la forme par exécution quand la question est l'authenticité des
entrées d'une exécution. Aucune des deux ne protège d'un détenteur de la clef de
signature qui resignerait une chaîne plus courte : c'est l'ancre exportée et
conservée hors machine qui couvre ce cas, et `audit anchor` l'affiche.

Ajoutez `--json` à `audit list`, `audit show`, `audit stats` et `audit
verify` pour obtenir une sortie exploitable par une machine. `audit export`
écrit toujours du JSON. `--json` est une option globale, donc `audit export
--json` est accepté aussi, mais cela ne change rien à l'export lui-même ; cela
ne modifie que la forme d'un message d'erreur.

## En pratique

Un passage type de responsabilisation consiste à utiliser `audit show` pour
lire ce qu'une exécution a fait, puis `audit verify` sans argument pour
confirmer que rien n'a été retiré, puis `audit verify <run-id>` sur l'exécution
en question. Les trois commandes sont en lecture seule : aucune ne modifie quoi
que ce soit sur le disque.

## Voir aussi

- [Le modèle de responsabilisation](/explanation/accountability-model) pour
  comprendre comment ces primitives s'articulent et ce qu'elles permettent.
- La [référence CLI](/reference/cli) pour tous les indicateurs de `audit`.
- La [référence de l'API HTTP](/reference/api/apollia-os-runtime-api) pour
  les points de terminaison d'audit utilisés par une intégration hôte.
