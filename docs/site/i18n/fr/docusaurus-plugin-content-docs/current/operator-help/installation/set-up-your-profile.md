---
title: Configurer votre profil au premier lancement
slug: /operator-help/installation/set-up-your-profile
sidebar_position: 4
---

# Configurer votre profil au premier lancement

> Au premier démarrage d'Apollia (ou après une réinitialisation), un parcours de configuration s'ouvre automatiquement.
> Il vous fait passer en sept étapes - la langue, l'IA, les modèles de recherche par le sens et de dictée, une courte conversation qui propose votre profil, un premier agent, une connexion dont cet agent a besoin, une première tâche avec lui - pour qu'Apollia soit prêt à l'emploi. Il ne vous demande jamais de choisir un profil d'usage : l'interface est la même pour tous, et les écrans techniques supplémentaires relèvent du mode développeur, un interrupteur dans **Réglages → Développeur**, éteint par défaut.
> Durée totale : 3 à 8 minutes selon le téléchargement des modèles.
>
> Cette page couvre **le parcours initial uniquement**. Pour éditer votre profil au quotidien depuis l'application installée, voir **[Mon profil](../memoire/manage-my-profile.md)**.

## Prérequis

- Apollia est installé et lancé pour la première fois (ou votre profil a été réinitialisé).
- Connexion internet active pendant l'étape **IA** si vous téléchargez un modèle local.

## Vue d'ensemble du parcours

Le parcours est rythmé par un rail de progression visible en haut de la fenêtre, surmonté du compte des étapes : **Langue · IA · Recherche, voix · Qui je suis · Premier agent · Connexion · Première tâche**. Vous pouvez revenir en arrière à toute étape, et **Configurer plus tard** est disponible en haut à droite de la fenêtre, à chaque étape, pour ajourner l'ensemble.

| Étape | Ce qui s'y passe |
|---|---|
| **Langue** | Un mot de bienvenue et le choix de la langue de l'interface. |
| **IA** | Le modèle de langage avec lequel vos agents travaillent : un modèle sur votre poste, avec téléchargement embarqué, ou un serveur que vous utilisez déjà. C'est la seule étape que vous ne pouvez pas passer. |
| **Recherche, voix** | Deux modèles plus petits, à côté de celui qui converse : l'un permet à vos agents de retrouver un document par son sens, l'autre transcrit votre voix pour que vous puissiez dicter. Chacun se télécharge ou s'importe depuis un fichier, puis s'essaie sur place : **Essayer** fait répondre le modèle de recherche, et le bloc de dictée enregistre quelques mots et affiche leur transcription. L'étape peut être passée ; la dictée est facultative. |
| **Qui je suis** | Une courte conversation avec un agent, dont chaque question peut être passée. L'agent propose ensuite un profil, que vous relisez et corrigez avant qu'il soit enregistré. |
| **Premier agent** | Trois façons d'obtenir un agent prêt à travailler : installer un paquet, en créer un en décrivant votre besoin, ou partir d'un exemple. Elle peut être passée. |
| **Connexion** | Les connexions dont votre premier agent a besoin et que vous n'avez pas encore, chacune ajoutée sur place. Elle n'apparaît que s'il en manque une, et elle peut être passée. |
| **Première tâche** | La fenêtre se ferme sur votre agent au travail : une conversation avec son premier exemple prêt à envoyer, ou un lot sur un classeur d'exemple. Elle n'a pas d'écran à elle. |

## Étapes détaillées

### Étape 1 - Langue

Au lancement, Apollia détecte qu'aucun profil n'existe et ouvre la fenêtre de configuration centrée sur l'écran.

Choisissez **Français** ou **English** : la fenêtre bascule aussitôt, et toutes les étapes suivantes se lisent dans la langue choisie. Vous pourrez la changer à tout moment dans **Réglages → Général**. Cliquez ensuite sur **Continuer**.

### Étape 2 - Choisir l'IA

C'est la seule étape que vous ne pouvez pas passer : sans modèle ni serveur, aucun agent ne peut répondre. Un bandeau d'informations système (RAM, OS, GPU) apparaît en haut pour vous aider à choisir. Rien d'autre n'est demandé ici. La fenêtre de contexte suit le défaut du modèle, et la dictée vocale se règle plus tard dans **Réglages → Dictée** ; son modèle s'installe et s'essaie à l'étape suivante.

Deux cartes ouvrent l'étape : **Sur ce poste** et **Utiliser un serveur**. Quatre chemins possibles, selon votre situation, les trois premiers sur ce poste :

1. **Modèles déjà présents sur la machine.** Si vous avez déjà placé un fichier `.gguf` dans `~/.apollia/models/` ou `~/Downloads/`, Apollia le détecte automatiquement. Cliquez sur la ligne correspondante pour le configurer en un clic.
2. **Modèles recommandés à télécharger.** Apollia affiche une liste de modèles dimensionnés pour votre machine, avec un badge **Recommandé** sur le plus pertinent. La liste reste proposée qu'un GGUF ait déjà été trouvé ou non, et après la configuration d'un premier modèle. Cliquez sur la ligne pour lancer le téléchargement - une barre de progression avec débit en MB/s s'affiche, vous pouvez l'annuler à tout moment.
3. **Recherche HuggingFace.** Le bouton **Rechercher sur HuggingFace** ouvre un mini-navigateur intégré : tapez le nom d'un modèle, dépliez les fichiers GGUF disponibles, et cliquez pour télécharger celui qui correspond à votre RAM (les fichiers sont étiquetés *fits* / *might fit* / *too large*).
4. **Un serveur.** Cliquez sur la carte **Utiliser un serveur** quand votre organisation fait tourner un serveur d'IA, ou quand vous en utilisez déjà un (Ollama, llama-server, vLLM, ou tout serveur à l'interface compatible OpenAI). Le formulaire reste dans la fenêtre : saisissez l'**adresse du serveur** (par exemple `http://localhost:11434`), ajoutez la **clé** seulement si le serveur en demande une, puis cliquez sur **Tester**. Apollia liste les modèles que le serveur propose ; choisissez-en un, il devient le modèle par défaut de vos agents. Si le test échoue, le message dit quoi vérifier : l'adresse, la clé, ou le démarrage du serveur.

**Continuer** devient actif dès qu'un modèle est configuré (badge vert **Configuré**), qu'un modèle d'un serveur est choisi, ou qu'un téléchargement est lancé. Vous n'avez pas à attendre la fin du téléchargement : poursuivez les étapes suivantes pendant que le fichier arrive. Sa progression reste affichée en haut de la fenêtre à chaque étape, et le modèle s'installe de lui-même quand le fichier est là.

#### Boutons disponibles en pied de page

- **← Retour** - revient à l'étape Langue.
- **Continuer** - désactivé tant qu'il n'y a ni modèle, ni serveur, ni téléchargement en cours.

### Étape 3 - Retrouver vos documents par le sens

Apollia propose un second modèle, plus petit que celui qui converse, recommandé pour votre machine. Avec lui, vos agents retrouvent une idée dans vos documents même formulée autrement.

- **Télécharger** récupère le modèle recommandé ; **Importer depuis un fichier** le prend dans un fichier que vous avez déjà, ce qui est le moyen d'une machine sans connexion.
- **Passer** montre ce à quoi vous renoncez avant de confirmer : la recherche par le sens dans vos sources et dans la mémoire de vos agents. La recherche par mots-clés reste possible, et le modèle pourra être installé plus tard depuis l'écran IA. **Continuer sans ce modèle** confirme.

### Étape 4 - Une courte conversation, puis votre profil à relire

Si le modèle choisi à l'étape 2 est encore en téléchargement, la fenêtre le dit et la conversation commence d'elle-même dès que le modèle est prêt.

L'agent d'onboarding s'ouvre dans la fenêtre. Il pose quelques questions, un sujet à la fois, et chacune peut être passée.

Les questions couvrent les champs d'un profil :

| Champ | Pourquoi |
|---|---|
| Nom | Les agents vous appellent par votre nom |
| Rôle | Les réponses et suggestions restent dans votre métier |
| Fonction | Le service ou le domaine dans lequel vous travaillez |
| Outils utilisés | Les agents connaissent les outils de votre quotidien |
| Préférences | La langue, le ton et le format de réponse que vous préférez |
| Ce que vos agents doivent savoir | Tout le reste, avec vos mots |

Répondez naturellement, sans formulation précise. **Rien n'est écrit pendant la conversation.** Quand les questions sont finies, ou dès que vous cliquez sur **J'ai fini**, l'agent propose un profil et la conversation laisse place à un formulaire, **Votre profil, à relire**, rempli avec ce qu'il a compris.

Aucun champ n'est obligatoire. Corrigez ce qui ne va pas, puis choisissez :

- **Enregistrer** - le formulaire est écrit dans votre profil par défaut, et le parcours passe au premier agent.
- **Passer sans enregistrer** - rien n'est écrit, et le parcours passe au premier agent. Votre profil reste vide jusqu'à ce que vous le remplissiez depuis **Réglages → Profil**.
- **Retour** - revient à la conversation ; **Revoir la proposition** rouvre le formulaire.

<!-- claim:onboarding-skip-direct -->
Le bouton **Passer les questions** (dans le pied de la fenêtre) met fin à la conversation **immédiatement, sans tour d'IA supplémentaire** : le formulaire s'ouvre aussitôt, vide ou avec ce que l'agent avait déjà proposé, et vous le remplissez à la main ou vous passez. Le bouton **J'ai fini** dans le chat, lui, laisse l'agent proposer un profil à partir de ce que vous avez déjà dit.

Le premier lancement ne propose aucune règle de permission. Un consentement se donne depuis une demande d'approbation, et ce qu'un agent peut faire se lit et se retire sur l'écran **Autorisations** de la barre latérale.

### Étape 5 - Votre premier agent

Trois cartes, trois façons d'avoir un agent prêt à travailler quand la fenêtre se ferme :

- **Installer un agent** ouvre un paquet que vous avez reçu (un fichier `.apo`). Apollia montre ce qu'il contient, ce qu'il demande et qui l'a publié ; rien n'est installé avant votre confirmation.
- **Créer en décrivant mon besoin** ouvre un court formulaire : vous dites à quoi sert l'agent et, si vous le souhaitez, ce qu'il doit rendre, ce qu'il ne doit jamais faire et ce qu'il peut consulter. Apollia pose une question ou deux sur ce qui manque, chacune avec des réponses à choisir, puis rédige la fiche de l'agent ; vous la relisez et la corrigez, et **Enregistrer l'agent** le crée.
- **Partir d'un exemple** liste les agents prêts à l'emploi qu'Apollia propose, chacun avec sa mission et la connexion qu'il requiert. Choisissez-en un : seul celui-ci est installé, après la même confirmation que pour tout paquet.

Une fois l'agent là, l'étape le nomme et **Continuer** passe à la suite. **Passer** ferme la fenêtre sans agent, sur une conversation avec Apollia, et vous pouvez ajouter un agent à tout moment depuis l'écran **Agents**.

### Étape 6 - Une connexion utile

Cette étape n'apparaît que si l'agent retenu a besoin d'une connexion que vous n'avez pas encore. Un agent créé en décrivant votre besoin, ou un agent dont toutes les connexions sont là, la franchit sans s'y arrêter.

Chaque connexion manquante a sa ligne, avec la raison que l'agent donne pour en avoir besoin :

- **Ajouter** ouvre, au-dessus de la fenêtre, le même dialogue que l'écran **Connexions**, directement sur cette connexion. Une fois ajoutée, sa ligne indique **Présente**.
- Une connexion que la sélection d'Apollia ne propose pas n'a pas de bouton **Ajouter** : ajoutez-la plus tard depuis l'écran **Connexions**.

**Passer** et **Continuer** ferment tous deux la fenêtre, quoi que montrent les lignes : rien ici n'est obligatoire, et une connexion manquante s'ajoute à tout moment depuis l'écran **Connexions**.

### Étape 7 - Une première tâche

La dernière étape n'a pas d'écran : la fenêtre se ferme, la configuration est enregistrée comme faite, et Apollia s'ouvre sur votre agent au travail.

- **Avec l'agent retenu**, une conversation avec lui s'ouvre. Le premier exemple que l'agent déclare est déjà écrit dans la zone de message, et rien n'est envoyé : envoyez-le tel quel, ou modifiez-le d'abord. Un agent qui ne déclare aucun exemple s'ouvre sur une zone de message vide.
- **Avec l'exemple qui qualifie des demandes**, Apollia propose de copier ses données d'exemple, un classeur de demandes fictives et un petit dossier de documents de référence, dans un dossier que vous choisissez. Rien n'est copié avant votre confirmation. Une fois la copie faite, **Lancer le lot sur le classeur** ouvre le lancement d'un lot sur ce classeur, avec un aperçu de ses premières lignes. Si vous annulez la copie ou le lancement, la conversation avec l'agent s'ouvre à la place.
- **Sans agent**, une conversation avec Apollia s'ouvre.

### Sauter le parcours

Si vous préférez configurer Apollia plus tard, cliquez sur **Configurer plus tard** (en haut à droite de la fenêtre, disponible à toutes les étapes). Apollia s'ouvre normalement, mais sans modèle le chat ne pourra pas répondre : vous devrez télécharger un modèle ou ajouter un serveur depuis l'écran **IA** avant la première conversation.

Pour rouvrir le parcours après coup, voir la section [Relancer le parcours](#relancer-le-parcours) ci-dessous.

### Reprendre après une interruption

Le parcours est **résumable** : si vous quittez la fenêtre en cours de route (par exemple en fermant Apollia), au retour Apollia reprend exactement à l'étape où vous étiez. La progression est persistée côté backend, pas en mémoire de session.

## Relancer le parcours

Si vous voulez rejouer le parcours du premier lancement :

1. Ouvrez **Réglages → Général** et descendez jusqu'au bloc **Zone de danger**.
2. Cliquez sur **Réinitialiser l'onboarding**. Cette action efface uniquement les marqueurs de progression et de profil : vos modèles, vos serveurs et vos autres données restent intacts.
3. Une fenêtre de confirmation demande de taper `RESET`. Confirmez.
4. Le parcours en sept étapes redémarre depuis le choix de la langue au prochain lancement d'Apollia.

Pour de **petits ajustements** (corriger votre nom, ajouter un outil quotidien) sans repasser le parcours, ouvrez directement **Réglages → Profil** - voir [Mon profil](../memoire/manage-my-profile.md). Sur un profil dont des champs sont vides, **Compléter avec un agent** lance la même courte conversation pour ce profil ; sa proposition revient dans le formulaire, où **Enregistrer** l'écrit et **Passer** l'écarte.

:::warning Réinitialiser entièrement Apollia
Le bouton **Réinitialisation d'usine** (même bloc **Zone de danger**, sous **Dernier recours**) supprime **toutes** vos données : agents, mémoire, modèles téléchargés, serveurs, connexions. Cette action ne peut pas être annulée. Voir [Réinitialiser Apollia](../troubleshooting/factory-reset.md).
:::

## Vérification

Une fois le parcours terminé, ouvrez **Réglages → Profil** : le profil par défaut contient ce que vous avez enregistré, modifiable à tout moment. Si vous avez passé sans enregistrer, ses champs sont vides.

## Si ça ne marche pas

- **La fenêtre ne s'ouvre pas au premier lancement :** consultez les logs au démarrage. Si l'erreur évoque l'agent d'onboarding, redémarrez Apollia ; il est provisionné automatiquement à chaque lancement.
- **Le bouton Continuer reste désactivé à l'étape IA :** il s'ouvre dès qu'un modèle est configuré (badge vert **Configuré**), qu'un modèle est choisi sous **Utiliser un serveur**, ou qu'un téléchargement est lancé. Un téléchargement échoué ou annulé le referme.
- **Le téléchargement d'un modèle bloque à 0 % :** vérifiez votre connexion internet, annulez avec le bouton **X**, puis relancez. Le débit attendu est de 5 à 50 MB/s selon votre lien.
- **L'agent ne répond plus pendant la conversation :** fermez la fenêtre via **Configurer plus tard** et relancez avec **Réinitialiser l'onboarding**, dans le bloc **Zone de danger** de **Réglages → Général**.
- **Le formulaire s'ouvre vide :** l'agent n'a rien proposé, ou vous avez passé les questions. Remplissez les champs à la main, ou passez et revenez depuis **Réglages → Profil**.

> **Référence technique :** [Référence Apollia](/reference).
