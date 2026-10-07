---
sidebar_position: 8
title: 8. Décisions de conception en vigueur
---

# 8. Décisions de conception en vigueur

Cette page énonce les décisions structurantes qui tiennent aujourd'hui, et
pourquoi. Elle est écrite au présent : elle décrit le système tel qu'il est
construit, pas le chemin qui y a mené. Quand une décision a un coût, le coût est
nommé.

## Socle technique et runtime {#stack-and-runtime}

Le runtime est en Rust sur Tokio. Le code qu'un paquet apporte est en Python,
chargé dans le processus via un pont PyO3 plutôt que lancé en sous-processus :
le pont traduit directement les futures Rust en coroutines Python, donc un appel
à ce code ne coûte ni frontière de processus ni aller-retour de sérialisation.

Tout sous-système qui détient un état mutable est un acteur Tokio, avec un canal
borné et une poignée clonable. Aucun état n'est partagé entre acteurs derrière un
verrou. Le coût est une indirection, et il achète la propriété qui compte à cette
taille : un interblocage ne peut pas se former entre deux sous-systèmes qui
n'échangent que des messages.

La persistance est SQLite en mode WAL, embarqué dans le binaire. Rien, sur le
chemin par défaut, n'exige un serveur, un démon ou un réseau.

## Le contrat d'agent {#agent-contract}

Un agent est une fiche, pas un programme : une mission, un modèle, les outils
qui lui sont accordés, les sources qu'il peut lire, et ses droits. Un paquet le
déclare comme une entrée `[[agents]]` de son manifeste `apollia.toml`, et aucun
objet Python ne le représente. Ce que le paquet déclare est un ensemble de
valeurs par défaut, jamais une contrainte : l'utilisateur change un outil, une
source, le modèle ou le palier d'autonomie depuis l'interface, et son réglage
local l'emporte sans nouvelle version du paquet.

Le code est optionnel. Le Python d'un paquet apporte des outils et des
workflows, des fonctions asynchrones sous `@tool` et `@workflow`, et le SDK ne
publie aucun autre décorateur. `python -m apollia describe` écrit leurs
descripteurs dans le manifeste, si bien que lire un manifeste n'exécute jamais
le code qu'il décrit.

Le runtime lit le manifeste par un seul analyseur, sous un schéma strict. Une
clé que le schéma ne connaît pas est refusée avec son chemin complet, de même
que ce que le schéma ne sait pas exprimer : un identifiant déclaré deux fois, un
accès qui nomme un outil de paquet que le paquet ne livre pas, un paquet qui ne
déclare rien. Un paquet dont le manifeste ne valide pas n'est ni construit ni
installé.

Le runtime remet à chaque outil et à chaque workflow un objet `ctx` qui expose
les services dont il peut se servir : le routeur LLM, la mémoire, les outils,
les autres agents et workflows, les notifications et un logger. `ctx` est toute
la surface du runtime. Un code qui passe à côté utilise quelque chose que le
runtime ne garantit pas.

Les schémas de charge utile sont des `TypedDict`, lus au moment où une fonction
est décorée pour construire le schéma de ses entrées. C'est pourquoi
l'évaluation différée des annotations est refusée dans ces modules : elle
transforme les annotations en chaînes, et le schéma ressort vide sans la moindre
erreur.

Le SDK Python n'a aucune dépendance tierce. Chacune deviendrait une dépendance
de chaque paquet, sur chaque machine, pour toujours.

Ce contrat en a remplacé un autre le 30 septembre 2026, quand le principe 3 a
été réécrit. Jusque-là, un agent était une classe Python sous `@agent`, avec des
méthodes `@skill` ou `@on_message`, et le pont refusait tout objet dépourvu du
point d'entrée de dispatch que ces décorateurs installaient. Deux constats ont
conduit au changement. Un agent qui fixe dans le code les outils qu'il exige est
un workflow écrit en code : la personne qui s'en sert ne peut ni ajouter un
outil ni changer un droit sans nouvelle version. Et un seul nom recouvrait des
objets dont le contrôle n'était pas tenu par la même partie, le modèle dans un
cas et le code de l'auteur dans l'autre, si bien que personne ne pouvait savoir
lequel il avait devant lui. Le coût est une rupture : un agent écrit comme une
classe ne se charge plus, et se réécrit comme une entrée de manifeste dont le
Python passe dans des outils ou dans un workflow.

## Modèle d'exécution {#execution-model}

Deux choses s'exécutent, et une seule question les distingue : qui décide de
l'étape suivante. Pour un agent, c'est le modèle. Pour un workflow, c'est
l'auteur.

Un agent tourne sur le moteur autonome, qui observe, raisonne et agit en boucle.
Sa mission est le prompt système, les accès qui lui sont accordés sont les
outils proposés au modèle, et son modèle comme son palier d'autonomie sont
résolus à l'envoi de chaque message, si bien qu'un changement sur sa fiche
s'applique à une conversation déjà ouverte. Le moteur fonctionne selon deux
modes : direct, où le modèle répond et appelle des outils au fil de l'eau, et
orchestré, où un plan est construit puis ses étapes exécutées. Le réglage de
planification de l'agent choisit : `never`, `always`, ou `auto`, qui planifie
quand la tâche se lit comme une tâche à plusieurs étapes.

Un workflow est du Python dont les étapes sont déclarées d'avance. Son corps
ouvre chaque étape par son nom, et un nom absent de la déclaration est refusé.
Une étape peut lancer un agent ou un autre workflow : l'exécution imbriquée
s'arrête avec celle qui l'a lancée, dépense le même budget, et l'imbrication est
bornée à trois niveaux, la première exécution comptant pour un. Le coût de ce
partage est qu'une suite fixe ne peut plus passer pour un agent. Elle s'écrit
comme un workflow, sa structure est visible, et son utilisateur change ses
entrées, pas ses étapes.

Sur le chemin orchestré, les arguments d'étape sont remplis au moment de la
planification, sous grammaire, avec une extraction à la volée en repli au moment
de l'exécution. C'est ce remplissage à la planification qui permet à un plan de
piloter de vrais outils avec des arguments structurés, au lieu de réanalyser de
la prose à chaque étape.

Une exécution orchestrée terminée est vérifiée par un critique à partir du
palier `supervised` ; le palier `assisted` ne lance aucune passe de
vérification. Le verdict est consigné comme événement du runtime, et un verdict
négatif déclenche une replanification bornée, sous le budget qui bornait déjà
l'exécution d'origine.

## Budget et garde-fous {#budget-and-safeguards}

Toute exécution porte un budget d'étapes à trois dimensions : un nombre maximal
d'étapes, un nombre maximal d'appels d'outils, et un plafond de temps réel. Le
runtime l'applique. Le code de l'agent ne peut ni l'augmenter, ni l'étendre, ni
s'en soustraire, parce que le compteur vit du côté runtime du pont et que
l'agent n'en détient jamais de référence.

C'est le garde-fou sur lequel repose la conception : une boucle autonome qu'on
peut arrêter est un produit, une qu'on ne peut pas arrêter est un risque.

## Supervision {#supervision}

<!-- claim:supervisor-has-no-restart-machinery -->
<!-- claim:supervisor-has-no-child-spec -->
<!-- claim:supervisor-has-no-restart-tracker -->
Le superviseur applique l'échec rapide puis la dégradation. Un acteur qui meurt
n'est pas relancé. Le runtime signale la perte, dégrade la capacité que cet
acteur servait, et continue de servir tout le reste.

Le redémarrage sur crash a été envisagé puis écarté : un acteur qui plante a
déjà perdu l'état qu'il détenait, et le relancer produit un sous-système qui
répond avec une vue plausible mais vide. Une capacité honnêtement absente
s'exploite mieux qu'une capacité qui ment discrètement.

## Outils et confinement {#tools-and-sandbox}

Les outils sont résolus au démarrage, pas au premier appel, si bien qu'un outil
absent ou mal configuré est une erreur de démarrage plutôt qu'un échec au milieu
d'une exécution.

Le jeu d'outils natifs couvre l'exécution shell, la lecture et l'écriture de
fichiers confinées à un chemin, l'exécution Python dans un environnement virtuel
propre à l'agent, la récupération HTTP, la recherche web et la recherche en
mémoire. Les outils de fichiers sont confinés à une racine résolue : le chemin
est canonisé puis revérifié contre la racine après résolution, donc un lien
symbolique ne permet pas d'en sortir.

La racine des outils de fichiers d'un agent est l'union des dossiers accordés à
cet agent, agent de conversation intégré compris, sur le chemin de la
conversation comme sur celui des tâches. Un dossier du poste est une connexion,
choisie au sélecteur du système, et ses outils s'accordent à un agent un par
un : les outils de lecture atteignent tous les dossiers qui lui sont accordés,
les outils d'écriture seulement ceux accordés pour écrire. Le premier dossier
accordé, par date d'ajout, sert d'ancre à un chemin relatif. Un agent sans
dossier accordé a des outils de fichiers qui refusent avec un message disant
comment en accorder un. Rien d'autre n'est une racine : ni le dossier
personnel, ni le dossier de données, ni le volume, ni le dossier d'un projet.
Les anciennes clés `[filesystem] trusted_paths` et `[chat] default_workspace`
sont retirées ; les dossiers qu'elles nommaient, dossier personnel excepté, ont
été ajoutés une fois comme connexions, accordées à aucun agent.

Le confinement n'est pas uniforme selon les plateformes, et la différence n'est
pas cosmétique. Sous Linux, l'exécution shell tourne sous espaces de noms PID et
mount. Sous macOS le confinement est plus faible. Sous Windows il n'y en a pas.
C'est dit franchement plutôt que lissé, parce qu'un opérateur qui choisit une
plateforme choisit un modèle de menace. Voir [le modèle de confiance des
agents](/explanation/agent-trust-model) pour ce que cela implique en pratique.

## HTTP sortant {#outbound-http}

Tout appel HTTP sortant passe par un seul module, `apollia_core::net`, et
`scripts/check_http_clients.py` refuse un client construit ailleurs ou un corps
de réponse consommé par un autre moyen.

Le module porte deux règles. Une destination doit se résoudre hors des plages de
bouclage, privées, lien-local, multicast et unique-local, et hors des suffixes
de domaine internes, vérifié sur la première URL puis à chaque saut de
redirection, donc une URL publique ne peut pas rediriger vers le réseau local.
Un corps de réponse est lu à travers un plafond qui abandonne le flux dès que le
budget est franchi, plutôt que de tamponner la réponse entière et de la mesurer
ensuite, quand la mémoire est déjà dépensée.

Une exception est nommée plutôt que laissée implicite. Une destination que
l'opérateur a configurée peut légitimement être interne : un serveur MCP local,
le runner, un point d'accès de modèle auto-hébergé. Ces clients-là sont
construits par un constructeur distinct, qui garde le plafond de corps et laisse
tomber la vérification d'adresse.

Tenir les deux règles dans `apollia-core` est ce qui met `reqwest` et `url` dans
la crate dont toutes les autres dépendent. Elles sont derrière une option de
compilation `net`. Neuf des crates qui dépendent d'`apollia-core` l'activent ;
les autres gardent la fermeture de dépendances qu'elles avaient, et aucune des
deux n'est nouvelle dans l'arbre : les deux étaient déjà des dépendances de
l'espace de travail, et le fichier de verrouillage n'a gagné aucun paquet. Le
coût est que la crate de socle n'est plus sans dépendance dans toutes les
configurations. Il vaut la peine d'être payé : la politique d'adresses existait
auparavant en deux copies et la lecture plafonnée en six, et chaque règle était
tout simplement absente partout où sa copie n'avait pas été faite.

## Modèle de permissions {#permission-model}

<!-- claim:prefix-rules-evaluated-per-invocation -->
Sur le chemin du chat, un appel d'outil est décidé dans un ordre fixe. Les
règles persistées de l'opérateur sont évaluées en premier, à chaque invocation,
contre le premier argument de cet appel-ci : règles de projet, puis règles
portant sur l'agent du chat, puis règles globales, le préfixe d'argument le plus
long gagnant. Un refus qui correspond rejette l'appel sur-le-champ, y compris un
appel que le tour avait déjà autorisé par son nom. Sinon l'appel s'exécute si le
nom de l'outil est dans l'ensemble autorisé du tour ou si une règle l'autorise,
et une autorisation venue d'une règle n'est pas mémorisée : l'appel suivant est
réévalué avec son propre argument. Ce qui reste déclenche une demande
d'approbation et attend une personne ; un refus, ou cinq minutes sans réponse,
n'exécute rien.

Une règle persistée porte l'une des trois portées suivantes : projet, agent ou
globale. La portée session est refusée à l'écriture et ne vit qu'en mémoire, et
le chemin du chat ne transmet aucune règle de session en mémoire.

Les exécuteurs de code ne sont jamais autorisés en bloc. Une règle qui
accorderait tous les outils n'accorde pas l'exécution shell ni l'exécution
Python ; celles-ci exigent leur propre autorisation explicite. Une autorisation
en bloc est en général une décision de confort à propos de la lecture de
fichiers, et elle ne doit pas devenir en silence le droit d'exécuter du code
arbitraire.

<!-- claim:executor-guard-blocks-command-chaining -->
Une règle ne dispense une commande shell d'approbation que lorsque cette
commande est une commande simple et unique, sans chaînage, tube, redirection,
substitution ni exécution en arrière-plan. Cette garde décide d'une dispense
d'approbation ; ce n'est pas un filtre placé devant l'exécution, et un exécuteur
de code sans règle correspondante demande une personne à chaque invocation.

<!-- claim:permission-decision-is-not-recorded -->
La décision de permission elle-même n'est écrite nulle part. Rien, dans un
binaire livré, n'écrit dans la table `permission_audit` : elle est lue par
`apollia-os permissions audit` et par la vue d'audit du bureau, et par rien
d'autre. Ce qui est enregistré, c'est l'invocation : `tool_invocations` retient
ce qui a tourné, pas qui l'a autorisé.

## Humain dans la boucle {#human-in-the-loop}

Tout outil peut exiger une approbation humaine avant de s'exécuter. Quand c'est
le cas, le runtime suspend l'exécution, émet un événement portant ce qui est
demandé, et reprend sur la réponse. La suspension est un état de premier rang,
pas une attente bloquante : le processus est libre, et une exécution peut rester
en attente au travers d'un redémarrage.

Un workflow pose une question à l'utilisateur depuis une étape, par
`ctx.user.ask` : une question, les choix qu'elle propose quand elle en a, et une
réponse libre que l'utilisateur peut toujours donner à la place. La question est
listée avec les autres décisions qui attendent l'utilisateur, avec l'exécution,
le workflow et l'étape dont elle vient, et la réponse réveille l'étape là où
elle s'est arrêtée. Cette attente n'est pas un état suspendu : un workflow
s'exécute en mémoire et ne reprend pas, donc une exécution qui attend une
réponse quand le runtime s'arrête est mise en échec au démarrage suivant, avec
cette raison, comme toute exécution interrompue. Une question refusée arrive à
l'étape sous la forme de `QuestionDeclined`, que l'auteur peut intercepter.

## Mémoire {#memory-model}

La mémoire a trois couches : les événements épisodiques avec un score
d'importance, les faits sémantiques avec un score de confiance, et les
procédures avec leurs déclencheurs. Chaque agent a son propre magasin, isolé par
espace de noms, interrogeable en texte intégral via SQLite FTS5 avec classement
BM25.

La mémoire est lue à l'initiative de l'agent. Le runtime n'injecte jamais de
contenu mémoire dans le prompt d'un agent. Un agent qui veut du contexte le
demande, ce qui garde le prompt sous le contrôle de l'auteur de l'agent.

Trois exceptions existent, et aucune n'est atteignable depuis un chemin
d'exécution d'agent. Deux vivent à l'intérieur de l'assistant conversationnel
intégré : un résumé du profil de la conversation, et les résumés de sessions
passées au premier message d'une conversation libre. La troisième vit en dehors
de l'assistant, dans la commande de réécriture du bureau, qui porte le rôle, la
fonction et les outils de ce profil dans son propre prompt ponctuel et rend du
texte au composeur plutôt qu'à une exécution.

Le résumé du profil est ajouté à chaque tour de l'assistant, à tous les paliers
d'autonomie, tant que l'interrupteur de profil de la conversation est allumé.
L'interrupteur est allumé par défaut, enregistré avec la conversation, et la
jauge de la conversation dit si le prompt porte le profil. Ce résumé était
auparavant conditionné par le palier d'autonomie le plus long, que le chat libre
n'atteint jamais : un profil rempli par l'utilisateur n'arrivait donc jamais à
l'assistant. Un réglage sans effet est pire qu'une injection que l'utilisateur
voit et peut couper : la condition est passée du palier à la conversation. La
règle des agents est inchangée : un agent installé ne lit le profil que
lorsqu'il le demande.

## Inférence locale {#local-inference}

Les modèles locaux tournent via un `llama-server` embarqué, le serveur amont de
llama.cpp, que le démon lance et supervise à travers son API HTTP compatible
OpenAI, avec appel d'outils natif et traitement continu par lots.

Un binding intégré à l'arbre a été maintenu par le passé, puis abandonné. Le
garder revenait à suivre un amont qui bouge vite à travers une couche
d'interface étrangère, et chaque capacité qui arrivait en amont arrivait ici en
retard, ou pas du tout. Parler l'API HTTP amont coûte un processus local et
achète la cadence de publication de l'amont.

Une compilation empaquetée installe le binaire du moteur. Une compilation depuis
les sources attend `llama-server` sur le `PATH`. Quand un backend local est
configuré et qu'aucun moteur n'est joignable, les appels échouent avec une raison
d'indisponibilité explicite, sans repli silencieux vers un fournisseur cloud.

Sur macOS Apple Silicon, les modèles au format MLX tournent via un second
moteur embarqué, vllm-mlx (Apache 2.0), un serveur compatible OpenAI bâti sur
MLX, avec traitement continu par lots, cache de préfixe et sortie structurée
native. Une version est épinglée, et le moteur est installé par empreinte dans
un environnement à part, dans l'arbre Python embarqué, séparé des paquets
qu'importent les agents : le moteur démarre avec l'interpréteur embarqué dans
un mode qui tient les paquets des agents hors de son chemin. Seules les
dépendances qu'importe le service d'un modèle sont installées, sans résoudre
le reste de la fermeture que le moteur déclare. Deux exclusions en découlent.
mlx-embeddings est sous GPL-3.0 et ne sert que le point d'entrée de
plongements du moteur : il n'est jamais livré et ce point d'entrée n'est pas
servi ; la construction comme la vérification de publication refusent un
paquet qui le porte. PyTorch, OpenCV, Gradio, SciPy et la pile audio ne sont
pas importés pour servir un modèle : ils sont laissés de côté. Le démon
supervise le moteur comme il supervise `llama-server`, derrière une interface
de moteur qui garde le moteur remplaçable, et il n'écoute que sur l'interface
de boucle locale. Le prix est le poids : le moteur et ses dépendances ajoutent
environ 150 Mo à l'image disque macOS, et ils demandent macOS 14 ou plus
récent, plancher des roues MLX à la version épinglée. Les paquets Windows et
Linux ne le portent pas.

Les moteurs ont leur propre canal de mise à jour et ne suivent pas la version
d'Apollia. Le moteur embarqué sert chaque machine dès le premier lancement ; une
version accélérée, CUDA sur une machine dotée d'une carte NVIDIA, se télécharge
à la demande, ou s'importe depuis un disque sans réseau, et le lanceur prend
alors la version installée la plus récente de cette variante plutôt que le
moteur embarqué. Le moteur qui sert chaque format de modèle est un réglage,
automatique par défaut ; une variante choisie qui n'est plus installée retombe
sur le choix automatique. Le canal suivi est vérifié au démarrage du runtime et
au changement de canal, et une mise à jour qu'il annonce pour un moteur choisi
est installée sans qu'on la demande, sauf si le réglage qui la gouverne est
éteint, auquel cas elle est seulement annoncée. Tout téléchargement ou import
de moteur est précédé de la
vérification d'un manifeste d'empreintes signé au format minisign avec une clé
de distribution dédiée. Chaque canal, stable ou préversion, publie un tel
manifeste, qui liste chaque archive de moteur avec son SHA-256. La moitié
publique de cette clé est compilée dans le binaire et c'est la seule clé
acceptée. Aucune archive n'est demandée avant que la signature tienne, une
archive dont l'empreinte diffère du manifeste est supprimée, et un manifeste
plus ancien que le dernier accepté sur son canal est refusé. Un miroir peut
remplacer l'adresse d'où viennent manifestes et archives ; il ne peut pas
remplacer la clé. Les archives de moteur se lisent au format zip seul, celui déjà
lu pour les paquets d'agents, et aucun autre format d'archive n'est analysé. Les
versions amont publiées en `.tar.gz`, dont celles pour macOS et Linux, sont
reconditionnées en zip par la chaîne de publication qui produit le manifeste, et
le manifeste porte l'empreinte de ce zip. L'environnement du moteur MLX figure
dans le même manifeste et s'installe par la même vérification.

Il n'y a aucun fournisseur cloud dédié. Un modèle distant se joint par une seule
connexion générique à un serveur compatible OpenAI, à l'adresse que
l'organisation désigne : un vLLM ou un LM Studio auto-hébergé, ou une
passerelle. Toute adresse compatible OpenAI est acceptée, mais une API cloud
n'est ni testée ni promue. Quand un modèle cloud est nécessaire, LiteLLM est la
passerelle recommandée : elle tourne sur une infrastructure que l'organisation
maîtrise et parle le protocole OpenAI à Apollia. À la migration de `system.db`,
les backends qui visaient les API d'OpenAI, d'Anthropic ou de Mistral sont
supprimés, et ceux qui visent un serveur auto-hébergé sont gardés tels quels.

### Vérifier un modèle téléchargé {#model-download-integrity}

Chaque fichier d'un modèle téléchargé depuis HuggingFace est vérifié contre
l'identifiant que le dépôt publie pour lui avant que le modèle soit inscrit. Un
fichier stocké par LFS (poids, projecteur, gros tokenizer) est vérifié contre le
SHA-256 de son contenu. Un petit fichier commité hors LFS (`config.json`,
`tokenizer_config.json` et leurs pareils dans un dépôt MLX) n'a pas de SHA-256
publié : HuggingFace ne publie que son identifiant de blob git, le SHA-1 de
`blob <taille>\0` suivi du contenu. Un tel fichier est vérifié contre cet
identifiant, et c'est pourquoi `sha1` est une dépendance directe de
`apollia-llm`. Elle vient de la même famille RustCrypto que `sha2`, n'ajoute
aucune crate transitive, et sert à comparer un fichier à ce que son dépôt
déclare, non à résister à un faussaire : la connexion TLS au dépôt porte cette
garantie, et un fichier qui ne correspond à aucun des deux identifiants est
supprimé.

### Le registre des modèles {#model-registry}

Le catalogue des modèles lit un index en ligne de fiches de modèles. Une fiche
est un texte court rédigé pour ce projet, en français et en anglais : ce que le
modèle sait bien faire, les capacités que déclare son gabarit de conversation,
la mémoire qu'il demande, les valeurs d'échantillonnage que recommande son
éditeur, les réglages qu'expose son gabarit, sa licence et la page de son dépôt
d'origine. Chaque valeur qu'une fiche reprend d'un dépôt de modèle cite
l'adresse où elle a été lue ; aucun texte de README n'est recopié dans une
fiche, et un README n'est lu depuis le dépôt qu'à la demande de l'utilisateur,
en texte, sans jamais être écrit sur le disque. Les fiches sont écrites dans ce
dépôt, pour qu'une fiche soit ajoutée ou corrigée sans nouvelle version de
l'application.

Le registre est lu sous le principe du local d'abord. L'index est un fichier
statique, demandé à l'ouverture du catalogue et à aucun autre moment, par une
requête sans corps, sans paramètre, sans cookie et sans identifiant : le
registre n'apprend rien de qui le lit, et aucune donnée de l'utilisateur n'est
envoyée. Il est signé au format minisign par la clé de distribution dédiée
décrite dans [Signatures](#signing-schemes), dont la moitié publique est
compilée dans le binaire et est la seule clé acceptée ; un index dont la
signature ne tient pas est refusé, jamais utilisé. L'index est servi depuis un
stockage objet opéré en France, derrière un domaine du projet, avec des
journaux d'accès conservés peu de temps. Quand cette adresse ne répond pas, le
même index signé est lu depuis le miroir des versions publiées sur GitHub, avec
la même vérification. Le dernier index accepté est gardé sur le disque, et la
fiche d'un modèle est copiée à côté de lui à son téléchargement : un modèle
installé lit sa fiche hors ligne.

Une organisation remplace l'adresse par un miroir interne, ou coupe le
registre, par la section `[models]` d'`apollia.toml`. Un miroir
change l'origine de l'index, jamais qui peut le signer. Sans réseau et sans
copie gardée, le catalogue est vide et le dit, rien n'est recommandé, et un
modèle peut toujours être importé depuis un disque.

Le coût est une source réseau dont dépend le catalogue : aucun index n'est
livré avec l'application, donc un premier lancement sans réseau n'a pas de
recommandation.

## Reconnaissance vocale {#speech-to-text}

La transcription tourne hors processus, dans un sidecar bâti sur whisper, si
bien qu'un plantage de modèle ne peut pas emporter le démon. Elle est
optionnelle : une compilation sans elle perd la dictée, et rien d'autre.

## Model Context Protocol {#mcp}

Apollia est un client MCP sur les transports stdio et HTTP. C'est aussi un
serveur MCP, pour les agents et workflows que l'utilisateur choisit et rien
d'autre : `apollia-os mcp server` est un pont stdio vers le démon en cours, qui
liste les agents et workflows marqués exposés et en exécute un jusqu'à sa fin
quand un client l'appelle. Un agent prend un message, sous le nom d'outil
`agent__<id>` ; un workflow prend les entrées que son schéma déclare, sous
`workflow__<paquet>_<workflow>`. Un nom s'en tient à `[A-Za-z0-9_-]` et à 64
caractères, comme l'exigent les API des modèles, et deux éléments dont les noms
se confondraient sont tous deux écartés. Le pont est désactivé par défaut : le
démon le refuse tant que `[mcp] server_enabled` n'est pas activé, réglage qu'il
relit à chaque ouverture d'un pont. Les outils natifs (shell, fichiers,
installation d'agents) ne sont plus servis à un client MCP.

Toute réponse MCP est traitée comme une entrée non fiable. Les réponses sont
plafonnées par serveur, les noms et descriptions d'outils sont validés dès
l'ingestion contre un jeu de caractères et une longueur maximale, et le nombre
d'outils qu'un seul serveur peut apporter est borné. Un serveur qui se comporte
mal se dégrade lui-même, pas le runtime.

Les serveurs qui exigent OAuth passent par un flux dédié, avec son propre
plafond de réponse, plus petit.

Node.js n'est pas embarqué dans Apollia, ce qui tient le principe 2 (zéro
dépendance externe) : rien n'est à installer sur le poste au préalable. Une
version LTS épinglée de Node.js est publiée dans le manifeste d'empreintes
signé du canal des moteurs, signé au format minisign avec la clé de
distribution dédiée d'Apollia (voir [Inférence locale](#local-inference)), et
elle se télécharge au premier serveur lancé par `node` ou `npx` qui en a
besoin : la signature du manifeste est vérifiée avant toute requête d'archive,
puis l'empreinte de l'archive, puis l'archive est extraite dans le magasin des
moteurs. Un tel serveur tourne alors sur ce Node.js par son chemin absolu,
jamais par le `PATH` ; tant qu'aucun n'est installé, son lancement échoue sur
une erreur typée « Node.js requis ». Un poste sans réseau importe Node.js depuis
un disque, par la même vérification. Les paquets npm des serveurs s'installent
depuis un verrou qui porte une empreinte d'intégrité par paquet.

## Connecteurs {#connectors}

Les connecteurs Google et Microsoft s'authentifient en OAuth2 avec PKCE. Aucun
agrégateur ne s'intercale : un relais tiers payant entre un opérateur et sa
propre boîte mail contredit la raison d'être du produit.

Le binaire embarque un client OAuth Microsoft et aucun client Google. Les portées
restreintes de Google exigent un processus de vérification que le projet n'a pas
mené, donc une connexion Google demande à l'opérateur de fournir ses propres
identifiants client. L'asymétrie est assumée, et elle est énoncée là où
l'opérateur la rencontre.

## Documents et sources {#documents-and-sources}

Les classeurs et les fichiers CSV se lisent en Rust, sans l'interpréteur
Python embarqué. `calamine` (MIT) lit `.xlsx`, `.xlsm`, `.xlsb`, `.xls` et
`.ods` ; `csv` (MIT ou Unlicense) lit les fichiers CSV, avec le séparateur
détecté parmi la virgule, le point-virgule et la tabulation. Les deux sont en
pur Rust, en lecture seule, ne lient aucune bibliothèque système et n'ouvrent
aucune connexion. Chaque cellule se lit en texte avec sa feuille, sa ligne et sa
colonne : un nombre sans partie décimale nulle, une date en ISO 8601, une
formule par la valeur que le fichier a calculée en dernier (aucune formule
n'est évaluée), une erreur par son code. Un classeur protégé par un mot de
passe, ou endommagé, donne une erreur typée, jamais un plantage. Un fichier CSV
qui n'est pas en UTF-8 se lit avec ses octets invalides remplacés, et le
résultat le signale : aucun encodage n'est deviné.

Le coût est nommé : `calamine` dépend de `zip` 8 quand l'espace de travail porte
`zip` 4 pour l'archive de paquet et l'export d'un lot ; deux versions majeures
de `zip` entrent donc dans chaque compilation, avec cinq petites crates sous
`calamine` (`atoi_simd`, `codepage`, `debug_unsafe`, `fast-float2`,
`typed-path`) et `csv-core` sous `csv`. `calamine` est épinglé en 0.36.1 : une
nouvelle version, et la version de `zip` qu'elle apporte, est une décision et
non une mise à jour.

Le texte d'un document PDF s'extrait en Rust par `pdf_oxide` (MIT ou
Apache-2.0), page par page, chaque page avec son numéro. La bibliothèque est en
pur Rust, en lecture seule, ne lie aucune bibliothèque système et n'ouvre aucune
connexion ; ses fonctions par défaut sont désactivées, si bien que ni son moteur
de profils colorimétriques ni ses modules de rendu, d'OCR ou de signature
n'entrent dans la compilation, et seule `legacy-crypto` est gardée, pour qu'un
document verrouillé par un seul mot de passe propriétaire s'ouvre encore. Une
page qui ne dessine ni lettre ni chiffre est marquée sans texte : c'est la page
que la reconnaissance optique de caractères devra lire. Un document qui demande
un mot de passe pour s'ouvrir, ou endommagé, donne une erreur typée, et un
défaut de l'analyseur sur un fichier malformé est rattrapé et signalé comme un
fichier endommagé, jamais comme un plantage. La bibliothèque a été mesurée avant
d'être adoptée, sur vingt documents français réels publiés sous Licence Ouverte
(pièces de marché et conditions d'utilisation, budgets et grilles tarifaires,
notes méthodologiques), avec cinq phrases témoins accentuées par document,
prises à un extracteur indépendant : dix-neuf documents sur vingt rendent leurs
cinq phrases, tous les accents intacts ; le vingtième perd une phrase dont une
cellule de tableau répartit les mots sur deux lignes. Le corpus est versionné
avec les tests qui le lisent, et PDFium, le repli natif envisagé, n'est pas
livré.

Le coût est nommé : `pdf_oxide` apporte cinquante-trois versions de crates
absentes du fichier de verrouillage, dont une pile de polices (`skrifa`, `read-fonts`,
`write-fonts`, `ttf-parser`, `subsetter`), un moteur de mise en page (`taffy`)
et des secondes versions majeures de `quick-xml`, `brotli`, `sha2` et `aes`.
`pdf_oxide` est épinglé en 0.3.78, et `office_oxide`, dont il dépend, est tenu
en 0.1.11 dans le fichier de verrouillage : sa version 0.1.12 ne compile plus
sous `pdf_oxide` 0.3.78 ; mettre l'une ou l'autre à jour est donc une décision et
non un rafraîchissement.

Une crate de cette pile de polices porte un avis : `ttf-parser` 0.25.1, dont
seul `pdf_oxide` se sert pour lire les polices qu'un document embarque, est
signalée non maintenue (RUSTSEC-2026-0192). Aucune faille n'y est connue et
aucune version plus récente n'existe vers laquelle monter : l'avis est donc
exempté dans `deny.toml`, daté du 2026-10-04, avec cette raison à côté, et il
est revu à la prochaine version mineure. L'exemption tombe quand `pdf_oxide`
abandonne `ttf-parser` ou qu'une version maintenue en paraît.

Les documents Word et les présentations PowerPoint sont lus en Rust, par un
code propre posé sur deux crates : `zip`, celle qui ouvre l'archive de paquet
(voir [Archive de paquet](#package-archive)), ouvre le fichier, et `quick-xml`
(MIT), un lecteur XML en flux, en lit les parties. Un document Word rend les
paragraphes et les lignes de tableau de son corps, une ligne par segment, dans
l'ordre du document et chacun avec son rang ; une présentation rend le texte et
les notes de chaque diapositive, dans l'ordre numérique, chacune avec son
numéro. Les en-têtes, pieds de page, commentaires et zones de texte flottantes
ne sont pas lus. Un fichier aux anciens formats binaires, `.doc` ou `.ppt`, est
une erreur typée qui nomme les formats lus. L'archive vient d'ailleurs, sa
lecture est donc bornée : au-delà de 10 000 entrées, ou de 64 Mio décompressés
sur l'ensemble des parties lues, le fichier est refusé, les octets étant comptés
à mesure qu'ils sont décompressés et non tels que l'archive les déclare. Une
crate dédiée n'a pas été retenue : `docx-rs`, celle pesée pour Word, est
tournée vers l'écriture et ne lit pas PowerPoint, et `office_oxide`, qu'apporte
le lecteur PDF, est tenue à une ancienne version (voir plus haut) et n'est pas
appelée directement.

Le coût est nommé : aucune crate n'entre dans le fichier de verrouillage.
`quick-xml` 0.41 était déjà compilée pour le lecteur de classeurs et le paquet
du bureau, et devient une dépendance directe, fonctionnalités par défaut
coupées ; `zip` était déjà une dépendance directe. Ce que ces crates ne couvrent
pas est à notre charge : la structure des deux formats, lue ici jusqu'à leur
texte seulement.

Les pages scannées et les images sont lues par reconnaissance optique de
caractères, avec un moteur compilé dans le binaire. Une page de document PDF
qui ne trace aucun texte et qui est une image unique est prise telle que le
document l'embarque, sans rendu de la page, puis reconnue ; les fichiers
`.png`, `.jpg` et `.tiff` d'une source sont lus de la même façon. Le moteur est
Tesseract, sur Leptonica, tous deux compilés depuis leurs sources par
`tesseract-rs` (MIT ; Tesseract est sous Apache-2.0, Leptonica sous
BSD-2-Clause) et liés statiquement : le poste qui exécute Apollia n'installe
rien, et le binaire ne dépend d'aucune bibliothèque Tesseract ou Leptonica.
Leptonica est compilée sans ses codecs d'image, les images sont donc décodées
en Rust par `image` (MIT ou Apache-2.0), avec ses seuls décodeurs PNG, JPEG et
TIFF. Le texte reconnu vit dans l'index de la source et nulle part ailleurs :
aucun fichier n'est écrit à côté des documents.

Le moteur lit avec un fichier de données par langue. Le français et l'anglais,
tirés du jeu `tessdata_fast` (Apache-2.0), sont des entrées du registre des
modèles, chacune épinglée par son SHA-256. Ils sont obtenus la première fois
qu'une source contient une page scannée, une fois par exécution, par le
téléchargeur qui contrôle chaque modèle contre son empreinte ; un poste sans
réseau prend les mêmes fichiers sur un disque par l'import de modèle, qui
n'accepte un fichier de données de langue que si son empreinte est épinglée.
Un fichier dont l'empreinte n'est pas épinglée n'est jamais remis au moteur.
Tant que les données manquent, chaque page en attente est listée dans l'état de
sa source comme en attente de reconnaissance, et elle est lue dès qu'elles
arrivent ; une page sans texte qui n'est pas une image unique est listée comme
non lue. Aucun moteur de rendu n'est livré : `hayro` et PDFium, ceux qui ont
été pesés, restent dehors tant que les documents n'en montrent pas le besoin.

Le coût est nommé : la chaîne C++ entre dans la compilation. Compiler le moteur
demande cmake, un compilateur C++ et le réseau, prend une à deux minutes (83
secondes sur macOS Apple Silicon, 133 secondes sur Linux x64 sous émulation),
et ajoute 2,8 Mo au binaire sur macOS et 4,4 Mo sur Linux. Le script de
compilation de `tesseract-rs` télécharge les sources de Tesseract 5.5.2 et de
Leptonica 1.87.0 depuis GitHub sans en contrôler l'empreinte, les garde avec
les bibliothèques compilées dans un dossier du répertoire personnel plutôt que
sous `target/`, et y pose deux fichiers de langue que le produit ne lit pas. La
reconnaissance est donc derrière la fonctionnalité `ocr`, désactivée par
défaut : une compilation de tous les jours n'en compile rien, et une
compilation qui livre la reconnaissance l'active. `tesseract-rs` est épinglée à 0.4.0, puisqu'une version change les
sources qu'elle va chercher ; trois crates entrent dans le fichier de
verrouillage, `tesseract-rs` elle-même et `aws-lc-rs` avec `aws-lc-sys`, que
le client HTTP de son script de compilation apporte et qui ne s'exécutent qu'à
la compilation. `image` était déjà compilée pour le lecteur PDF et n'en ajoute
aucune. La compilation statique est mesurée sur macOS et sur Linux ; elle ne
l'est pas sur Windows, où la fonctionnalité reste désactivée tant qu'elle ne
l'est pas.

Les résultats d'un lot sont écrits dans le tableau qu'il a lu, par défaut dans
une copie posée à côté de l'original, `<nom> (résultats AAAA-MM-JJ).xlsx`,
nommée dans la langue de l'interface, si bien que l'original est lu et jamais
ouvert en écriture. Sur demande explicite, et pour `.xlsx` et `.xlsm` seulement,
ils vont dans l'original lui-même, après la copie de sauvegarde datée qu'exige
le contrat des lots ; si la sauvegarde échoue, rien n'est écrit. Dans les deux
cas, le classeur est édité en place, par du code à nous au-dessus de `zip` et
de `quick-xml` : les cellules de la colonne ajoutée, après la dernière colonne
de la feuille, sont insérées dans le XML de cette feuille en chaînes en ligne,
la chaîne de calcul est retirée pour que le tableur recalcule à l'ouverture, et
toutes les autres parties de l'archive sont recopiées telles qu'elles sont
stockées, octet pour octet. La mise en forme, les graphiques, les tableaux
croisés, les commentaires et les macros sont donc conservés, ce que ne garantit
pas une bibliothèque qui charge un classeur puis l'enregistre de nouveau :
`umya-spreadsheet`, celle qui a été pesée, documente des graphiques corrompus,
des commentaires perdus et des tableaux croisés non lus. L'édition est testée
sur un corpus versionné de classeurs réels (mise en forme conditionnelle,
graphique, tableaux croisés, commentaires, fichier enregistré par LibreOffice,
classeur à macros). Un fichier CSV reçoit une copie avec sa colonne ajoutée,
jamais une édition de l'original ; un classeur `.xls`, `.xlsb` ou `.ods` reçoit
une copie `.xlsx` neuve écrite par `rust_xlsxwriter` (voir
[Export d'un lot](#batch-export)), ses valeurs seulement.

Le coût est nommé : aucune crate n'entre dans le fichier de verrouillage,
`rust_xlsxwriter` devenant une dépendance directe des lecteurs. Ce qui est à
notre charge est l'édition de la feuille, une petite partie d'un format dont
les autres parties ne sont jamais réécrites.

Une boîte mail se connecte en IMAP avec un mot de passe d'application : tout
fournisseur est joignable, Gmail compris, sans client OAuth et sans relais. Le
client est `async-imap` (MIT ou Apache-2.0), sur le runtime Tokio, au-dessus de
la pile TLS `tokio-rustls` que le runtime porte déjà ; il n'apporte pas
OpenSSL. Les messages sont décodés par `mail-parser` (Apache-2.0 ou MIT), qui
ne dépend de rien hors de Rust. Le certificat du serveur est vérifié contre les
certificats racine du système, lus par `rustls-native-certs` (Apache-2.0, ISC
ou MIT), déjà présent dans la compilation par le client HTTP et désormais
dépendance directe.

L'accès est en lecture seule, et le client l'impose au lieu de le promettre :
les dossiers s'ouvrent par `EXAMINE`, jamais `SELECT`, les messages se lisent
par `BODY.PEEK` et restent non lus, et aucune commande d'écriture (`STORE`,
`EXPUNGE`, `COPY`, `MOVE`, `APPEND`) ne part. Envoyer, déplacer ou supprimer un
message est hors d'atteinte. Le mot de passe vit dans le magasin de secrets
configuré, jamais dans la liste des connexions, un journal de traces ou le
journal d'audit ; comme `async-imap` trace les commandes qu'il envoie par la
façade `log`, connexion comprise, chaque binaire éteint sa cible de traces.

Le coût est nommé : deux crates et leurs petits arbres (`imap-proto`, `hashify`,
`stop-token`, `async-channel`) entrent dans chaque compilation, pour un
protocole dont les serveurs varient dans leur rigueur. Un serveur qui comprend
mal une commande donne une erreur de protocole typée, pas un plantage.

## Export d'un lot {#batch-export}

Le résultat d'un lot s'exporte en classeur Excel, en CSV ou en JSON, une ligne
par élément. CSV et JSON s'écrivent sans dépendance. Le classeur est écrit par
`rust_xlsxwriter` (MIT ou Apache-2.0), un écrivain seul, sans accès réseau,
épinglé à 0.92.0 : c'est la dernière version qui dépend de la version majeure
de `zip` que l'espace de travail porte déjà, et une version plus récente
mettrait deux versions de `zip` dans chaque build. Il ajoute trois crates au
fichier de verrouillage, lui-même, `zopfli` et `zlib-rs` (voir
[Archive de paquet](#package-archive)).

`rust_xlsxwriter` ne sait pas modifier un classeur existant. Écrire les
résultats dans le fichier d'où un lot a été lu se décide avec le lecteur de
tableurs (voir [Documents et sources](#documents-and-sources)).

L'application de bureau écrit un classeur de plus avec lui : le jeu de
démonstration sur lequel un agent d'exemple s'essaie, construit depuis un
tableau que l'application porte quand l'utilisateur en demande une copie.
Aucune crate n'entre pour cela dans le fichier de verrouillage,
`rust_xlsxwriter` devenant une dépendance directe du bureau.

## Messagerie entre agents {#agent-messaging}

Un agent en joint un autre de deux façons. Une étape de workflow lance un agent
installé sur un message et attend sa réponse : l'agent tourne avec ses propres
accès, imbriqué dans l'exécution qui l'a appelé. Ou bien un message est déposé
dans la boîte aux lettres durable d'un agent, adossée à SQLite, et le
destinataire le retire à son initiative. La livraison par boîte aux lettres se
fait par bail : un consommateur prend un bail, et l'acquittement est verrouillé
sur l'exécution qui le détient, si bien qu'un consommateur périmé dont le bail a
été réattribué ne peut ni supprimer ni remettre en file le message que le nouveau
détenteur est en train de traiter.

L'API liste ce qui peut être appelé sous un identifiant complet, `agent:<id>`
pour un agent et `workflow:<package>/<name>` pour un workflow, et lance l'un
comme l'autre par cet identifiant. Le dispatch le propage en entier. Une clé de
dispatch plus courte a été utilisée une fois, et elle a produit de l'ambiguïté
dès que deux travailleurs ont exposé des skills aux noms voisins.

## Audit et preuve {#audit-and-evidence}

Il y a deux registres, et la garantie n'est pas la même sur chacun.

La piste d'invocations d'outils porte ce qui a tourné, une empreinte de ses
entrées, s'il a réussi, et combien de temps il a pris. Un appel qui a échoué est
persisté comme ayant échoué. Elle est écrite en pose-et-oublie, si bien que
l'enregistrement ne bloque jamais l'exécution, et un enregistrement est jeté
quand le canal est saturé : la piste est un enregistrement au mieux, pas un
enregistrement complet. Ses lignes sont plates : l'empreinte des entrées que
chaque ligne porte n'est chaînée à aucune autre, et aucune ligne n'est signée.

Le journal chaîné par hachage est le registre qui porte la preuve. Ses entrées
sont chaînées et signées, et la chaîne est ancrée globalement pour que la
troncature soit détectable : retirer la fin du journal casse une vérification
qu'un lecteur peut lancer lui-même.

Les deux registres couvrent le chemin agent. Un appel d'outil effectué dans une
session de chat n'atteint ni l'un ni l'autre.

Rejouer une exécution puis la comparer à son enregistrement n'est délibérément
pas construit. Une réexécution prouve qu'une seconde exécution s'est comportée
d'une certaine façon, pas que la première l'a fait, et la responsabilité repose
déjà sur la chaîne signée. C'est nommé ici pour que cette absence se lise comme
un choix et non comme un manque.

## Secrets et authentification de l'API {#secrets-and-api-auth}

Les secrets vivent dans le trousseau du système, ou dans un fichier chiffré avec
age là où il n'existe pas de trousseau. Ils ne sont jamais écrits dans un fichier
de configuration.

<!-- claim:daemon-binds-tcp-by-default -->
L'API HTTP écoute sur une socket Unix et sur un port TCP ; le daemon lie les deux
à chaque démarrage, et le numéro de port est la seule chose que `--port` décide.
La socket Unix relève de la confiance locale et s'appuie sur les permissions du
système de fichiers. Le TCP exige un jeton porteur sur tous les chemins
`/api/v1` sauf un, et le lier à une adresse non locale sans jeton est une erreur
de démarrage plutôt qu'un avertissement : un attachement distant non authentifié
est la seule erreur qu'on ne peut pas défaire une fois que le trafic est passé.
TLS est facultatif : avec `[api] tls_cert` et `tls_key` renseignés, l'écouteur
TCP sert en HTTPS, et un certificat qui ne se charge pas fait échouer le
démarrage au lieu de retomber en clair.

L'exception est `GET /api/v1/health`, qui répond sans jeton et ne révèle que le
marqueur de produit et la version du runtime, rien d'autre. Un processus qui
trouve le port pris ne détient aucun jeton de cette installation, et ce qu'il doit
savoir, c'est si le détenteur est un runtime Apollia et dans quelle version.
Toutes les autres routes `/api/v1` gardent le jeton.

### Le serveur compatible et le réseau local {#local-server}

Le runtime a deux écouteurs. L'API native, port 7771 et socket locale, pilote le
runtime ; le serveur compatible, port 7772 par défaut, sert les formats d'OpenAI
et d'Anthropic aux autres applications. L'API native reste sur la machine : le
runtime embarqué du bureau la lie à `127.0.0.1` quoi que dise le fichier, le
daemon à la boucle locale que `[api] bind` nomme par défaut (une autre adresse
exige un jeton, comme ci-dessus), et aucun réglage du serveur compatible ne la
déplace. Le serveur compatible est la seule surface que l'utilisateur ouvre au
réseau local, par `[server] serve_on_lan`, éteint par défaut et allumé
seulement par un geste explicite de l'utilisateur.

Pour les modèles, le réseau et les jetons sont deux interrupteurs indépendants.
Un appareil du réseau local peut appeler les modèles sans jeton quand
`[server] require_auth` est éteint, comme d'autres serveurs d'inférence locaux
le permettent ; ce choix appartient à l'utilisateur. Les agents et les workflows
ne sont pas des modèles : ils atteignent les connexions, les documents et le
courrier de l'utilisateur, si bien qu'un appel venu du réseau local n'est admis
qu'avec un jeton actif du serveur, quoi que dise `require_auth`. Chaque requête
porte son origine, la machine ou le réseau local, lue dans l'adresse du pair à
l'acceptation de sa connexion. La politique de l'organisation est l'endroit où
une organisation interdit le réseau local ou impose les jetons, par-dessus les
interrupteurs de l'utilisateur.

Au regard du principe du local d'abord : rien ne sort de la machine tant que
l'utilisateur n'ouvre pas le réseau, et ce qui répond alors sans jeton est une
inférence sur le texte que l'appelant a lui-même envoyé. Les données de
l'utilisateur, que seuls les agents et les workflows atteignent, ne répondent
jamais au réseau sans un jeton que l'utilisateur a créé et peut révoquer. L'API
native, qui porte toutes les autres capacités, n'est jamais joignable depuis le
réseau par cet interrupteur. Le trafic sur le réseau est du HTTP en clair : le
serveur compatible ne termine pas TLS.

## Signatures {#signing-schemes}

Deux schémas de signature coexistent, un par sorte de signataire, et aucun n'a
besoin du réseau pour vérifier.

Les paquets d'agents et les politiques d'organisation sont signés par leur
auteur, avec une clé que l'application génère. La signature est une signature
Ed25519 détachée sur une liste canonique d'entrées `(chemin, SHA-256)` triée par
les octets du chemin : l'ordre dans lequel les fichiers ont été rassemblés ne la
change jamais, et ajouter, retirer ou modifier un seul fichier la casse. Elle
voyage dans un fichier `SIGNATURE` versionné qui porte la version du format,
l'empreinte de la clé (le SHA-256 de la clé publique), le nom d'éditeur déclaré
par l'auteur, et la signature. La clé privée est rangée par le magasin de
secrets et n'est jamais écrite en clair sur le disque. Le nom d'éditeur est une
déclaration et n'est pas couvert par la signature : ce qui identifie un auteur,
c'est l'empreinte de sa clé.

Les métadonnées de distribution publiées par la chaîne de publication d'Apollia
(le manifeste d'empreintes des moteurs d'inférence et de Node.js, l'index du
registre des modèles) sont signées au format minisign, avec une clé de
distribution dédiée. C'est le même format que la clé du programme de mise à jour
de l'application de bureau, et une clé distincte, rangée dans ses propres
secrets de la chaîne (`APOLLIA_DIST_SIGNING_KEY`,
`APOLLIA_DIST_SIGNING_KEY_PASSWORD`). La signature se fait dans la chaîne avec
l'outil de signature de Tauri ; l'application ne fait que vérifier, et le démon
vérifie sans Tauri, ce qui fait du vérificateur une dépendance directe plutôt
qu'une dépendance atteinte par le greffon de mise à jour.

minisign n'a pas été retenu pour les paquets et les politiques : il signe un
fichier, pas une liste d'entrées, et la génération et le rangement de clé dont
l'application a besoin seraient à écrire de toute façon. Sigstore et X.509 ont
été écartés pour les deux usages : l'un exige un journal de transparence et un
fournisseur d'identité joints par le réseau, l'autre une autorité de
certification, et chacun fait d'un tiers une condition de la vérification,
contre le principe du local d'abord.

Le coût est une surface de dépendances nouvelle et l'absence de révocation.
`ed25519-dalek` 2 est compilé sans ses fonctionnalités par défaut (pas de tables
précalculées) et apporte `ed25519` et `signature` ; son arithmétique de courbe,
`curve25519-dalek`, était déjà dans l'arbre. `minisign-verify` n'a aucune
dépendance propre et était déjà dans l'arbre par le programme de mise à jour.
Une clé ne se révoque qu'en changeant l'ensemble des clés auxquelles un lecteur
fait confiance.

## Politique de l'organisation {#organisation-policy}

Une organisation impose ses règles à un poste en déposant une politique signée
dans un dossier système : `/Library/Application Support/Apollia/policy` sous
macOS, `C:\ProgramData\Apollia\policy` sous Windows, `/etc/apollia/policy` sous
Linux. Le dossier porte `policy.toml`, un `SIGNATURE` au format des paquets
ci-dessus sur une liste d'une entrée, et `org.pub`, la clé publique de
l'organisation. Le runtime le lit une fois, au démarrage. Pas de dossier, pas de
politique : l'utilisateur est libre.

La clé est rangée à côté de la politique, si bien que la politique ne vaut que
par les droits de son dossier. Un dossier ou un fichier qu'un autre que
l'administrateur du système peut modifier est refusé : sous Unix le
propriétaire doit être root, et ni les autres ni un groupe autre que celui de
root ne peuvent écrire ; sous Windows le propriétaire doit être le groupe
Administrateurs ou le compte LocalSystem, et aucune entrée de la liste d'accès
ne peut accorder une écriture à quelqu'un d'autre. Un lien symbolique est
refusé partout. Le chemin Windows est écrit en dur plutôt que lu dans
`%ProgramData%`, que l'utilisateur règle.

La date de signature est un champ `signed_at` de `policy.toml`, une date, une
heure et un décalage. La signature couvre le fichier entier : la date est donc
signée, et le format `SIGNATURE` n'a pas changé pour elle. Une date du système
de fichiers a été écartée : n'importe qui peut toucher un fichier.

Lire le propriétaire et la liste d'accès d'un fichier sous Windows passe par
l'API de sécurité Win32, que la bibliothèque standard n'expose pas.
`windows-sys` est une dépendance directe de la crate des permissions sous
Windows seulement, avec les fonctionnalités de sécurité, d'autorisation et de
système de fichiers. C'est la liaison brute que publie Microsoft, elle n'a
d'autre dépendance que sa propre cale d'édition de liens, et la même version
était déjà dans l'arbre par d'autres crates : le fichier de verrouillage n'a
gagné aucun paquet. Le coût est un bloc de code FFI avec sa propre permission
`unsafe`, confiné à un module.

## Le fichier de configuration {#configuration-file}

`apollia.toml` a un seul écrivain, `apollia_core::config::writer`. Les écrans de
réglages de l'application de bureau, la ligne de commande, les routes sous
`/api/v1/config` et les moteurs que le runtime recopie dans le fichier passent
tous par lui, si bien que les quatre mêmes vérifications tiennent quel que soit
l'auteur : le texte se valide contre le schéma strict, aucune ligne ne donne à
une clé que la politique de l'organisation impose une autre valeur, le fichier
sur le disque est encore celui que l'appelant a lu, comparé par son empreinte
SHA-256, et le texte remplace le fichier par un fichier voisin renommé
par-dessus. Une écriture par clé garde les commentaires et l'ordre du fichier.
La ligne de commande appelle l'écrivain directement : elle fonctionne démon
arrêté, et un refus se lit dans la même phrase sur chaque surface.

Tenir l'écrivain dans la crate de fondation place `toml`, `toml_edit` et `sha2`
parmi les dépendances directes d'`apollia-core`. Les trois étaient déjà des
dépendances de l'espace de travail, et le fichier de verrouillage n'a gagné
aucun paquet.

## Ligne de commande {#cli}

Le CLI est en nom-verbe. Les opérations quotidiennes sont des verbes nus ; tout
le reste est un nom portant des sous-commandes. `--json` est global et le
terminal est détecté automatiquement, donc une même commande sert un humain et un
script.

Les codes de sortie sont un contrat : 0 succès, 1 erreur d'usage, 2 erreur
d'exécution, 3 tâche échouée, 4 délai dépassé, 5 annulé. Un appelant s'y branche
sans analyser la sortie.

## Application de bureau {#desktop}

L'application de bureau est bâtie sur Tauri avec une interface Svelte, et
partage le même runtime et le même interpréteur Python embarqué que le CLI.
C'est une seconde façade au-dessus d'un seul runtime, pas une seconde
implémentation.

Tout texte visible par l'utilisateur passe par la couche d'internationalisation,
avec des entrées anglaises et françaises parallèles. Couleurs, espacements et
typographie viennent des jetons de design ; une valeur codée en dur qui double un
jeton est une valeur qui ne suivra pas un changement de thème.

### Le runtime dans la fenêtre ou à côté {#desktop-runtime-modes}

Le runtime tourne à l'un de deux endroits : dans le processus du bureau, ou comme
démon séparé, `apollia-os start` installé comme service de l'utilisateur, auquel
le bureau se rattache. Le démon est ce qui laisse les agents et le travail
planifié continuer fenêtre fermée, et c'est le même runtime, pas un second.

Le port TCP décide. Au lancement, avant l'interpréteur Python, toute base et tout
écouteur, le bureau prend le port du runtime. L'obtenir veut dire qu'aucun
runtime ne sert la machine : le bureau en démarre un dans son propre processus,
sur cet écouteur même, qui n'est jamais relâché entre la décision et le service.
Ne pas l'obtenir veut dire que quelque chose tient le port, et le bureau lui
demande `GET /api/v1/health`, dont le corps porte un marqueur de produit et une
version : un runtime Apollia compatible est rattaché, tout le reste est refusé.
Une sonde suivie d'une liaison a été écartée, parce que l'instant entre les deux
appartient au premier processus qui prend le port.

Refusé : deux runtimes sur une machine, le rattachement à un écouteur qui ne se
dit pas Apollia ou à un runtime dont la version majeure ou mineure diffère, et
un lancement qui s'arrête sans un mot. Un refus ouvre une fenêtre qui nomme ce
qui tient le port et le geste à faire, et ajoute la raison à
`startup-refused.log` dans le dossier de données.

Rattaché, le bureau ne construit aucun runtime à lui. Il joint le démon par
l'API HTTP locale avec le jeton du dossier de données, et reçoit tout le bus
d'événements en un flux, `GET /api/v1/events`. Il interroge la route de santé
toutes les deux secondes plutôt que de supposer le démon présent, si bien
qu'une sonde sans réponse montre aussitôt le runtime déconnecté. Une commande
encore sans route répond une erreur typée qui la nomme, au lieu d'aller chercher
un état que le processus n'a jamais construit ;
`scripts/check_attached_coverage.py` compte les commandes routées face aux
commandes enregistrées, sur un cliquet descendant. Une commande qui relève de
la fenêtre plutôt que du runtime (quitter, redémarrer, chercher une mise à jour,
lister les micros) n'a pas besoin de route : elle figure dans une liste à côté
des commandes routées, avec la raison pour laquelle elle ne lit aucun état du
runtime, et le même script refuse une entrée sans raison. Quitter le bureau
rattaché laisse le démon servir.

La version 0.3.0 livre les deux modes.

### Une seule application, et un fichier de paquet qui l'ouvre {#desktop-single-instance}

L'extension `.apo` est associée à l'application de bureau
(`bundle.fileAssociations` dans `tauri.conf.json`) : un paquet s'ouvre comme un
document. Ouvrir un paquet ne l'installe jamais : quel que soit le geste qui
apporte le fichier, l'application montre l'aperçu du paquet, et le paquet n'est
installé ou mis à jour que depuis cet aperçu. Trois gestes y mènent : ouvrir le
fichier depuis le système, le déposer dans la fenêtre, et le bouton Installer,
qui ouvre un sélecteur de fichier.

Un fichier ouvert depuis le système doit atteindre l'application qui tourne
déjà, pas en démarrer une seconde à côté. macOS garde de lui-même un seul
processus par application et remet le fichier sous forme d'événement, au
lancement comme ensuite. Windows et Linux démarrent un nouveau processus avec le
chemin parmi ses arguments ; là, `tauri-plugin-single-instance` fait que ce
second processus transmet ses arguments au premier et se termine, et le premier
montre sa fenêtre. Le greffon est enregistré avant tout autre, dans les deux
modes du runtime, et sous Windows et Linux seulement : sous macOS il ajouterait
un second mécanisme là où le système en fournit déjà un. Le processus garde les
chemins jusqu'à ce que l'interface les prenne, car un fichier qui démarre
l'application arrive avant que l'interface soit chargée. Un fichier déposé dans
la fenêtre passe par l'événement de glisser-déposer de la vue web, sans greffon.

Le coût est un greffon Tauri de plus à suivre, qui n'apporte aucune bibliothèque
que l'arbre ne contenait déjà, et un chemin de code qui diffère entre macOS et
les deux autres systèmes, donc à vérifier sur chacun. Sous Windows et Linux, un
second lancement ne peut plus ouvrir une seconde fenêtre.

Refusés : un lien profond (`apollia://install`), qui ne répond pas au
double-clic sur un fichier, et le bouton et le dépôt seuls, qui laissent un
paquet incapable de s'ouvrir comme un document.

## Distribution des agents {#agent-distribution}

Un agent voyage dans un paquet, un seul fichier `.apo` (voir
[Archive de paquet](#package-archive)), et s'installe depuis ce fichier et rien
d'autre : ouvert depuis le système, déposé sur la fenêtre, choisi avec le bouton
Installer, ou passé à `apollia-os package install`. Quel que soit le geste, le
paquet est d'abord prévisualisé, avec ce qu'il contient, qui l'a signé et ce que
chacun de ses agents demande, et rien n'est écrit avant que l'utilisateur
accepte. Un agent créé dans l'application en sort de la même façon : il est
exporté en `.apo`, dont le manifeste est écrit depuis l'agent tel qu'il est,
sans secret, sans mémoire et sans historique.

Publier les agents sur un index de paquets a été envisagé puis écarté : cela
ferait de l'index une dépendance de disponibilité pour un produit dont la
première promesse est de fonctionner sans aucune.

Les dépendances Python tierces déclarées par un paquet sont installées dans
l'environnement virtuel propre à ce paquet, après consentement explicite, et
l'utilisateur voit la liste avant que cela n'arrive.

Le coût est une étape pour un auteur : un dossier ou un dépôt ne s'installe pas
tel quel. L'archive est d'abord construite (`apollia-os package build`), et
c'est là que le manifeste est validé et que le paquet est signé.

## Archive de paquet {#package-archive}

Un paquet d'agents voyage en un seul fichier `.apo` : une archive zip qui porte
le manifeste `apollia.toml` à sa racine, les modules Python et les ressources du
paquet à leurs chemins relatifs et, quand l'auteur l'a signé, un fichier
`SIGNATURE` (voir [Signatures](#signing-schemes)) et un fichier `SIGNATURE.key`
qui porte la clé publique qui l'a produit. La signature couvre toutes les autres
entrées. La construction de l'archive valide le manifeste et refuse un dossier
dont les modules définissent des outils ou des workflows que le manifeste ne
décrit pas ; elle n'exécute aucun Python.

Le zip a été préféré au `tar.gz` : tout système d'exploitation l'ouvre sans outil,
et son répertoire central permet de lire le manifeste sans décompresser le
reste, ce dont un aperçu d'installation a besoin. Un `tar.gz` n'a pas d'index,
lire un fichier y oblige à décompresser tout ce qui le précède, et il aurait
coûté les deux mêmes dépendances directes (`tar` et `flate2`) sans rien de cela.
Lire le zip avec un code
maison a été écarté : un analyseur d'archives est une surface d'attaque, et une
surface non auditée est pire qu'une surface maintenue.

Une archive vient d'ailleurs, sa lecture est donc bornée. Une entrée au chemin
absolu, à composant `..`, à barre oblique inverse ou à préfixe de lecteur est
refusée, comme un lien symbolique et un nom présent deux fois. L'archive est
plafonnée à 256 Mio, 4096 entrées, 64 Mio par entrée décompressée et 512 Mio
décompressés au total, le manifeste à 1 Mio. Les plafonds portent sur les octets
réellement décompressés, pas sur les tailles que l'archive déclare : un en-tête
menteur ne les lève pas. Un paquet au-delà d'un plafond est aussi refusé à la
construction, si bien qu'Apollia lit tout ce qu'il construit.

Le coût est deux dépendances directes du moteur, `zip` et `flate2`, toutes deux
déjà dans l'arbre par le programme de mise à jour du bureau ; aucune crate n'est
entrée dans le fichier de verrouillage. `zip` est compilée avec deflate seul :
ni chiffrement, ni bzip2, lzma, zstd ou deflate64. `flate2` est déclarée à côté
pour une seule raison, choisir l'implémentation en Rust pur (`miniz_oxide`) de
ce deflate : `zip` laisse ce choix à l'appelant, et `flate2` refuse de
compiler sans implémentation.

L'export Excel d'un lot (voir [Export d'un lot](#batch-export)) a changé un
point de ce choix. Son écrivain active la fonctionnalité `deflate` de `zip`,
qui apporte deux crates en Rust pur, `zopfli` (Apache-2.0) et `zlib-rs` (Zlib),
et fait de `zlib-rs` l'implémentation deflate de `flate2` dans tout le
programme. Les deux ont été acceptées plutôt qu'écrire le classeur à la main
avec `zip` seul. Les bornes de lecture ci-dessus ne changent pas.

Les paquets qu'Apollia livre dans son propre binaire, le guide de l'application
d'abord, ne sont pas signés. Le bureau assemble chacun d'eux au démarrage à
partir de fichiers embarqués à la compilation et l'installe par le même chemin
que tout paquet. L'installation les reconnaît comme embarqués parce que le
bureau le dit, jamais parce qu'une archive le dit : rien dans un fichier ne peut
revendiquer cette origine, et la même archive remise d'ailleurs est un paquet
non signé ordinaire. Un paquet embarqué s'installe sans avertissement sur sa
signature, et une politique d'organisation qui exige des paquets signés ne le
refuse pas. L'exemption est bornée à cette origine. Sa raison : le binaire qui
porte le paquet est ce que couvre la signature de la distribution, si bien que
signer le paquet une seconde fois ne prouverait rien de plus. Comme tout paquet,
il est inscrit au journal d'audit, une version installée plus récente n'est
jamais rétrogradée, et les surcharges de l'utilisateur comme l'état désactivé de
ses agents survivent à une mise à jour.

Les paquets d'exemple suivent la même règle, à une différence près sur la
suite. Ce sont des paquets déclaratifs embarqués dans le runtime à la
compilation, depuis un dossier de l'arbre des sources. L'application les propose
et n'en installe un que lorsque l'utilisateur le choisit, après l'aperçu que
reçoit tout paquet. Un exemple est désigné au runtime par le nom de son paquet,
jamais par un chemin, si bien que seul ce que porte le binaire est traité comme
un exemple ; il n'est pas signé, et n'est ni signalé ni refusé pour cela, pour
la raison ci-dessus. Une fois installé, il appartient à l'utilisateur : à la
différence du guide, il se désinstalle comme tout paquet.

Un outil de paquet s'exécute avec un contexte nu : ni mémoire, ni profil de
l'utilisateur. Une exception est décidée par l'hôte, jamais par un paquet.
L'onboarding d'Apollia est livré comme un agent déclaratif doté d'un outil qui
enregistre les faits de profil de la première conversation, le droit que
l'agent d'onboarding tenait quand il exécutait du code. Le runtime accorde à cet
outil le profil de l'utilisateur, en écriture, et la mémoire de l'onboarding,
que le bureau lit pour savoir que l'onboarding est terminé. L'accord nomme le
paquet, le module et la source que porte le binaire : un outil dont le module
sur le disque n'est pas cette source octet pour octet, quel que soit le nom de
son paquet, n'obtient rien.

## Intégration hôte {#host-integration}

L'API HTTP versionnée est un contrat de pilotage, pas un détail
d'implémentation. Elle porte un schéma OpenAPI généré et des clients TypeScript
et Python générés, si bien qu'un produit hôte pilote le runtime sans avoir à
rétro-concevoir des modules de routes.

Les ruptures passent par un nouveau préfixe de version, jamais par une mutation
silencieuse de la version courante.

## Surface d'erreur publique {#public-error-enums}

Toute énumération d'erreur publique de l'espace de travail porte
`#[non_exhaustive]`, et `scripts/check_rust_rules.py` en refuse une qui ne le
porte pas. Ajouter une variante à un type d'erreur devient alors un changement
ordinaire, plutôt qu'une rupture pour chaque crate qui filtre dessus, ce que
l'attribut existe précisément pour absorber.

`apollia-core` est scellée comme les autres, et c'est la crate où l'attribut
rapporte le plus : toutes les autres en dépendent, donc une variante non
absorbée y casserait l'espace de travail entier d'un coup.

Le coût tombe du côté du filtrage, et seulement au franchissement d'une
frontière de crate. Un `match` sur une de ces énumérations depuis une autre
crate demande un bras de repli, et le compilateur cesse d'exiger que ce `match`
soit exhaustif, donc une variante ajoutée plus tard atteint le repli au lieu de
casser la compilation. Trois `match` inter-crates étaient exhaustifs quand
l'attribut est arrivé, tous les trois traduisant une erreur du runtime en statut
HTTP ; chacun répond désormais à une variante non filtrée par un 500 portant le
message de l'erreur, plutôt que par une panique.

## Plateformes et publication {#platforms-and-release}

Linux, macOS et Windows sont des cibles supportées. Le confinement des outils
diffère entre elles, comme dit plus haut.

Les versions sont publiées sur GitHub Releases. L'application de bureau consulte
ce flux uniquement quand l'opérateur le demande, jamais en arrière-plan, et
rapporte un flux vide comme un flux vide plutôt que comme une erreur.

Il y a un installeur par système. Aucun installeur n'existe en seconde édition
pour une sorte de carte graphique : une édition par carte faisait choisir un
téléchargement d'après un matériel que le lecteur ne connaît pas forcément, et
liait la version d'un moteur à une version de l'application. Les moteurs qu'un
installeur n'embarque pas, la compilation CUDA de `llama-server` et le Node.js
qui fait tourner les serveurs d'outils, sont publiés à part, sur leur propre
canal, sous un manifeste signé au format minisign avec une clé de distribution
dédiée (voir [Inférence locale](#local-inference)). La chaîne qui les publie
est distincte de celle qui publie l'application et se lance à la main : elle
prend des archives amont épinglées par SHA-256, reconditionne chacune en zip, et
signe le manifeste avec une clé qui ne signe rien d'autre, distincte de la clé
des mises à jour de l'application. Le registre d'Apollia sert le manifeste et
les archives, et GitHub Releases en est le miroir. Une application installée
depuis une ancienne édition CUDA continue de se mettre à jour : le manifeste de
mise à jour qu'elle demande est toujours publié, en copie du manifeste standard,
si bien qu'elle passe à l'installeur unique de son système et reçoit le moteur
CUDA par le canal des moteurs.

## Vérification {#verification}

Au-delà de la suite de tests, deux classes de propriétés sont vérifiées par la
machine plutôt que par la relecture : les entrelacements de concurrence des
algorithmes d'acteurs, et les deux invariants cardinaux, le budget d'étapes non
contournable et le verrou de bail de la boîte aux lettres, prouvés sous exécution
symbolique bornée.

Elles tournent sur planification plutôt qu'à chaque changement, parce qu'elles
sont lentes. Chacune a une contrepartie dans l'arbre qui tourne avec la suite de
tests ordinaire, si bien qu'une régression apparaît au moment habituel et que le
travail planifié la confirme.

## Documentation {#documentation}

Ce site est la documentation, organisé selon ce que le lecteur cherche à faire :
des tutoriels pour apprendre, des guides pratiques pour accomplir, une référence
pour consulter, des explications pour comprendre.

Les références de la ligne de commande et du SDK sont générées depuis le code
puis commitées, avec un contrôle qui échoue quand les pages commitées dérivent de
ce que le code produirait. Une référence qui peut dériver en silence est pire que
pas de référence.
