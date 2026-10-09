---
title: Connecter un serveur MCP depuis le catalogue
slug: /operator-help/integrations/connect-an-mcp-server
sidebar_position: 5
---

# Connecter un serveur MCP depuis le catalogue

> Pour tout operator qui veut activer un serveur MCP du catalogue (Notion, Linear, Atlassian, Stripe, Heure, etc.) en quelques clics.

## Prérequis

- Apollia lancé, page **Connexions** accessible.
- Vous savez quel service vous voulez brancher. Le catalogue s'ouvre sur la sélection qu'Apollia fournit, listée dans la [Vue d'ensemble des intégrations](vue-d-ensemble-integrations.md) ; le registre MCP public est à un clic de plus.
- Pour les services authentifiés, vos identifiants (clé API ou compte OAuth chez le fournisseur).

## Étapes

1. Dans la barre latérale, ouvrez **Connexions**, puis cliquez sur **Ajouter une connexion** en haut de la page. Une feuille s'ouvre sur son onglet **Catalogue des connexions**.

   ![La feuille qui ajoute une connexion, ouverte sur son onglet catalogue avec sa liste d'entrées](/img/operator-help/integration-connecter-un-serveur-mcp-1.png)

2. Cherchez l'entrée souhaitée, ou filtrez la liste avec **Officiels** et **Communauté**, puis cliquez dessus. Si elle n'est pas dans la sélection, cliquez sur **Explorer tout** pour charger le catalogue public. Une fenêtre s'ouvre : deux gestes vous reviennent, choisir l'entrée et accepter un avertissement, et le reste se fait tout seul.

### L'avertissement

La fenêtre s'ouvre sur **Avant d'ajouter cette connexion** : vos agents pourront utiliser ce que propose le service, une action qui envoie, modifie ou supprime quelque chose vous demande d'abord votre accord, ce qu'ils lisent ou envoient passe par le service selon ses propres conditions, et vous pouvez retirer la connexion à tout moment. Il n'y a rien à cocher. Cliquez sur **Ajouter** ; votre accord est inscrit au **Journal**.

À l'écran : l'avertissement, avec les boutons Annuler et Ajouter.

### L'authentification, seulement quand la connexion en a besoin

Apollia détecte ce que la connexion exige. Trois cas possibles :

- **Aucune authentification** : cette étape n'apparaît pas, la connexion est ajoutée directement.
- **Clé API ou jeton statique** : un champ apparaît pour chaque valeur, masqué quand c'est un secret. Collez votre clé, puis cliquez sur **Ajouter**.
- **OAuth** : un bouton **Se connecter à [Provider]** apparaît, avec les **Permissions demandées** quand le serveur les publie ; décochez celles que vous ne voulez pas accorder. Cliquez sur le bouton, votre navigateur ouvre la page de consentement, autorisez, le retour est automatique. Cliquez ensuite sur **Ajouter**.

À l'écran : l'étape d'authentification dans le cas OAuth, avec le bouton Se connecter à [Provider] et la liste des permissions demandées au-dessus.

### L'essai, tout seul

Il n'y a pas de bouton de test à cliquer. Une fois ajoutée, la connexion est essayée aussitôt, et un composant dont elle a besoin est d'abord téléchargé si nécessaire. La fenêtre affiche le résultat :

- **Connexion ajoutée et fonctionnelle**, en vert, avec le nombre d'outils disponibles.
- **La connexion est ajoutée, mais son essai a échoué**, en rouge, avec un message précis (clé invalide, adresse injoignable, etc.). La connexion reste ajoutée : cliquez sur **Réessayer** pour l'essayer de nouveau, ou sur **Retirer** pour l'enlever.

Un refus dit qui a refusé : la politique de votre organisation, par exemple.

À l'écran : le résultat de l'essai, en vert, avec le nombre d'outils disponibles.

### Des exemples à essayer

Sous le résultat, **Que pouvez-vous faire maintenant ?** affiche quelques cartes d'exemples, chacune avec un bouton **Essayer** qui ouvre une conversation avec Apollia limitée aux outils de cette connexion. Cliquez sur **Fermer** pour quitter la fenêtre.

À l'écran : les cartes d'exemples, portant chacune un bouton Essayer, et le bouton Fermer.

## Vérification

- La connexion apparaît dans la liste de **Connexions** avec un badge **actif**.
- Sa fiche liste, dans l'onglet **Outils**, les outils déclarés par le serveur, avec leur description.
- Cliquez sur **Essayer** sur l'une des cartes d'exemples. L'outil correspondant est appelé.

> **Note - chargement différé :** par défaut, `[mcp] tool_loading = "deferred"`. Les schémas d'outils du serveur ne sont pas tous placés devant le modèle au démarrage : quand les outils accordés à un agent sont au plus `tool_search_limit`, ils sont tous déclarés, au-dessus l'agent appelle `tool_search` pour trouver ce qu'il lui faut. Le nombre d'outils affiché dans l'UI reste complet dans les deux cas. Voir [Configurer le chargement différé des outils MCP](configure-deferred-mcp-loading.md).

## Si ça ne marche pas

- **L'essai échoue avec « Authentification refusée »** : votre clé ou token est invalide ou révoqué. Cliquez sur **Retirer**, puis ajoutez de nouveau la connexion en collant la valeur sans espaces parasites.
- **L'essai échoue avec « Service introuvable »** : le serveur n'est pas joignable. Vérifiez votre connexion ou le statut du fournisseur, puis cliquez sur **Réessayer**.
- **Le serveur ajouté n'expose aucun outil** : le serveur démarre mais ne déclare rien. Voir [Tester une connexion MCP](test-an-mcp-connection.md) pour relancer le test, puis vérifier les logs côté fournisseur.
- **Vous voulez brancher un serveur qui n'est pas dans le catalogue** : voir [Câbler son propre serveur MCP](cabler-son-propre-serveur-mcp.md).
- **L'agent ne trouve pas l'outil en mode deferred** : `tool_search` ne cherche que parmi les outils accordés à l'agent. Accordez l'outil sur la fiche de l'agent, comme le décrit la ligne suivante.
- **L'agent dit qu'il n'a pas accès à l'outil** : une connexion ajoutée ici n'est pas accordée à tous les agents. Ouvrez la fiche de l'agent, cliquez sur **Ajuster**, et sous **Accès** utilisez **Accorder une connexion**. Voir [Comprendre la portée d'une intégration](understand-integration-scope.md).

> **Référence technique :** [Référence Apollia](/reference) , protocole MCP, transports, trust levels, gouvernance.
