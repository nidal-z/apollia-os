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

<!-- claim:journal-records-the-tool-calls-of-a-run -->
Le journal d'audit enregistre les appels d'outils d'une conversation : la boucle
de conversation annonce chaque appel avec son exécution, émet
`ToolOutputCaptured` pour chaque résultat d'outil et `LlmResponseCaptured` pour
chaque complétion, et le souscripteur du journal en fait des entrées signées et
chaînées. Les sorties d'outils et les complétions du modèle d'une session de
chat sont donc persistées verbatim dans un registre en ajout seul, lisible avec
`audit show <run>` et vérifiable avec `audit verify`. Une session de chat
conserve en outre l'enregistrement d'appel d'outil porté par son message
d'assistant, sérialisé dans la colonne `tool_calls_json` de `chat_messages`, qui
n'est pas un enregistrement audité.

Un agent se voit accorder ces outils par leur nom, dans sa liste d'accès :
`access` sous `[agents.defaults]` dans le
[manifeste de paquet](/reference/package-manifest), que la personne peut
modifier sur la fiche de l'agent. Chaque appel est distribué par le nom
canonique de l'outil listé ci-dessous. Le Python d'un paquet ne les appelle
pas : `ctx.tools` vaut `None` dans un outil comme dans un workflow.

## Lister l'état effectif

`apollia-os tools list` affiche chaque outil natif avec son état
d'activation, son backend actif et son statut d'identifiants :

```sh
apollia-os tools list
```

Désactivez ou réactivez un outil avec `apollia-os tools disable <name>` et
`apollia-os tools enable <name>`. Le réglage est écrit dans la base de
gouvernance. Un outil est désactivé quand cette base le désactive ou quand la
liste `disabled` de `[tools]` dans `apollia.toml` le nomme : les runners d'agent
comme le chemin de conversation lisent l'union des deux, si bien qu'un agent qui
invoque un outil désactivé reçoit une erreur `UnknownTool`.

Sur le chemin de conversation, `file_write`, `file_edit`, `notebook_edit`,
`bash_executor`, `python_executor` et `http_fetch` sont construits à part,
enracinés dans les dossiers accordés à l'agent, et un outil désactivé n'est pas
construit du tout. L'état est relu à chaque message : un outil désactivé pendant
une conversation disparaît dès le message suivant, sans redémarrage.

## Disponibilité et identifiants

La plupart des outils sont toujours compilés. Quatre sont conditionnés par
une feature de build et sont absents si le runtime est compilé sans elle. Un
outil lit un identifiant optionnel.

| Outil | Feature de build | Identifiant |
|---|---|---|
| `http_fetch` | `http` | aucun |
| `web_search` | `web-search` | `brave.api_key` (optionnel ; repli sur DuckDuckGo) |
| `web_read` | `web-read` | aucun |
| `memory_search`, `memory_write`, `profile_propose` | `memory-search` | aucun |
| tous les autres outils natifs | toujours compilé | aucun |

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

<!-- claim:file-tools-root-is-the-granted-folders -->
Les racines sont les dossiers accordés à l'agent, agent de conversation intégré
compris, dans une conversation comme dans une tâche. Un dossier du poste est une
connexion, et ses outils s'accordent à un agent sur sa fiche : les outils de
lecture atteignent tous les dossiers accordés, les outils d'écriture seulement
ceux accordés pour écrire. Le premier dossier accordé, par date d'ajout, est
celui où arrive un chemin relatif. Ni le répertoire personnel, ni le dossier de
données, ni le dossier d'un ancien projet ne sont des racines. Un agent sans dossier
accordé a des outils de fichiers qui refusent, en disant comment en accorder un.

| Outil | Objectif | Paramètres clés |
|---|---|---|
| `file_read` | Lit un fichier, avec un offset et une limite optionnels pour les fichiers volumineux. Renvoie du texte UTF-8 avec numéros de ligne. Un document PDF, Word (`.docx`), PowerPoint (`.pptx`) ou Excel (`.xlsx`, `.xlsm`, `.xls`, `.ods`) est lu comme le texte qu'il contient, en entier ou une page, une diapositive ou une feuille à la fois. | `path`, `offset`, `limit`, `page`, `sheet` |
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
| `memory_write` | Garde une note dans la mémoire propre de l'agent sous une clé (`remember`), en remplaçant ce que la clé contenait, ou la retire (`forget`). Seul l'espace de noms propre de l'agent est écrit. | `action`, `key`, `value` |
| `profile_propose` | Propose un fait durable sur la personne pour son profil. L'appel est soumis à la personne à chaque fois, le fait affiché ; rien n'est écrit avant son accord. Un champ du formulaire de profil est refusé. | `key`, `fact` |
| `source_search` | Recherche dans les dossiers et fichiers indexés cochés pour l'agent, par mots-clés, et par le sens quand un modèle d'embeddings est installé. Chaque résultat porte son extrait, son document, son emplacement, son score et sa source, et la réponse dit comment elle a été classée. Une source non cochée est refusée. | `query`, `sources`, `limit` (de 1 à 50) |
| `code_context` | Lit l'état d'un dossier de code coché pour l'agent, au moment de l'appel : sa branche git, son statut et ses derniers commits, son fichier de règles (`APOLLIA.md`) et son arborescence. Rien du dossier n'atteint le prompt autrement. | `folder`, `parts` (`git`, `rules`, `tree`) |

Chacun de ces outils n'est proposé à un agent que si sa source est cochée pour
lui, quoi que listent ses accès : `memory_search` et `memory_write` quand sa
mémoire l'est, `profile_propose` quand le profil l'est, `source_search` quand au
moins un dossier ou un fichier indexé l'est, `code_context` quand un dossier de
code l'est. Cocher Mémoire sur la fiche d'un
agent est ce qui ouvre sa mémoire, en lecture et en écriture, et aucun des deux
outils de mémoire ne demande d'accord : ils agissent sur l'espace propre de
l'agent. La mémoire d'un agent est un seul espace, nommé d'après l'agent et
gardé d'une conversation et d'une tâche à l'autre.

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

Aucun outil natif ne lit ni n'écrit les règles de permission de la base de
gouvernance : un agent ne peut donc pas s'accorder d'accès. Les règles sont
gérées par l'utilisateur, depuis l'application de bureau ou avec
`apollia-os permissions`.

Pour savoir comment les règles de permission et les paliers d'autonomie
déterminent ce qui s'exécute sans demander, voir
[Paliers d'autonomie](/explanation/autonomy-tiers) et
[le modèle de responsabilité](/explanation/accountability-model).
