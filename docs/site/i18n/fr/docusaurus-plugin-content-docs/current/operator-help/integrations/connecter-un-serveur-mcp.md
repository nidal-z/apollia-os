---
title: Connecter un serveur MCP depuis le catalogue
slug: /operator-help/integrations/connect-an-mcp-server
sidebar_position: 5
---

# Connecter un serveur MCP depuis le catalogue

> Pour tout opérateur qui veut ajouter un service du catalogue (Notion, Linear, Atlassian, Stripe, Heure, etc.) en quelques clics, et choisir quels agents peuvent s'en servir.

## Prérequis

- Apollia lancé, page **Connexions** accessible.
- Vous savez quel service vous voulez brancher. La sélection proposée par Apollia est listée dans la [Vue d'ensemble des intégrations](vue-d-ensemble-integrations.md) ; le catalogue public est à un clic de plus.
- Pour les services qui en demandent, votre clé ou votre compte chez le fournisseur.

## Trouver le service

1. Dans la barre latérale, ouvrez **Connexions**, puis cliquez sur **Ajouter une connexion**. Une fenêtre s'ouvre sur une seule recherche.
2. Tapez le nom du service. Les réponses viennent en quatre groupes :
   - **Proposés par Apollia** : les comptes Microsoft 365 et Google Workspace, puis les services sélectionnés par Apollia, chacun marqué **Officiel** ou **Communauté**.
   - **Sur cet ordinateur** : un dossier, une boîte mail.
   - **Catalogue public** : interrogé seulement quand vous cliquez sur **Explorer tout**. Tout le monde peut y publier et Apollia n'a pas vérifié ses entrées, donc chacune est marquée **Non vérifié**.
   - **Brancher un service par son adresse** : toujours en dernier, pour un service que vous joignez par son adresse. Voir [Câbler son propre serveur MCP](cabler-son-propre-serveur-mcp.md).
3. Cliquez sur le service. Sa carte dit **Ce qu'elle demande** : une connexion à votre compte dans votre navigateur, une clé, quelques réglages, un téléchargement sur cet ordinateur, ou que le service est joint par internet. Cliquez sur **Ajouter**. Un service déjà ajouté propose plutôt **Ouvrir sa fiche**.

## L'ajouter en trois étapes

Les étapes s'affichent en haut de la fenêtre : **Brancher**, **Qui peut s'en servir**, **Essai**. **Annuler** avant la fin de la première étape vous ramène à la recherche, et rien n'est ajouté.

### Brancher

Pour un service du catalogue, sa carte montrait déjà **Avant d'ajouter cette connexion**, au-dessus de **Ajouter** : vos agents pourront utiliser ce que propose le service, une action qui envoie, modifie ou supprime quelque chose vous demande d'abord votre accord, ce qu'ils lisent ou envoient passe par le service selon ses propres conditions, et vous pouvez retirer la connexion à tout moment. Cliquer sur **Ajouter** a donné votre accord, et il est inscrit au **Journal**.

Apollia ne demande ensuite que ce dont le service a besoin :

- **Rien** : le service est ajouté tout de suite.
- **Une clé** : collez-la dans le champ. Elle est rangée dans le coffre de secrets de cet ordinateur, jamais dans les réglages de la connexion.
- **Une connexion à un compte** : cliquez sur le bouton de connexion, votre navigateur ouvre la page de consentement du fournisseur, autorisez, et le retour est automatique. Quand le service publie ses permissions, **Ce que la connexion accordera** les liste avant que vous vous connectiez. Choisir celles à accorder est réservé au mode développeur.

Un composant dont le service a besoin est d'abord téléchargé si nécessaire, et son intégrité est vérifiée avant son installation.

### Qui peut s'en servir

**Quels agents peuvent se servir de cette connexion ?** **Apollia, dans le Chat** vient en premier et coché ; les autres agents de cet ordinateur sont listés, à cocher. Les agents cochés reçoivent tous les outils de la connexion. Un outil qui écrit, envoie ou exécute du code, et un outil que personne n'a classé, demandent toujours votre accord avant de s'exécuter. Cliquez sur **Continuer**.

Vous pourrez le changer ensuite, sur la fiche de la connexion avec **Donner à un agent**, ou sur la fiche de chaque agent.

### Essai

Apollia essaie la connexion aussitôt et montre le résultat :

- **Connexion ajoutée et fonctionnelle**, avec le nombre d'outils disponibles.
- **La connexion est ajoutée, mais son essai a échoué**, avec un message clair. La connexion reste ajoutée : **Réessayer** essaie à nouveau, **Retirer** l'enlève.

Un refus dit qui a refusé, la politique de votre organisation par exemple.

Sous le résultat, **Que pouvez-vous faire maintenant ?** propose quelques exemples, chacun avec un bouton **Essayer** qui ouvre une conversation avec Apollia limitée aux outils de cette connexion.

## Vérification

- La connexion figure dans la liste **Connexions**. Son état indique **Fonctionne**, ou **Pas encore vérifiée** jusqu'à son premier usage ou sa première vérification.
- Sa fiche liste les outils du service, chacun avec sa famille, et sous **Agents qui s'en servent**, les agents que vous avez cochés.
- Cliquez sur **Essayer** sur la fiche et demandez quelque chose que le service sait faire. L'outil correspondant est appelé.

> **Note :** un service démarre à sa première utilisation et s'arrête après un temps sans usage. Ses outils restent listés entre les deux. Voir [Configurer le chargement différé des outils MCP](configure-deferred-mcp-loading.md).

## En cas de problème

- **L'essai échoue avec « Authentification refusée »** : la clé est invalide ou révoquée. Cliquez sur **Retirer**, puis ajoutez le service à nouveau et collez la clé sans espace parasite.
- **L'essai échoue avec « Service introuvable »** : le service a répondu que rien ne se trouve à l'adresse donnée. Vérifiez l'adresse ou le nom du service.
- **Le service n'expose aucun outil** : il démarre mais ne déclare rien. Lancez [une vérification](test-an-mcp-connection.md), puis regardez les journaux côté fournisseur.
- **Le service voulu n'est pas dans le catalogue** : voir [Câbler son propre serveur MCP](cabler-son-propre-serveur-mcp.md).
- **Un agent dit qu'il n'a pas accès à l'outil** : il n'a pas été coché sous **Qui peut s'en servir**. Sur la fiche de la connexion, cliquez sur **Donner à un agent**. Voir [Comprendre la portée d'une intégration](understand-integration-scope.md).

### Un service demande Node.js {#nodejs}

Certains services (le navigateur web, les fichiers d'un dossier, et d'autres) tournent sur Node.js. Apollia télécharge le sien la première fois que l'un d'eux est ajouté. Quand il ne le peut pas (pas de réseau, ou son téléchargement n'est pas encore disponible), il utilise le Node.js déjà installé sur votre ordinateur, si sa version est 22 ou plus.

- **L'ajout dit que le service a besoin de Node.js** : aucun Node.js utilisable n'a été trouvé. Installez Node.js 22 ou plus depuis [nodejs.org](https://nodejs.org) (la version LTS), puis ajoutez le service à nouveau. Vous pouvez aussi attendre qu'Apollia puisse télécharger le sien.
- **Le message cite un Node.js trop ancien** : celui de votre ordinateur est trop ancien. Mettez-le à jour en version 22 ou plus.
- **L'essai dit qu'il a utilisé le Node.js installé sur cet ordinateur** : Apollia n'a pas pu télécharger le sien et s'est servi du vôtre. Rien à faire ; Apollia repasse sur le sien dès qu'il peut le télécharger.

> **Référence technique :** [Référence Apollia](/reference) , protocole MCP, transports, niveaux de confiance, gouvernance.
