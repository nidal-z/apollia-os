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
| la piste d'invocations d'outils | une table SQLite plate des appels d'outils : ce qui a été exécuté, une empreinte de ses entrées, son succès, sa durée. Ni chaînage par hachage, ni signature. Les mises à jour comme les suppressions sont refusées par un déclencheur | `audit list`, `audit stats` |
| le journal chaîné par hachage | des entrées rattachées à une exécution, chaînées deux fois, à l'entrée précédente de leur exécution et à l'entrée précédente de n'importe quelle exécution, signées au moment de l'ajout, avec une ancre de tête exportable | `audit journal`, `audit export`, `audit show`, `audit verify`, `audit anchor`, `audit replay` |

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
réponse entière et sans aucun avertissement. Par l'API, la route prend un
`offset` qui passe ce plafond, comme dans
`GET /api/v1/audit?limit=500&offset=500`.

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
l'avance. Elle affiche une ligne par entrée : quand, quel agent, ce qui s'est
passé, avec quel outil, sur quoi et avec quel résultat, puis l'exécution de
l'entrée, sa position dans la chaîne de celle-ci, et si l'entrée porte une
signature. La ligne est un résumé et ne contient aucun contenu. D'un appel
d'outil, elle garde la cible, c'est-à-dire le seul argument que l'outil désigne
comme tel (un chemin, un destinataire, l'hôte d'une adresse) et aucun autre ; un
outil MCP n'en désigne aucun. Un même appel d'outil apparaît en deux entrées,
une au démarrage et une à la fin, parce que le journal enregistre des événements
et non des invocations.

Les arguments et la sortie d'un appel se lisent à la demande, depuis les entrées
que l'exécution a gardées pour la relecture : `audit show` affiche l'exécution
entière, et par l'API `GET /api/v1/audit/journal/<run>/entries/<seq>` rend une
entrée avec les arguments et la sortie de son appel. Un appel fait par le code
d'un agent, sans qu'un modèle l'ait demandé, n'a pas d'arguments capturés à
montrer.

Le journal tient toutes les exécutions de l'instance. Cinq filtres resserrent
une page, et ils se cumulent ; `--limit` et `--offset` comptent alors les
entrées qui les passent :

| Filtre | Garde | Par l'API |
|---|---|---|
| `--agent` | les entrées qui nomment l'agent, et les exécutions de ses tâches | `agents=` |
| `--family` | une famille d'action : `read`, `read_personal`, `write_or_delete`, `send_external`, `execute_code` | `family=` |
| `--nature` | `action` (un appel, un consentement, un refus), `access_change` (un accès, une connexion, un secret, une installation, une règle), `policy`, ou `system` (ce que le journal garde pour la relecture) | `nature=` |
| `--result` | `success`, `failure` ou `refused` | `result=` |
| `--since`, `--until` | une période, en jours lus en UTC ou en instants RFC 3339, la fin exclue | `since=`, `until=` |

Chaque filtre se répète en ligne de commande et prend des valeurs séparées par
des virgules par l'API, comme dans
`GET /api/v1/audit/journal?agents=<agent>,<agent>&result=refused` :

```sh
apollia-os audit journal --agent flux-quotes --agent flux-invoices --limit 20
apollia-os audit journal --nature access_change --since 2026-10-01
apollia-os audit journal --family send_external --result refused
```

## Exporter le journal

Pour remettre le journal à un audit ou le conserver pour une revue,
exportez-le dans un fichier, en CSV ou en JSON. `--format` et `--output` sont
tous deux obligatoires, et les cinq filtres de `audit journal` restreignent
l'export de la même façon :

```sh
apollia-os audit export --format csv --output journal.csv
apollia-os audit export --format json --since 2026-10-01 --until 2026-11-01 --output october.json
apollia-os audit export --format csv --agent flux-quotes --result refused --detail --output refused.csv
```

Le fichier contient toutes les entrées qui passent les filtres, de la plus
récente à la plus ancienne. L'export n'est pas paginé : il n'y a pas de
`--limit`, donc un fichier n'est jamais la partie la plus récente du journal
sous un nom qui annonce le tout.

<!-- claim:audit-export-holds-no-content-without-detail -->
**Sans `--detail`, un export ne contient aucun contenu.** Chaque entrée est le
résumé qu'affiche `audit journal` (`ts`, `agent`, `nature`, `family`, `what`,
`tool`, `target`, `outcome`) et sa place dans la chaîne (`run_id`, `seq`,
`kind`, `hash`). `--detail` ajoute la charge utile de chaque entrée et, pour un
appel d'outil, ses arguments, sa sortie et son statut : en CSV sous la forme de
quatre colonnes de plus (`payload`, `arguments`, `output`, `status`, les trois
premières contenant du JSON), en JSON sous la forme d'un objet `detail` sur
chaque entrée. C'est là que des données personnelles peuvent se trouver, et la
commande le dit sur stderr quand elle écrit un tel fichier.

Le runtime répond le contenu du fichier et la commande l'écrit, donc un export
va vers un chemin de cette machine et nulle part ailleurs. Chaque export est
lui-même inscrit au journal, sous l'exécution `governance:journal:export`, avec
son format, ses filtres, le nombre d'entrées et la présence du détail. Sans
runtime, la commande sort avec le code 2 et n'écrit rien.

Le même export est disponible via l'API HTTP pour une intégration hôte, sous
la forme `GET /api/v1/audit/export?format=csv&detail=true&agents=<agent>,<agent>` ;
voir les opérations d'audit dans la
[référence de l'API HTTP](/reference/api/apollia-os-runtime-api).

Deux choses méritent d'être connues avant de s'appuyer sur l'un ou l'autre
registre. Un appel d'outil effectué dans une session de **chat** n'atteint pas la
piste d'invocations d'outils : cette piste est écrite depuis le `ctx.tools` d'un
agent, et le chemin du chat n'y passe pas, donc `audit list` ne montrera pas
cet appel. Il atteint en revanche le journal chaîné par hachage :
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
écrit le format que nomme `--format`. `--json` est une option globale, donc
`audit export --json` est accepté aussi, mais cela ne change rien au fichier :
cela modifie ce que la commande affiche une fois le fichier écrit, et la forme
d'un message d'erreur.

## Conservation et politique d'organisation {#retention-and-the-organisation-policy}

Le journal n'a aucune fonction d'effacement : ni commande, ni route, ni écran ne
retire une entrée. Ce qui existe, c'est la conservation, et seule une politique
d'organisation signée la fixe. Sa section `[journal]` tient quatre clés :

| Clé | Effet |
|---|---|
| `required = true` | Sans journal ouvert, le runtime refuse toute action sensible et toute nouvelle connexion. La lecture continue, et la fenêtre dit pourquoi. |
| `retention_days = N` | Au démarrage, puis une fois par jour, les entrées de plus de `N` jours sont retirées. |
| `export.detail = "forbid"` ou `"require"` | Le détail d'un export n'est plus un choix : `audit export` demandé dans l'autre sens est refusé avec la raison. |
| `export.daily_summary_dir = "<dossier>"` | Un fichier de résumé par jour UTC, `journal-AAAA-MM-JJ.json`, est écrit dans ce dossier du poste, avec l'ancre de tête de la chaîne. |

Un retrait par conservation est lui-même une entrée, `journal_purged`, chaînée
et signée comme les autres. Elle porte l'ancre de ce qui a été retiré : `audit
verify` sans argument repart d'elle, reste vert, et dit d'où il est parti :

```text
OK    whole journal  412 entries across 18 runs verified
      verified from global_seq=9051 (2026-07-02T06:14:09Z): earlier entries were removed by the retention of the organisation policy on 2026-09-30T00:01:00Z
```

Avec `--json`, la même information est l'objet `start` du rapport. Une entrée
altérée avant ou après l'entrée de purge reste signalée, de même qu'un journal
qui commence plus tard que ne le dit sa dernière entrée de purge.

L'export quotidien passe avant la conservation, et rattrape au démarrage les
jours où le runtime ne tournait pas : un jour atteint le dossier avant que ses
entrées soient retirées. Un dossier inutilisable ne refuse rien : la fenêtre le
dit, et le journal enregistre `journal_export_failed`. Sans politique, rien
n'est exporté à heure fixe et rien n'est jamais retiré.

Ce que cela ne fait pas, c'est empêcher : le journal et sa clé sont dans votre
propre dossier, une modification est donc détectée par `audit verify`, jamais
bloquée. Un journal coupé à la fin n'est détecté que face à une ancre conservée
hors du poste, celle que les fichiers quotidiens, ou `audit anchor`, vous
donnent à garder.

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
