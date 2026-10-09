---
title: Gérer mon profil
slug: /operator-help/memory/manage-my-profile
sidebar_position: 1
---

# Gérer mon profil

> Pour les operators qui veulent **consulter et modifier** au quotidien ce que leurs agents savent d'eux : nom, rôle, fonction, outils, préférences.
>
> Cette page couvre **l'édition d'un profil existant**. Si vous lancez Apollia pour la première fois, le parcours guidé peuple les valeurs initiales : voir **[Configurer votre profil au premier lancement](../installation/set-up-your-profile.md)**.

## Pourquoi un profil utilisateur

Un profil vous décrit une fois, pour tous les agents autorisés à le lire. Quand il dit que vous êtes développeur en fintech, ces agents adaptent leur vocabulaire, leur ton et leurs suggestions, sans ressaisie pour chacun.

Vous pouvez garder **plusieurs profils**, un pour chaque facette de votre travail. L'un d'eux est le profil **par défaut** : c'est celui que vos agents utilisent.

Un agent ne lit votre profil que si la source **Profil** est cochée sur sa fiche, sous **Sources consultées**. Un agent sans cette source n'en sait rien.

Les profils sont **locaux**. Aucune donnée ne quitte votre machine.

## Où l'éditer

Dans la barre latérale, cliquez sur **Réglages**, puis sur la section **Profil**.

La section contient trois blocs :

- **Profils** : la liste de vos profils, celui par défaut marqué **Par défaut**, et le bouton **Nouveau profil**.
- **Informations du profil** : le formulaire du profil sélectionné dans la liste. Six champs, tous facultatifs : **Nom**, **Rôle**, **Fonction**, **Outils utilisés**, **Préférences** et **Ce que vos agents doivent savoir**, un texte libre.
- **Ce que vos agents ont retenu** : les faits gardés dans ce profil, notés par vos agents ou ajoutés par vous.

## Étapes

1. Ouvrez **Réglages**, section **Profil**, et cliquez sur le profil à modifier dans la liste **Profils**.

2. Modifiez les champs voulus. Rien n'est enregistré pendant la saisie.

3. Cliquez sur **Enregistrer** en bas de la fenêtre (ou appuyez sur **Cmd+S** sous macOS, **Ctrl+S** sous Windows et Linux). **Abandonner** remet les champs comme ils étaient.

Sélectionner un autre profil enregistre d'abord les modifications en cours.

## Garder plusieurs profils

- **Nouveau profil** demande un nom et crée un profil vide.
- Le menu en bout de chaque ligne propose **Renommer**, **Définir par défaut**, **Réinitialiser…** et **Supprimer…**.
- Le profil par défaut ne peut pas être supprimé : choisissez d'abord un autre profil par défaut.
- Supprimer un profil supprime ses champs, son texte libre et ce que vos agents y ont retenu. Cette action ne peut pas être annulée.

Ces changements prennent effet aussitôt, sans **Enregistrer**.

## Les faits d'un profil

Sous le formulaire, **Ce que vos agents ont retenu** liste les faits du profil. Ils ne servent qu'à ce profil.

- **Ajouter un fait** en ajoute un à la main, par exemple *« Je facture mes clients le 5 du mois »*.
- Sur chaque fait, **Modifier** le corrige, **Copier la valeur** le copie et **Supprimer** le retire, après une confirmation.
- Une pastille sur chaque fait dit d'où il vient : **onboarding** (le parcours guidé du premier lancement), **vous**, ou **agent**.

Un fait est enregistré aussitôt, sans **Enregistrer**.

## Ce qu'un agent peut proposer

Un agent dont la source **Profil** est cochée peut proposer un fait durable vous concernant pour votre profil. Il n'en écrit jamais un de lui-même :

- La proposition vous arrive comme une demande d'accord, dans le chat et dans **À valider**, avec le fait affiché.
- **Une fois** accepte le fait, qui est alors ajouté à votre profil. **Refuser** l'abandonne.
- Chaque proposition est demandée, à chaque fois. Il n'y a pas de « toujours » pour elle.

Un agent ne peut pas proposer ainsi une valeur pour l'un des six champs du formulaire : ceux-là, c'est à vous de les remplir.

## Compléter un profil avec un agent

Quand un profil a un nom et un rôle mais garde des champs vides, un bandeau propose **Compléter avec un agent** : un agent vous pose quelques questions, puis remplit le formulaire avec sa proposition. Un second bandeau, *Proposition de l'agent, à relire*, coiffe alors le formulaire. Corrigez ce qui ne va pas, puis cliquez sur **Enregistrer** pour garder la proposition ou sur **Passer** pour l'abandonner. Rien n'est écrit avant **Enregistrer**.

## Réinitialiser votre profil

Plusieurs chemins, selon ce que vous voulez effacer.

### A - Vider un profil

Dans la liste **Profils**, ouvrez le menu du profil et choisissez **Réinitialiser…**. Les champs, le texte libre et ce que vos agents ont retenu dans ce profil sont vidés. Le profil reste dans la liste.

Pour le profil par défaut, la fenêtre propose deux boutons : **Réinitialiser seulement**, ou **Réinitialiser et refaire connaissance**, qui lance ensuite une courte conversation avec un agent, lequel vous propose un nouveau profil à relire.

L'historique des conversations et les mémoires de vos agents ne sont pas touchés.

### B - Repasser le parcours guidé

Dans **Réglages**, section **Général**, le bloc **Zone de danger** contient un bouton **Réinitialiser l'onboarding**. Il fait rejouer le parcours d'accueil du premier lancement.

Il n'est pas neutre pour votre profil : il vide aussi ce que votre profil contient, alors même que la fenêtre annonce qu'aucune donnée n'est supprimée. Préférez le chemin A quand vous voulez seulement changer un profil.

### C - Effacer toutes les mémoires (profils et agents)

Dans le même bloc **Zone de danger**, le bouton **Effacer les mémoires** supprime **toutes** les mémoires d'Apollia : votre profil et la mémoire de chaque agent. C'est plus large que la réinitialisation d'un profil. Utilisez-le si vous voulez repartir d'une ardoise entièrement vierge côté mémoire.

> Pour aller encore plus loin (effacer aussi les agents installés, les conversations, les modèles et les réglages), l'option est **Réinitialisation d'usine**, en bas du même bloc.

## Vérification

- Fermez les réglages et rouvrez-les : les valeurs enregistrées sont là.
- Démarrez une nouvelle conversation avec un agent dont la source **Profil** est cochée : il s'adapte à ce que dit le profil.
- Un fait que vous avez ajouté porte la pastille **vous**.

## Si ça ne marche pas

- **Les champs reviennent comme avant** : la modification n'a pas été enregistrée. Cliquez sur **Enregistrer** avant de quitter la section.
- **Supprimer… est refusé sur un profil** : c'est le profil par défaut. Définissez-en d'abord un autre par défaut.
- **Un agent ignore votre profil** : un agent ne lit votre profil que si la source **Profil** est cochée pour lui. Ouvrez **Agents**, puis la fiche de l'agent, et regardez sous **Sources consultées**. En ligne de commande, `apollia-os agent show <agent>` vous dit où il en est et `apollia-os agent set <agent> sources profile` la coche (listez toutes les sources à garder, la liste remplace celle du paquet).
- **Un agent ne propose jamais de fait** : sa source **Profil** n'est pas cochée, ou il n'a rien trouvé de durable à proposer.

> **Référence technique :** [Référence Apollia](/reference)
