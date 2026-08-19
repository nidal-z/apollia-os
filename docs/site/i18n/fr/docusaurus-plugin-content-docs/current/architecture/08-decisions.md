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

Le runtime est en Rust sur Tokio. Les agents sont en Python, chargés dans le
processus via un pont PyO3 plutôt que lancés en sous-processus : le pont traduit
directement les futures Rust en coroutines Python, donc un appel d'agent ne coûte
ni frontière de processus ni aller-retour de sérialisation.

Tout sous-système qui détient un état mutable est un acteur Tokio, avec un canal
borné et une poignée clonable. Aucun état n'est partagé entre acteurs derrière un
verrou. Le coût est une indirection, et il achète la propriété qui compte à cette
taille : un interblocage ne peut pas se former entre deux sous-systèmes qui
n'échangent que des messages.

La persistance est SQLite en mode WAL, embarqué dans le binaire. Rien, sur le
chemin par défaut, n'exige un serveur, un démon ou un réseau.

## Le contrat d'agent {#agent-contract}

Un agent est une classe Python portant `@agent`, avec au moins une méthode
asynchrone `@skill` ou `@on_message`. C'est tout le contrat. Le pont refuse un
objet qui n'expose pas le point d'entrée de dispatch que les décorateurs
installent ; il n'existe aucune échappatoire acceptant un appelable quelconque.

Le runtime remet à chaque appel un objet `ctx` qui expose les services qu'un
agent peut utiliser : le routeur LLM, la mémoire, les outils, la messagerie
entre agents, les notifications et un logger. `ctx` est toute la surface du
runtime. Un agent qui passe à côté utilise quelque chose que le runtime ne
garantit pas.

Les schémas de charge utile sont des `TypedDict`, lus à l'enregistrement pour
construire les schémas de skills. C'est pourquoi l'évaluation différée des
annotations est refusée dans ces modules : elle transforme les annotations en
chaînes, et le schéma ressort vide sans la moindre erreur.

Le SDK Python n'a aucune dépendance tierce. Chacune deviendrait une dépendance
de chaque agent, sur chaque machine, pour toujours.

## Modèle d'exécution {#execution-model}

Le moteur autonome observe, raisonne et agit en boucle. Il fonctionne selon deux
modes : direct, où un skill répond, et orchestré, où un plan est construit puis
ses étapes exécutées.

Sur le chemin orchestré, les arguments d'étape sont remplis au moment de la
planification, sous grammaire, avec une extraction à la volée en repli au moment
de l'exécution. C'est ce remplissage à la planification qui permet à un plan de
piloter de vrais outils avec des arguments structurés, au lieu de réanalyser de
la prose à chaque étape.

Une exécution orchestrée terminée est vérifiée par un critique. Le verdict est
consigné comme événement du runtime, et un verdict négatif déclenche une
replanification bornée, sous le budget qui bornait déjà l'exécution d'origine.

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

Le confinement n'est pas uniforme selon les plateformes, et la différence n'est
pas cosmétique. Sous Linux, l'exécution shell tourne sous espaces de noms PID et
mount. Sous macOS le confinement est plus faible. Sous Windows il n'y en a pas.
C'est dit franchement plutôt que lissé, parce qu'un opérateur qui choisit une
plateforme choisit un modèle de menace. Voir [le modèle de confiance des
agents](/explanation/agent-trust-model) pour ce que cela implique en pratique.

Le trafic HTTP sortant est vérifié contre les plages d'adresses privées à chaque
saut de redirection, pas seulement sur la première requête, donc une URL publique
ne peut pas rediriger vers le réseau local.

## Modèle de permissions {#permission-model}

Les appels d'outils passent par une couche de gouvernance qui résout une
décision à partir de règles persistées. Une règle porte une portée : session,
projet, agent ou globale. Les refus priment sur les autorisations à spécificité
égale, et la décision est journalisée dans les deux sens.

Les exécuteurs de code ne sont jamais autorisés en bloc. Une règle qui
accorderait tous les outils n'accorde pas l'exécution shell ni l'exécution
Python ; celles-ci exigent leur propre autorisation explicite. Une autorisation
en bloc est en général une décision de confort à propos de la lecture de
fichiers, et elle ne doit pas devenir en silence le droit d'exécuter du code
arbitraire.

Les chaînes de commande qui atteignent un exécuteur shell sont analysées en
tenant compte des guillemets, si bien que le chaînage, la redirection et les
constructions qui redirigent vers un interpréteur sont refusés plutôt que
manqués par une règle naïve de sous-chaîne.

## Humain dans la boucle {#human-in-the-loop}

Tout outil peut exiger une approbation humaine avant de s'exécuter. Quand c'est
le cas, le runtime suspend l'exécution, émet un événement portant ce qui est
demandé, et reprend sur la réponse. La suspension est un état de premier rang,
pas une attente bloquante : le processus est libre, et une exécution peut rester
en attente au travers d'un redémarrage.

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
intégré : un brief de persona utilisateur au palier d'autonomie le plus long, et
les résumés de sessions passées au premier message d'une conversation libre. La
troisième vit en dehors de l'assistant, dans la commande de réécriture du bureau,
qui porte la section Travail du profil utilisateur dans son propre prompt ponctuel
et rend du texte au composeur plutôt qu'à une exécution.

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

Les backends cloud se configurent un par un avec une clé d'API. Il n'y a pas de
flux OAuth pour un fournisseur de modèle cloud, parce qu'aucun n'en propose pour
cet usage.

## Reconnaissance vocale {#speech-to-text}

La transcription tourne hors processus, dans un sidecar bâti sur whisper, si
bien qu'un plantage de modèle ne peut pas emporter le démon. Elle est
optionnelle : une compilation sans elle perd la dictée, et rien d'autre.

## Model Context Protocol {#mcp}

Apollia est un client MCP sur les transports stdio et HTTP, et expose aussi ses
propres outils natifs comme serveur MCP sur stdio.

Toute réponse MCP est traitée comme une entrée non fiable. Les réponses sont
plafonnées par serveur, les noms et descriptions d'outils sont validés dès
l'ingestion contre un jeu de caractères et une longueur maximale, et le nombre
d'outils qu'un seul serveur peut apporter est borné. Un serveur qui se comporte
mal se dégrade lui-même, pas le runtime.

Les serveurs qui exigent OAuth passent par un flux dédié, avec son propre
plafond de réponse, plus petit.

## Connecteurs {#connectors}

Les connecteurs Google et Microsoft s'authentifient en OAuth2 avec PKCE. Aucun
agrégateur ne s'intercale : un relais tiers payant entre un opérateur et sa
propre boîte mail contredit la raison d'être du produit.

Le binaire embarque un client OAuth Microsoft et aucun client Google. Les portées
restreintes de Google exigent un processus de vérification que le projet n'a pas
mené, donc une connexion Google demande à l'opérateur de fournir ses propres
identifiants client. L'asymétrie est assumée, et elle est énoncée là où
l'opérateur la rencontre.

## Messagerie entre agents {#agent-messaging}

Un agent directeur délègue à des agents travailleurs par identifiant de skill,
au travers d'une boîte aux lettres durable adossée à SQLite. La livraison se
fait par bail : un consommateur prend un bail, et l'acquittement est verrouillé
sur l'exécution qui le détient, si bien qu'un consommateur périmé dont le bail a
été réattribué ne peut ni supprimer ni remettre en file le message que le nouveau
détenteur est en train de traiter.

Le dispatch propage l'identifiant de skill complet. Une clé de dispatch plus
courte a été utilisée une fois, et elle a produit de l'ambiguïté dès que deux
travailleurs ont exposé des skills aux noms voisins.

## Audit et preuve {#audit-and-evidence}

Chaque appel d'outil est enregistré : ce qui a tourné, une empreinte de ses
entrées, s'il a réussi, et combien de temps il a pris. Un appel qui a échoué est
persisté comme ayant échoué. La piste est écrite sans attendre, si bien que
l'enregistrement ne bloque jamais l'exécution.

Les enregistrements sont chaînés, et la chaîne est ancrée globalement pour que
la troncature soit détectable : retirer la fin du journal casse une vérification
qu'un lecteur peut lancer lui-même.

Rejouer une exécution puis la comparer à son enregistrement n'est délibérément
pas construit. Une réexécution prouve qu'une seconde exécution s'est comportée
d'une certaine façon, pas que la première l'a fait, et la responsabilité repose
déjà sur la chaîne signée. C'est nommé ici pour que cette absence se lise comme
un choix et non comme un manque.

## Secrets et authentification de l'API {#secrets-and-api-auth}

Les secrets vivent dans le trousseau du système, ou dans un fichier chiffré avec
age là où il n'existe pas de trousseau. Ils ne sont jamais écrits dans un fichier
de configuration.

L'API HTTP écoute sur une socket Unix et, en option, sur un port TCP. La socket
Unix relève de la confiance locale et s'appuie sur les permissions du système de
fichiers. Le TCP exige un jeton porteur sur tous les chemins, et le lier à une
adresse non locale sans TLS est une erreur de démarrage plutôt qu'un
avertissement : un attachement distant non sécurisé est la seule erreur qu'on ne
peut pas défaire une fois que le trafic est passé.

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

## Distribution des agents {#agent-distribution}

Un agent s'installe depuis un chemin local ou une URL Git. Publier les agents sur
un index de paquets a été envisagé puis écarté : cela ferait de l'index une
dépendance de disponibilité pour un produit dont la première promesse est de
fonctionner sans aucune.

Les dépendances Python tierces déclarées par un agent sont installées dans
l'environnement virtuel propre à cet agent, après consentement explicite, et
l'opérateur voit la liste avant que cela n'arrive.

## Intégration hôte {#host-integration}

L'API HTTP versionnée est un contrat de pilotage, pas un détail
d'implémentation. Elle porte un schéma OpenAPI généré et des clients TypeScript
et Python générés, si bien qu'un produit hôte pilote le runtime sans avoir à
rétro-concevoir des modules de routes.

Les ruptures passent par un nouveau préfixe de version, jamais par une mutation
silencieuse de la version courante.

## Plateformes et publication {#platforms-and-release}

Linux, macOS et Windows sont des cibles supportées. Le confinement des outils
diffère entre elles, comme dit plus haut.

Les versions sont publiées sur GitHub Releases. L'application de bureau consulte
ce flux uniquement quand l'opérateur le demande, jamais en arrière-plan, et
rapporte un flux vide comme un flux vide plutôt que comme une erreur.

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
