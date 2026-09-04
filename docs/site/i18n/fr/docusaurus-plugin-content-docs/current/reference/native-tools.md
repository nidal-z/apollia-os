---
sidebar_position: 5
title: Catalogue des outils natifs
---

# Catalogue des outils natifs

Les outils natifs que le runtime met à disposition des agents dès l'installation.
Ils sont câblés en un seul endroit (`crates/apollia-tools/src/native_dispatcher.rs`).

<!-- claim:chat-tool-governance-path -->
Sur le chemin du chat, chaque appel passe par la même route gouvernée : la
porte d'approbation humaine avec des règles de permission persistées (les
règles d'autorisation par nom seul pré-autorisent un outil, les règles à
préfixe d'argument sont évaluées à chaque invocation, et les exécuteurs de
code ne sont jamais autorisés en bloc), et le budget de pas du palier
d'autonomie.

<!-- claim:audit-list-reads-the-tool-invocation-trail -->
Les deux registres d'audit ne voient pas les mêmes appels. La piste
d'invocations d'outils, la table plate que `audit list` lit, n'est écrite que
depuis `ctx.tools` sur le chemin agent : un appel d'outil du chat n'y figure
donc jamais. Le journal chaîné par hachage, lui, enregistre bien le chat : la
boucle ReAct de conversation émet `ToolOutputCaptured` pour chaque résultat
d'outil et `LlmResponseCaptured` pour chaque complétion, et le souscripteur du
journal transforme les deux en entrées signées et chaînées. Les sorties d'outils
et les complétions du modèle d'une session de chat sont donc persistées
verbatim dans un registre en ajout seul, lisible avec `audit show <run>` et
vérifiable avec `audit verify`. Une session de chat conserve en outre
l'enregistrement d'appel d'outil porté par son message d'assistant, sérialisé
dans la colonne `tool_calls_json` de `chat_messages`, qui n'est pas un
enregistrement audité.

Un agent accède à ces outils via `ctx.tools`, ou se les voit transmis dans
une boucle ReAct via `ctx.tools.describe(<name>)`. Chaque appel est distribué
par le nom canonique de l'outil listé ci-dessous.

## Lister l'état effectif

`apollia-os tools list` affiche chaque outil natif avec son état
d'activation, son backend actif et son statut d'identifiants :

```sh
apollia-os tools list
```

Désactivez ou réactivez un outil avec `apollia-os tools disable <name>` et
`apollia-os tools enable <name>`. Le réglage est écrit dans la base de
gouvernance, et il gouverne le chemin agent seulement : les runners d'agent
fusionnent cet instantané dans la liste des outils désactivés du dispatcher, si
bien qu'un agent qui invoque un outil désactivé reçoit une erreur `UnknownTool`.

Le chemin de conversation du bureau ne lit pas la base de gouvernance. Il
construit son dispatcher depuis la liste `disabled` de `[tools]` dans
`apollia.toml`, où `tools disable` n'écrit jamais. Six natifs échappent même à
cette liste : `file_write`, `file_edit`, `notebook_edit`, `bash_executor`,
`python_executor` et `http_fetch` sont retirés du dispatcher puis réinjectés
derrière leur enrobage d'approbation, sans condition, donc une conversation peut
toujours les appeler quoi que dise la configuration. Restreindre ce qu'une
conversation atteint passe par la porte d'approbation et le sélecteur d'outils
du chat, pas par `apollia-os tools disable`.

## Disponibilité et identifiants

La plupart des outils sont toujours compilés. Quatre sont conditionnés par
une feature de build et sont absents si le runtime est compilé sans elle. Un
outil lit un identifiant optionnel.

| Outil | Feature de build | Identifiant |
|---|---|---|
| `http_fetch` | `http` | aucun |
| `web_search` | `web-search` | `brave.api_key` (optionnel ; repli sur DuckDuckGo) |
| `web_read` | `web-read` | aucun |
| `memory_search` | `memory-search` | aucun |
| tous les autres outils natifs | toujours compilé | aucun |

Les outils `permission_rule_*` requièrent en plus qu'une base de gouvernance
soit configurée ; en son absence, ils ne sont pas enregistrés.
`python_executor` requiert un Python 3 système sur l'hôte (voir les notes de
plateforme ci-dessous).

Stockez la clé Brave optionnelle avec :

```sh
apollia-os tools credentials set web_search brave.api_key
```

## Exécution de code

| Outil | Objectif | Paramètres clés |
|---|---|---|
| `bash_executor` | Exécute une commande shell. Préférez des commandes ciblées et rapides plutôt que des balayages larges. | `command`, `timeout_secs`, `working_dir` |
| `python_executor` | Exécute du code Python dans le virtualenv propre à l'agent (seuls les paquets préinstallés sont disponibles). | `code`, `timeout_secs` |

### Disponibilité par plateforme

Ce qui confine le processus enfant lancé diffère selon l'OS ; le vocabulaire
et la vue d'ensemble se trouvent dans
[le modèle de confiance de l'agent](/explanation/agent-trust-model).

| OS | `bash_executor` | `python_executor` | Confinement du processus enfant |
|---|---|---|---|
| Linux | disponible, s'exécute via `/bin/sh` | disponible, nécessite `python3` ou `python` | namespaces PID + mount via `unshare` (nécessite `CAP_SYS_ADMIN`), plus des limites de ressources |
| macOS | disponible, s'exécute via `/bin/sh` | disponible, nécessite `python3` ou `python` | limites de ressources uniquement (CPU, fichiers ouverts), aucun sandbox OS |
| Windows | nécessite un shell POSIX présent dans `PATH` (Git Bash, MSYS2 ou WSL) | disponible, nécessite un Python 3 installé | aucun |

<!-- claim:bash-executor-requires-posix-shell -->
Sur Windows, `bash_executor` refuse avec une erreur qui nomme le prérequis
manquant lorsqu'aucun shell POSIX n'est présent dans `PATH` ; `cmd.exe` et
PowerShell ne sont jamais utilisés, car la validation des commandes encode
la sémantique du shell POSIX. Un seul shell résolu valide et
exécute chaque commande, sur chaque OS.

<!-- claim:python-executor-locates-windows-interpreter -->
`python_executor` localise l'interpréteur système selon la plateforme : sur
Windows, il sonde `python`, puis le lanceur `py -3`, puis `python3` en
dernier, et rejette le stub du Microsoft Store qui répond au nom `python3`
sur les installations de base.

<!-- claim:unavailable-tool-surfaces-reason -->
Un outil d'exécution de code qui ne peut pas démarrer sur cet hôte reste
invocable et renvoie la raison de son indisponibilité (ce qui manque,
comment l'installer) au lieu d'une simple erreur `UnknownTool`. Cela vaut
aussi bien pour une session de chat que pour un agent installé.

<!-- claim:python-venv-created-on-first-use -->
`python_executor` s'exécute à l'intérieur d'un virtualenv, jamais
directement contre l'interpréteur système. Un agent installé obtient le
sien, provisionné à partir des paquets que déclare son manifest. Une session
de chat n'en déclare aucun et partage un virtualenv unique, créé la première
fois qu'un chat exécute réellement du Python ; ce premier appel paie donc
quelques secondes, les suivants non. Deux échecs restent distincts :
l'absence de Python 3 sur l'hôte est signalée dès la construction et indique
comment en installer un, tandis qu'un virtualenv qui n'a pas pu être créé
rapporte ce que `python -m venv` a refusé.

## Système de fichiers

<!-- claim:tool-sandbox-covers-child-processes-only -->
Chaque outil de système de fichiers est restreint à la racine de l'espace de
travail de l'agent par une vérification de préfixe de chemin canonicalisé,
une garantie applicative et non un sandbox OS (le modèle de confiance
réserve ce mot au confinement du processus enfant).

<!-- claim:absolute-paths-resolve-inside-workspace-root -->
Les chemins peuvent être relatifs à cette racine ou absolus : un chemin
absolu est accepté quand sa forme canonique reste sous la racine, de sorte
que les alias de plateforme d'un chemin situé dans la racine (macOS `/var`
face à `/private/var`, les préfixes verbatim Windows `\\?\`) se résolvent
au lieu d'être refusés. Les échappatoires par lien symbolique et tout chemin
dont la cible réelle sort de la racine sont rejetés.

<!-- claim:chat-file-root-is-home-without-project -->
Le répertoire qui fait office de racine dépend de la session. Avec un projet
ouvert, c'est le répertoire du projet. Dans un chat sans projet, c'est le
`default_workspace` de `[chat]` quand l'opérateur en a configuré un qui existe,
et `~/.apollia` sinon ; la barrière sur ce chemin est l'approbation qui vous est
demandée avant une écriture, pas une racine plus étroite. Votre répertoire
personnel est le repli quand aucun des deux ne se résout, et le répertoire
temporaire système seulement lorsque le répertoire personnel ne peut être résolu
du tout. En pratique `~/.apollia` existe après le premier démarrage, donc un
chat libre est enraciné là et non dans votre répertoire personnel.

| Outil | Objectif | Paramètres clés |
|---|---|---|
| `file_read` | Lit un fichier, avec un offset et une limite optionnels pour les fichiers volumineux. Renvoie du texte UTF-8 avec numéros de ligne. | `path`, `offset`, `limit` |
| `file_write` | Écrit du contenu dans un fichier, en créant les répertoires intermédiaires et en écrasant le fichier s'il existe déjà. | `path`, `content` |
| `file_list` | Liste les fichiers et répertoires avec leur type et leur taille, de façon récursive en option. | `dir`, `recursive` |
| `file_edit` | Remplace un texte exact dans un fichier. Échoue si `old_text` est introuvable ou n'est pas unique (sauf avec `replace_all`). | `path`, `old_text`, `new_text`, `replace_all` |
| `file_glob` | Trouve les fichiers correspondant à un motif glob (`**` pour la récursivité), triés par date de modification. | `pattern`, `path` |
| `file_grep` | Recherche un motif regex dans le contenu des fichiers ; renvoie les lignes correspondantes avec le chemin, le numéro de ligne, et un contexte optionnel. Les fichiers binaires sont ignorés. | `pattern`, `path`, `glob`, `context_lines`, `case_insensitive`, `max_results` |

## Notebooks

Outils pour les notebooks Jupyter `.ipynb`, confinés à la même racine
d'espace de travail que les outils de système de fichiers, nbformat v4
uniquement.

| Outil | Objectif | Paramètres clés |
|---|---|---|
| `notebook_read` | Lit et met en forme les cellules d'un notebook (type et source) pour consommation par un LLM. | `path` |
| `notebook_edit` | Modifie un notebook via des opérations atomiques sur les cellules : modifier la source (les sorties sont effacées), insérer, supprimer, ou mettre à jour les métadonnées. Appliquées dans l'ordre. | `path`, `operations` |

## Réseau

| Outil | Objectif | Paramètres clés |
|---|---|---|
| `http_fetch` | Effectue des requêtes HTTP GET/POST/PUT/PATCH/DELETE. Renvoie le statut, les en-têtes et le corps (plafonné à 1 Mo). Sur le chemin agent il refuse par défaut : aucun chemin de code ne fournit la liste d'autorisation d'hôtes, donc chaque appel est refusé avec `no_allowlist`. Sur le chemin de conversation, un enrobage autorise l'hôte de l'URL demandée pour ce seul appel, donc n'importe quel hôte public est joignable une fois l'appel approuvé ; la garde SSRF refuse toujours les adresses privées, de boucle locale et de lien local. | `url`, `method`, `headers`, `body`, `timeout_secs` |
| `web_search` | Recherche sur le web et renvoie des résultats classés (titre, URL, extrait). Utilise DuckDuckGo par défaut ; passe sur Brave quand une clé est configurée. | `query`, `max_results` |
| `web_read` | Récupère une URL publique et renvoie le texte de l'article extrait, lisible. Rejette les adresses privées, loopback et link-local (garde SSRF). HTML et texte brut uniquement. | `url`, `max_chars`, `include_metadata` |

`web_search` ne respecte pas la liste d'autorisation réseau de l'agent :
l'activer dans le sélecteur d'outils du chat constitue le consentement
explicite de l'utilisateur à une sortie réseau vers un moteur de recherche.
Le contenu renvoyé par `web_read` et `web_search` provient de sites tiers
non fiables et est traité comme des données, jamais comme des instructions.

## Mémoire

| Outil | Objectif | Paramètres clés |
|---|---|---|
| `memory_search` | Recherche plein texte (FTS5, classement BM25) sur l'espace de noms propre de l'agent et les espaces de noms partagés déclarés. Les opérateurs FTS5 sont échappés automatiquement. | `query`, `namespace`, `limit`, `source` |

La récupération de mémoire est toujours à l'initiative de l'agent : le
runtime n'injecte jamais de mémoire dans le prompt d'un agent. L'assistant
conversationnel intégré fait exception, et ce n'est pas par ces outils qu'il
procède. Voir [les huit principes](/explanation/the-8-principles).

## Interaction

| Outil | Objectif | Paramètres clés |
|---|---|---|
| `ask_user` | Pose à l'utilisateur une ou plusieurs questions et attend ses réponses. Prend en charge les questions ouvertes, à choix unique et à choix multiple. | `questions`, `context` |

`ask_user` n'est enregistré que lorsque le runtime fournit un canal d'entrée
en attente (chat interactif). Les agents en mode tâche renvoient à la place
un résultat `input_required`.

## Gouvernance des permissions

Gestion des règles de permission dans la base de gouvernance, pilotée par
l'agent. Chacune de ces actions est soumise à une approbation humaine dans
la boucle avant de prendre effet.

| Outil | Objectif | Paramètres clés |
|---|---|---|
| `permission_rule_add` | Persiste une nouvelle règle de permission, étiquetée avec l'identité de l'agent appelant. `arg_prefix` restreint la règle aux arguments commençant par ce préfixe, évalué à chaque invocation ; pour un exécuteur de code, elle ne couvre jamais qu'une seule commande simple. | `tool_name`, `action` (`allow`/`deny`), `scope` (`global`/`project`/`agent`), `arg_prefix`, `project_path`, `agent_id`, `expires_at` |
| `permission_rule_remove` | Supprime une règle par son id. | `rule_id` |
| `permission_rule_list` | Liste les règles, avec filtrage optionnel. Lecture seule. | `tool_name`, `created_by`, `scope` |

Pour savoir comment les règles de permission et les paliers d'autonomie
déterminent ce qui s'exécute sans demander, voir
[Paliers d'autonomie](/explanation/autonomy-tiers) et
[le modèle de responsabilité](/explanation/accountability-model).
