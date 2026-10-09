---
sidebar_position: 6
title: Le modèle de confiance de l'agent
description: Un agent est une fiche et ne porte aucun code ; le Python d'un paquet est du code de confiance. Ce qu'Apollia confine, ce qu'il décide appel par appel, et ce qu'il laisse au choix d'installer un paquet.
---

# Le modèle de confiance de l'agent

Un agent est une fiche : une mission, un modèle, les outils qui lui sont
accordés, les sources qu'il peut lire. Il ne porte aucun code. Le moteur
intégré au runtime l'exécute, et chaque appel d'outil qu'il fait est décidé par
la gouvernance avant de s'exécuter. Le code qui s'exécute avec vos droits est
ailleurs : le Python qu'un paquet apporte, ses outils et ses workflows. Cette
page énonce clairement ce qui confine chacun des deux, ce qui ne le confine
pas, et ce qu'un opérateur, en particulier un opérateur soumis à régulation,
doit supposer avant d'installer un paquet qu'il n'a pas écrit lui-même.

Surestimer l'isolation serait pire qu'inutile ici : un adoptant soumis à
régulation qui croit qu'un code est confiné dans un sandbox alors qu'il ne
l'est pas prend ses décisions sur une prémisse fausse. Cette page est donc
délibérément prudente sur ce qu'Apollia garantit.

## Ce qui s'exécute où

Trois choses se situent à des niveaux de confiance différents.

**Un agent est une donnée, et ses appels d'outils sont décidés un par un.** Un
paquet déclare un agent comme une entrée `[[agents]]` de son `apollia.toml`, ou
la personne en crée un dans l'application. Ce qu'il peut atteindre est ce que
sa fiche accorde : des outils, des sources, des dossiers. Le modèle choisit
l'appel suivant, et c'est le runtime, pas le modèle, qui décide si cet appel
s'exécute (voir plus bas). Un agent n'a aucun moyen d'exécuter quoi que ce soit
en dehors des outils qui lui ont été accordés.

**Le Python d'un paquet est du code de confiance.** Un paquet peut apporter des
outils (`@tool`) et des workflows (`@workflow`). Ils sont chargés dans le
processus via le pont PyO3 et s'exécutent avec l'intégralité des droits du
processus runtime : le système de fichiers, le réseau, le lancement de
processus, et tous les identifiants que le compte utilisateur peut lire. Il n'y
a aucun sandbox au niveau du système d'exploitation autour de ce code, aucun
processus dédié par paquet, et aucun confinement au niveau du langage. L'objet
`ctx` qu'un outil de paquet reçoit ne porte ni modèle, ni proxy d'outils, ni
mémoire, et un workflow y ajoute `ctx.llm`, `ctx.agents`, `ctx.workflows` et
`ctx.user` ; c'est la surface que le runtime prend en charge, pas un mur. Un
code de paquet malveillant ou bogué peut faire tout ce que l'utilisateur
courant peut faire. Un paquet sans Python, un manifeste et rien d'autre,
n'apporte aucun code de ce genre.

<!-- claim:tool-sandbox-covers-child-processes-only -->
**Deux outils constituent la surface confinée, et seulement leur processus
enfant.** Quand un agent appelle `bash_executor` ou `python_executor`, cet
outil lance un processus enfant, et c'est ce processus enfant qu'Apollia
confine :

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

Tous les autres outils natifs s'exécutent sans confinement dans le processus
runtime. Les outils de système de fichiers sont bornés par une vérification
de préfixe de chemin : une racine canonicalisée qu'ils refusent de quitter,
échappatoires par lien symbolique incluses. Cette racine est **l'union des
dossiers accordés à l'agent**, dans une conversation comme dans une tâche ; un
agent sans dossier accordé n'en atteint aucun. Les outils qui récupèrent par
HTTP passent par une vérification d'adresse qui refuse les destinations de
bouclage, privées et de lien local, sur la première URL et à chaque
redirection. Ni l'une ni l'autre n'est une frontière du système
d'exploitation.

Trois conséquences à retenir. Un namespace mount sans `pivot_root` n'est pas
une prison de système de fichiers (filesystem jail) : le processus enfant
voit le même système de fichiers que vous. Une vérification de préfixe de
chemin est une garantie applicative, pas une garantie du noyau, et elle lie
les outils du runtime, pas le code d'un paquet. Et un exécuteur de code
accordé à un agent exécute ce que le modèle a écrit, avec le confinement de sa
plateforme et rien de plus.

**Dans cette documentation, le mot sandbox n'a qu'un seul sens : le
confinement au niveau du système d'exploitation du processus enfant d'un
outil.** Il ne désigne jamais un agent, jamais la racine de chemin des outils
de système de fichiers, et jamais un environnement de test jetable.

## Ce qui tient réellement la ligne

Les contrôles sont superposés, et ils ne couvrent pas tous la même chose.

- **L'aperçu avant une installation.** Un paquet s'installe depuis un fichier
  `.apo`, et quoi qui apporte le fichier, il est d'abord présenté en aperçu :
  ce qu'il contient, qui l'a signé, ce que chacun de ses agents demande. Rien
  n'est écrit avant que la personne accepte. L'aperçu porte des avis : le
  paquet n'est pas signé, la clé qui l'a signé n'est pas parmi les clés de
  confiance, une connexion dont un agent a besoin manque, un serveur MCP du
  même nom existe déjà. Une politique d'organisation peut refuser un paquet
  sans signature valide (`[packages] require_signature`). C'est le contrôle qui
  couvre le code d'un paquet, et il est humain : relire le Python d'un paquet
  avant de l'installer relève de la responsabilité de l'opérateur.
- **La décision de gouvernance sur chaque appel d'outil.**
  <!-- claim:hitl-wired-in-chat-path-only -->
  Avant qu'un outil demandé par un agent ne s'exécute, une seule décision y
  répond, dans une conversation, dans une tâche, dans une étape de plan et
  dans un élément de lot : la politique de l'organisation et une règle de
  refus le refusent, un outil qui ne fait que lire s'exécute, et tout ce qui
  est sensible (écrire, envoyer hors de la machine, lire des données
  personnelles, exécuter du code) s'exécute sur un accord permanent de cet
  agent pour cette famille d'action, ou demande à une personne, qui répond
  une fois, toujours ou refuser. Un outil de paquet est décidé de la même
  façon, par la famille que son auteur a déclarée. Ce que la décision
  gouverne, c'est l'appel ; ce qu'un outil de paquet fait une fois lancé,
  c'est son propre code.
- **Les accords de la fiche.** Ce qu'un paquet déclare pour un agent est un
  ensemble de valeurs par défaut. La personne change un outil, une source ou
  un dossier depuis la fiche de l'agent, et un agent n'atteint que ce qui y
  est accordé.
- **Garde-fous du runtime.** Le budget d'étapes et la piste d'audit sont
  appliqués par le runtime et ne peuvent pas être reconfigurés par un agent.
  Les exécuteurs de code ne sont jamais couverts par un accord permanent :
  chaque exécution demande, sauf une commande simple et unique qu'une règle
  autorise en mode développeur.
- **La politique de l'organisation.** Une politique signée déposée dans un
  dossier système refuse des familles d'action, des outils ou des connexions
  pour tous les agents de la machine, avant tout accord.

## Ce qu'un opérateur doit supposer

Si vous installez un paquet qui porte du Python que vous n'avez ni écrit ni
audité, supposez que ce code peut lire et exfiltrer tout ce que votre compte
utilisateur peut atteindre sur la machine, identifiants compris. Les
atténuations sont la relecture avant installation et la signature d'un auteur
en qui vous avez confiance, pas un mur technique autour du code. Un agent sans
code est un cas différent : ce qu'il peut faire est ce que font ses outils, et
chacun de ces appels passe par la décision ci-dessus. Pour un déploiement
soumis à régulation, cela veut dire : exécuter Apollia sous un compte
utilisateur limité à ce dont la charge de travail a besoin, exiger par
politique des paquets signés, auditer le code d'un paquet avant de
l'installer, et accorder les exécuteurs de code avec parcimonie.

## Voir aussi

- [Outils et confinement](/architecture/decisions#tools-and-sandbox) énonce la décision de confinement et les
  alternatives écartées.
- [Le modèle de permissions](/architecture/decisions#permission-model) énonce
  l'ordre de la décision de gouvernance.
- [Le modèle de responsabilité](/explanation/accountability-model) couvre
  l'audit et l'approbation en profondeur.
- [Souveraineté et local-first](/explanation/sovereignty-and-local-first) couvre
  la posture de résidence des données.
