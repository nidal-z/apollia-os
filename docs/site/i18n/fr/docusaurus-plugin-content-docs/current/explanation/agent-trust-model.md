---
sidebar_position: 6
title: Le modèle de confiance de l'agent
description: Un agent est du Python arbitraire. Ce qu'Apollia isole et ce qu'il n'isole pas, quelles frontières sont réelles aujourd'hui, et ce que cela implique.
---

# Le modèle de confiance de l'agent

Un agent est du code Python arbitraire. Apollia l'exécute dans le même
processus que le runtime, avec les mêmes droits que la personne qui a démarré
Apollia. Cette page énonce clairement ce que cela signifie, ce qui confine
l'agent et ce qui ne le confine pas, et ce qu'un opérateur, en particulier un
opérateur soumis à régulation, doit supposer avant de déployer un agent qu'il
n'a pas écrit lui-même.

Surestimer l'isolation serait pire qu'inutile ici : un adoptant soumis à
régulation qui croit qu'un agent est confiné dans un sandbox alors qu'il ne
l'est pas prend ses décisions sur une prémisse fausse. Cette page est donc
délibérément prudente sur ce qu'Apollia garantit.

## Ce qui s'exécute où

Deux corps de code se situent à des niveaux de confiance différents.

**Le code Python de l'agent est du code de confiance.** Il est chargé dans le
même processus via le pont PyO3, et s'exécute avec l'intégralité des droits
du processus runtime : le système de fichiers, le réseau, le lancement de
processus, et l'accès en lecture aux identifiants dans le trousseau
(keyring). Il n'y a aucun sandbox au niveau du système d'exploitation autour
de l'agent lui-même, aucune isolation par processus dédié par agent, et aucun
confinement au niveau du langage. Un agent malveillant ou bogué peut faire
tout ce que l'utilisateur courant peut faire. Il s'agit d'une décision
délibérée pour la v0.1.0 (voir [outils et confinement](/architecture/decisions#tools-and-sandbox)) : le public visé est
celui des builders qui écrivent ou auditent les agents qu'ils exécutent.

<!-- claim:tool-sandbox-covers-child-processes-only -->
**Deux outils constituent la surface confinée, et seulement leur processus
enfant.** Quand un agent appelle `bash_executor` ou `python_executor`, cet
outil lance un processus enfant, et c'est ce processus enfant, pas l'agent,
qu'Apollia confine :

- Sur Linux, les commandes d'outil s'exécutent à l'intérieur d'espaces de
  noms (namespaces) PID et mount, via `unshare`.
- Sur macOS, il n'y a aucun sandbox au niveau OS pour les outils ; Apollia
  émet un avertissement à chaque invocation d'outil, pour que cette absence
  soit impossible à manquer. L'isolation des outils en production exige
  Linux.
- Sur tout système Unix, les processus enfants des outils portent des
  limites de ressources par processus appliquées avec `setrlimit` : le
  temps CPU et le nombre de descripteurs de fichiers ouverts partout, plus
  l'espace d'adressage sur Linux (macOS rejette la limite d'espace
  d'adressage, donc Apollia ne l'y applique pas).
<!-- claim:windows-has-no-tool-sandbox -->
- Sur Windows, il n'y a **aucun confinement, quel qu'il soit** : ni
  namespaces, ni limites de ressources non plus, car `setrlimit` n'a pas
  d'équivalent Windows et la fonction qui l'applique est vide sur les
  cibles non-Unix. Un appel d'outil sur Windows s'exécute avec les mêmes
  droits que l'application. `bash_executor` a de plus besoin d'un shell
  POSIX présent dans `PATH` (Git Bash, WSL ou MSYS2), et échoue en son
  absence.

Tous les autres outils s'exécutent sans confinement dans le processus
runtime. Les outils de système de fichiers sont bornés par une vérification
de préfixe de chemin : une racine canonicalisée qu'ils refusent de quitter,
échappatoires par lien symbolique incluses. Cette racine est l'espace de
travail dans une session de chat, et **le répertoire personnel entier de
l'utilisateur** pour un agent installé. Les outils réseau sont bornés par une
liste d'autorisation (allowlist) au niveau applicatif. Ni l'une ni l'autre
n'est une frontière du système d'exploitation.

Trois conséquences à retenir. Un namespace mount sans `pivot_root` n'est pas
une prison de système de fichiers (filesystem jail) : le processus enfant
voit le même système de fichiers que vous. Une vérification de préfixe de
chemin est une garantie applicative, pas une garantie du noyau, et ne
survit pas à un outil qui l'ignore. Et rien de tout cela ne s'applique au
code propre de l'agent, qui peut atteindre directement ce que les outils lui
refusent.

**Dans cette documentation, le mot sandbox n'a qu'un seul sens : le
confinement au niveau du système d'exploitation du processus enfant d'un
outil.** Il ne désigne jamais l'agent, jamais la racine de chemin des outils
de système de fichiers, et jamais un environnement de test jetable.

## Ce qui tient réellement la ligne

Parce que l'agent n'est pas confiné dans un sandbox, les contrôles réels sont
procéduraux et reposent sur l'humain dans la boucle, superposés en défense en
profondeur.

- **Audit avant installation.** L'opérateur est responsable de la relecture
  d'un agent avant de l'installer. L'installation en ligne de commande
  affiche un avis rappelant que l'agent s'exécutera avec l'intégralité des
  droits de l'utilisateur et sans sandbox.
- **Approbation humaine (HITL).** Dans une session de chat, les écritures de
  fichiers, les modifications, ainsi que l'exécution shell et Python passent
  par un wrapper d'approbation dont la décision par défaut est de demander :
  l'action remonte à l'opérateur plutôt que de s'exécuter silencieusement.
  <!-- claim:hitl-wired-in-chat-path-only -->
  **Ce wrapper n'est pas placé sur le dispatcher d'un agent installé.** Les
  propres appels `ctx.tools` d'un agent ne rencontrent aucun point de
  contrôle humain, ce qui est cohérent avec le reste de cette page : un
  agent exécute déjà du code Python arbitraire sous votre compte, donc une
  barrière sur un seul chemin d'appel ne contiendrait pas un chemin
  hostile. Considérez le HITL comme une supervision du chemin
  conversationnel, pas comme un confinement de l'agent.
- **Déclarations de capacités.** Le manifest d'un agent déclare les outils,
  secrets, sources de données et messagerie qu'il prévoit d'utiliser, et les
  interfaces `ctx.*` correspondantes font respecter ces listes d'autorisation
  par défaut. C'est de l'ergonomie de moindre privilège, pas une frontière du
  système d'exploitation : un agent non confiné dans un sandbox peut ignorer
  `ctx.secrets` et lire l'environnement directement. Considérez ces listes
  d'autorisation comme un mécanisme de clarté et de confort, pas comme un
  confinement.
- **Garde-fous du runtime.** Le budget de pas et le journal d'audit sont
  appliqués par le runtime, indépendamment du modèle de confiance du système
  d'exploitation, et ne peuvent pas être désactivés par reconfiguration
  depuis l'agent. Les règles de permission persistées s'appliquent sur le
  chemin du chat, évaluées à chaque invocation ; les exécuteurs de code sont
  exclus de toute autorisation globale, et ne correspondent que via une
  règle de préfixe restreinte à une seule commande simple.

## Ce qu'un opérateur doit supposer

Si vous déployez un agent que vous n'avez ni écrit ni audité, supposez qu'il
peut lire et exfiltrer tout ce que votre compte utilisateur peut atteindre
sur la machine, y compris les identifiants. Les mesures d'atténuation sont la
relecture de la chaîne d'installation et les invites d'approbation, pas un
mur technique autour de l'agent. Pour un déploiement soumis à régulation,
cela signifie : exécuter Apollia sous un compte utilisateur restreint au
strict nécessaire pour la charge de travail, garder la barrière d'approbation
active pour tout ce qui est sensible, et auditer les agents avant de les
installer.

## Vers où l'isolation se dirige

La posture de la v0.1.0 est honnête sur ses limites, et plusieurs d'entre
elles sont sur la feuille de route plutôt que livrées :

- Une exécution hors processus, confinée par sandbox au niveau OS, pour les
  agents non fiables.
- Une application par profil pour les outils (namespace réseau et listes
  d'autorisation de sortie, un montage à portée d'écriture délimitée), afin
  que le profil de sandbox déclaré devienne une contrainte réelle plutôt
  qu'une simple métadonnée.
- Une véritable prison de système de fichiers pour les exécuteurs shell et
  Python.
- L'application du profil de souveraineté (`local_only`) comme barrière
  automatique.

Tant que ces éléments ne sont pas livrés, le modèle de confiance décrit
ci-dessus est toute l'histoire. Quand ils le seront, cette page et le
registre de décision seront mis à jour en conséquence, jamais en avance sur
le code.

## Voir aussi

- [Outils et confinement](/architecture/decisions#tools-and-sandbox) énonce la décision relative au confinement et à la confiance
  envers l'agent, ainsi que les alternatives rejetées.
- [Le modèle de responsabilité](/explanation/accountability-model) couvre
  l'audit et l'approbation en détail.
- [Souveraineté et local-first](/explanation/sovereignty-and-local-first)
  couvre la posture de résidence des données.
