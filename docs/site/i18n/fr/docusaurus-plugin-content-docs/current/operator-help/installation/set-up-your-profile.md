---
title: Configurer votre profil au premier lancement
slug: /operator-help/installation/set-up-your-profile
sidebar_position: 4
---

# Configurer votre profil au premier lancement

> Au premier démarrage d'Apollia (ou après une réinitialisation), un parcours de configuration s'ouvre automatiquement.
> Il vous fait passer en trois étapes - accueil, configuration de l'IA, une courte conversation qui propose votre profil - pour qu'Apollia soit prêt à l'emploi. Il ne vous demande jamais de choisir un profil d'usage : l'interface est la même pour tous, et les écrans techniques supplémentaires relèvent du mode développeur dans les Paramètres, éteint par défaut.
> Durée totale : 3 à 8 minutes selon le téléchargement des modèles.
>
> Cette page couvre **le parcours initial uniquement**. Pour éditer votre profil au quotidien depuis l'application installée, voir **[Mon profil](../memoire/manage-my-profile.md)**.

## Prérequis

- Apollia est installé et lancé pour la première fois (ou votre profil a été réinitialisé).
- Connexion internet active pendant l'étape **Modèles** si vous téléchargez un modèle local ou utilisez un fournisseur cloud.

## Vue d'ensemble du parcours

Le parcours est rythmé par un rail de progression visible en haut de la fenêtre : **Accueil · Modèles · Qui je suis**. Vous pouvez revenir en arrière à toute étape, et **Configurer plus tard** est disponible en bas pour ajourner l'ensemble.

| Étape | Ce qui s'y passe |
|---|---|
| **Accueil** | Présentation d'Apollia et lancement du parcours. |
| **Modèles** | Configuration du LLM (local GGUF avec téléchargement embarqué, ou fournisseur cloud) et, en option, du modèle de dictée vocale Whisper. |
| **Qui je suis** | Une courte conversation avec un agent, dont chaque question peut être passée. L'agent propose ensuite un profil, que vous relisez et corrigez avant qu'il soit enregistré. |

## Étapes détaillées

### Étape 1 - Accueil

Au lancement, Apollia détecte qu'aucun profil n'existe et ouvre la fenêtre de configuration centrée sur l'écran.

![Fenêtre d'accueil avec logo Apollia, sous-titre "Le runtime souverain pour tes agents IA en local", trois cartes Local-first / LLM au choix / Agents autonomes, bouton "Commencer la configuration"](/img/operator-help/installation-configurer-votre-profil-1.png)

Lisez le bandeau de bienvenue, puis cliquez sur **Commencer la configuration**.

### Étape 2 - Configurer le moteur d'IA

Cette étape calibre Apollia à votre matériel et à vos préférences de souveraineté. Un bandeau d'informations système (RAM, OS, GPU) apparaît en haut pour vous aider à choisir.

![Étape Modèles, bandeau RAM · macOS · GPU, section LLM avec liste curée de modèles Qwen3 et badge "Recommandé"](/img/operator-help/installation-configurer-votre-profil-3.png)

#### Section LLM (obligatoire)

Quatre chemins possibles, selon votre situation :

1. **Modèles déjà présents sur la machine.** Si vous avez déjà placé un fichier `.gguf` dans `~/.apollia/models/` ou `~/Downloads/`, Apollia le détecte automatiquement. Cliquez sur la ligne correspondante pour le configurer en un clic.
2. **Modèles recommandés à télécharger.** Apollia affiche une liste curée de modèles Qwen3 (4B, 8B, 14B, 30B-A3B) filtrée selon votre RAM, avec un badge **Recommandé** sur le plus pertinent. La liste reste proposée qu'un GGUF ait déjà été trouvé ou non, et après la configuration d'un premier modèle. Cliquez sur la ligne pour lancer le téléchargement - une barre de progression avec débit en MB/s s'affiche, vous pouvez l'annuler à tout moment.
3. **Recherche HuggingFace.** Le bouton **Rechercher sur HuggingFace** ouvre un mini-navigateur intégré : tapez le nom d'un modèle, dépliez les fichiers GGUF disponibles, et cliquez pour télécharger celui qui correspond à votre RAM (les fichiers sont étiquetés *fits* / *might fit* / *too large*).
4. **Fournisseur cloud.** Le bouton **Utiliser un fournisseur cloud** ferme la fenêtre et vous emmène vers la page **Backends LLM** des paramètres pour brancher Anthropic, OpenAI ou Ollama. Une fois un backend ajouté, le parcours d'onboarding se rouvre automatiquement à cette même étape.

![Téléchargement Qwen3 en cours avec barre de progression et débit, plus modèle Whisper téléchargé en parallèle](/img/operator-help/installation-configurer-votre-profil-4.png)

Une fois la configuration LLM réussie (badge vert **Configuré** ou liste de backends détectée), le bouton **Continuer** en bas devient actif.

#### Section Reconnaissance vocale (optionnel)

L'interrupteur **Reconnaissance vocale** active la dictée. Si aucun modèle Whisper n'est présent, vous pouvez en télécharger un depuis la liste curée :

| Modèle | Taille | Idéal pour |
|---|---|---|
| Whisper Tiny | 75 MB | Tests rapides, machines limitées |
| Whisper Base | 142 MB | Usage courant équilibré |
| Whisper Large-v3 Turbo Q5 | 547 MB | **Recommandé** - haute qualité, 6× plus rapide que Large-v3 |
| Whisper Large-v3 Q5 | 1.1 GB | Précision maximale, multi-langues |
| Whisper Large-v3 French | 1.1 GB | Fine-tuné spécifiquement pour le français |

Vous pouvez sauter cette section et l'activer plus tard depuis **Paramètres → Reconnaissance vocale**.

#### Boutons disponibles en pied de page

- **← Retour** - revient à l'étape Accueil.
- **Configurer plus tard** - saute directement à la conversation (uniquement si un LLM est déjà disponible).
- **Continuer** - désactivé tant qu'aucun LLM n'est utilisable, devient actif dès qu'un backend cloud ou local est configuré.

### Étape 3 - Une courte conversation, puis votre profil à relire

L'agent d'onboarding s'ouvre dans la fenêtre. Il pose quelques questions, un sujet à la fois, et chacune peut être passée.

![Étape Qui je suis avec les pips de progression en haut, l'agent d'onboarding pose sa première question et l'utilisateur répond en présentant son rôle](/img/operator-help/installation-configurer-votre-profil-5.png)

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

- **Enregistrer** - le formulaire est écrit dans votre profil par défaut, et la fenêtre se ferme.
- **Passer sans enregistrer** - rien n'est écrit, et la fenêtre se ferme. Votre profil reste vide jusqu'à ce que vous le remplissiez depuis **Paramètres → Profil**.
- **Retour** - revient à la conversation ; **Revoir la proposition** rouvre le formulaire.

<!-- claim:onboarding-skip-direct -->
Le bouton **Passer les questions** (dans le pied de la fenêtre) met fin à la conversation **immédiatement, sans tour d'IA supplémentaire** : le formulaire s'ouvre aussitôt, vide ou avec ce que l'agent avait déjà proposé, et vous le remplissez à la main ou vous passez. Le bouton **J'ai fini** dans le chat, lui, laisse l'agent proposer un profil à partir de ce que vous avez déjà dit.

L'onboarding ne propose plus de règles de permissions. Les règles se créent depuis les demandes d'approbation, ou à la main dans **Paramètres → Permissions**.

### Sauter le parcours

Si vous préférez configurer Apollia plus tard, cliquez sur **Configurer plus tard** (en bas de la fenêtre, disponible à toutes les étapes). Apollia s'ouvre normalement, mais sans LLM configuré le chat ne pourra pas répondre - vous devrez brancher un fournisseur depuis **Paramètres → Backends LLM** avant la première conversation.

Pour rouvrir le parcours après coup, voir la section [Relancer le parcours](#relancer-le-parcours) ci-dessous.

### Reprendre après une interruption

Le parcours est **résumable** : si vous quittez la fenêtre en cours de route (par exemple en cliquant sur **Utiliser un fournisseur cloud** pour ajouter un backend Anthropic), au retour Apollia reprend exactement à l'étape où vous étiez. La progression est persistée côté backend, pas en mémoire de session.

## Relancer le parcours

Si vous voulez rejouer le parcours du premier lancement :

1. Ouvrez **Paramètres → Zone de danger**.
2. Cliquez sur **Réinitialiser l'onboarding**. Cette action efface uniquement les marqueurs de progression et de profil - vos backends LLM, modèles téléchargés et autres données restent intacts.
3. Une fenêtre de confirmation demande de taper `RESET`. Confirmez.
4. Le parcours en trois étapes redémarre depuis l'Accueil.

Pour de **petits ajustements** (corriger votre nom, ajouter un outil quotidien) sans repasser le parcours, ouvrez directement **Paramètres → Profil** - voir [Mon profil](../memoire/manage-my-profile.md). Sur un profil dont des champs sont vides, **Compléter avec un agent** lance la même courte conversation pour ce profil ; sa proposition revient dans le formulaire, où **Enregistrer** l'écrit et **Passer** l'écarte.

:::warning Réinitialiser entièrement Apollia
Le bouton **Réinitialisation d'usine** (zone danger, bas de page) supprime **toutes** vos données - agents, mémoire, modèles téléchargés, backends LLM, intégrations. Cette action ne peut pas être annulée. Voir [Réinitialiser Apollia](../troubleshooting/factory-reset.md).
:::

## Vérification

Une fois le parcours terminé, ouvrez **Paramètres → Profil** : le profil par défaut contient ce que vous avez enregistré, modifiable à tout moment. Si vous avez passé sans enregistrer, ses champs sont vides.

![Page Paramètres → Profil après onboarding, sections Identité (prénom, rôle, secteur), Objectifs, et Supervision des agents avec les niveaux HITL](/img/operator-help/installation-configurer-votre-profil-7.png)

## Si ça ne marche pas

- **La fenêtre ne s'ouvre pas au premier lancement :** consultez les logs au démarrage. Si l'erreur évoque l'agent d'onboarding, redémarrez Apollia ; il est provisionné automatiquement à chaque lancement.
- **Le bouton Continuer reste désactivé en étape Modèles :** vérifiez qu'au moins un backend LLM est listé. Le téléchargement d'un GGUF doit être achevé (pastille verte **Configuré**), ou un backend cloud doit être ajouté via **Utiliser un fournisseur cloud**.
- **Le téléchargement d'un modèle bloque à 0 % :** vérifiez votre connexion internet, annulez avec le bouton **X**, puis relancez. Le débit attendu est de 5 à 50 MB/s selon votre lien.
- **L'agent ne répond plus pendant la conversation :** fermez la fenêtre via **Configurer plus tard** et relancez depuis **Paramètres → Zone de danger → Réinitialiser l'onboarding**.
- **Le formulaire s'ouvre vide :** l'agent n'a rien proposé, ou vous avez passé les questions. Remplissez les champs à la main, ou passez et revenez depuis **Paramètres → Profil**.

> **Référence technique :** [Référence Apollia](/reference) - spec complète des 4 étapes, persistence backend, commandes IPC, événements runtime.
