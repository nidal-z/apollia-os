---
title: Connecter Microsoft 365
slug: /operator-help/integrations/connect-microsoft-365
sidebar_position: 4
---

# Connecter Microsoft 365

> Pour tout operator qui veut brancher Outlook, Calendar et OneDrive à Apollia, que son compte soit personnel (outlook.com, hotmail.com, live.com) ou professionnel (Microsoft 365, Entra ID).

## Prérequis

- Apollia lancé.
- Un compte Microsoft, personnel ou professionnel.
- La politique de votre organisation, si elle en a déployé une, n'interdit pas les connexions distantes.
- Si votre tenant Entra ID exige une approbation administrative, l'administrateur doit pré-approuver l'application.
- Connexion internet active.

Rien à inscrire, rien à coller. Si vous venez de lire la page Google, cette différence est voulue et expliquée dans [la vue d'ensemble des intégrations](/operator-help/integrations/integrations-overview).

## Rien à configurer

<!-- claim:oauth-microsoft-client-embedded -->
Microsoft 365 fonctionne dès l'installation d'Apollia. Apollia inscrit une application auprès de Microsoft et en fournit l'identifiant dans le build, vous passez donc directement à la connexion de votre compte.

Cet identifiant n'est pas un secret qui fuiterait. Au sens Microsoft, une application de bureau est un *client public* : elle ne détient aucun mot de passe, prouve chaque requête par PKCE, et son identifiant d'application est un GUID public. N'importe qui peut le lire dans n'importe quelle copie de l'application, ce qui est précisément pourquoi le dispositif ne repose pas sur le fait de le cacher.

Vous pouvez malgré tout [utiliser votre propre inscription](#utiliser-votre-propre-inscription-dapplication), et aller directement aux étapes ci-dessous est le chemin normal.

## Quel type de compte je peux utiliser

Les deux passent par la même inscription. L'endpoint utilisé (`/common/`) accepte indifféremment :

- les comptes personnels Microsoft (outlook.com, hotmail.com, live.com),
- les comptes professionnels ou d'éducation (Entra ID, M365 Business, M365 Developer tenant).

Voir les différences observables dans le tableau plus bas.

## Étapes

1. Dans la barre latérale, ouvrez **Connexions**, cliquez sur **Ajouter une connexion**, et choisissez **Microsoft 365** sous **Proposés par Apollia**. Sa carte dit ce qu'elle demande : une connexion à votre compte, dans votre navigateur. Cliquez sur **Ajouter**.

2. À l'étape **Brancher**, cliquez sur **Se connecter**. Une fenêtre s'ouvre dans Apollia et votre navigateur ouvre la page de consentement Microsoft. Avec un compte professionnel, l'étape vous prévient que votre organisation peut demander qu'un administrateur approuve d'abord Apollia, et qu'Apollia vous écrit alors la demande à lui envoyer.

3. Authentifiez-vous avec votre compte Microsoft, puis acceptez les autorisations (Mail, Calendar, Files).

   À l'écran : la page de consentement Microsoft, avec la liste des accès demandés (Mail, Calendar, Files) et les boutons Non et Oui.

4. De retour dans Apollia, la fenêtre détecte le retour automatiquement. À **Qui peut s'en servir**, cochez les agents qui peuvent se servir du compte ; **Apollia, dans le Chat** vient coché. Cliquez sur **Continuer**.

5. **Essai** confirme que Microsoft 365 est ajouté, avec **Essayer** et **Terminé**. Le compte apparaît dans la liste **Connexions**, sur sa propre ligne, avec son adresse.

   À l'écran : la fiche du compte Microsoft dans Connexions, avec son état, ses agents et ses choix OneDrive.

## Ce que vous pouvez faire

**Outlook (mail)** :
- Lectures : chercher des messages, lire un mail précis, lister vos dossiers. Lire votre compte, c'est lire des données personnelles : Apollia demande donc votre accord, et **Toujours pour cet agent** fait cesser la question pour cet agent.
- Écritures et envois, avec votre accord : envoyer un mail, répondre, déplacer un message.

**Calendar** :
- Lectures, selon la même règle : lister des événements, ouvrir un événement précis.
- Écritures, avec votre accord : créer ou modifier un événement.
- Suppression, avec votre accord comme toute écriture : supprimer un événement. Il n'y a aucune phrase à taper.

**OneDrive (lecture seule)** :
- Chercher dans vos fichiers, lire les métadonnées, télécharger un fichier, lister les fichiers récents.
- Seulement dans les dossiers et fichiers que vous choisissez. Sur la fiche du compte, sous **Fichiers OneDrive qu'Apollia peut lire**, **Choisir des dossiers et fichiers** permet de parcourir votre OneDrive et de les retenir ; tant que rien n'est choisi, vos agents n'y lisent rien, et un agent qui essaie reçoit la consigne de vous demander de choisir. Un dossier choisi ouvre tout ce qu'il contient, et **Retirer** le referme. La racine du lecteur elle-même ne peut pas être choisie.
- L'autorisation que Microsoft accorde couvre tout votre OneDrive. La limite est tenue par Apollia, sur votre ordinateur, qui vérifie chaque élément avant qu'un agent le voie.
- Il n'y a ni écriture OneDrive, ni dossier de travail par agent comme le `Drive/Apollia/<agent>/` de Google.

Microsoft Teams n'est pas couvert.

## Multi-comptes

Comme pour Google, vous pouvez connecter plusieurs comptes Microsoft, et chacun garde son propre token. Sur la fiche d'un compte, **Ajouter un autre compte** connecte le suivant. Chaque compte connecté est une connexion à part entière, sur sa propre ligne de la liste. Ses outils se donnent à un agent compte par compte, sur la fiche du compte avec **Donner à un agent** ou sur la fiche de l'agent, et un appel s'exécute au nom du compte donné et d'aucun autre.

## Différences perso vs pro

| Capacité | Compte personnel | Compte professionnel |
|---|---|---|
| Backend Mail | Outlook.com | Exchange Online |
| Backend Calendar | Outlook.com | Exchange Online |
| Backend Drive | OneDrive Personal | OneDrive for Business |
| Consentement admin | Sans objet | Possible selon politique du tenant |
| Domaines | outlook.com, hotmail.com, live.com | `<vous>@<entreprise>.onmicrosoft.com` ou domaine custom |

Les outils sont les mêmes des deux côtés, seul le backend qui répond change.

## Utiliser votre propre inscription d'application

Optionnel. L'inscription d'Apollia couvre les deux types de comptes, la plupart des operators n'en ont donc jamais besoin. Deux situations le justifient : votre organisation veut que la connexion apparaisse sous une application qu'elle contrôle et peut auditer, ou votre tenant Entra ID bloque les applications qu'il n'a pas inscrites lui-même.

**Dans le centre d'administration Microsoft Entra** (ou le portail Azure, « Inscriptions d'applications ») :

1. Choisissez **Nouvelle inscription**.
2. Pour les types de comptes pris en charge, choisissez **Comptes dans un annuaire organisationnel quelconque et comptes Microsoft personnels**, ce qui rend utilisables aussi bien une adresse `outlook.com` qu'un compte professionnel. Choisir une option mono-tenant limiterait la connexion à votre seul annuaire.
3. Sous **URI de redirection**, ajoutez une plateforme de type **Applications mobiles et de bureau** et saisissez `http://127.0.0.1`. Apollia écoute sur un port de loopback choisi au moment de la connexion, et Microsoft accepte n'importe quel port sur cet hôte.
4. Inscrivez, puis copiez l'**ID d'application (client)** depuis la page de vue d'ensemble. Il ressemble à `00000000-1111-2222-3333-444444444444`.

**Dans Apollia.**

1. Activez le mode développeur. Dans **Connexions**, ouvrez la fiche d'un compte Microsoft : sa section **Détail technique** contient le **Client Microsoft**. Sans compte connecté, utilisez plutôt la variable d'environnement décrite plus bas.
2. Collez l'identifiant dans le champ **Client ID** et enregistrez. Il est rangé dans un fichier de configuration de votre poste, lisible par votre seul utilisateur. Il n'y a aucun secret à saisir, un client public n'en a pas.
3. Cliquez sur **Tester le client**.

Vider à nouveau le champ rétablit l'identifiant fourni avec Apollia. Les comptes déjà connectés l'ont été auprès de l'inscription précédente : retirez-les et connectez-les à nouveau après un changement.

**Alternative pour un shell ou une machine sans interface.** `APOLLIA_MICROSOFT_CLIENT_ID` prime sur ce qui a été saisi sur la fiche, et ne vaut que pour les processus lancés depuis le shell où vous l'avez exportée. Voir [Environment variables](/reference/environment-variables).

## Vérification

- Le compte a sa ligne dans **Connexions**, et sa fiche indique **Fonctionne** une fois vérifiée ou utilisée.
- Sur la fiche, cliquez sur **Essayer** et demandez *« Liste mes 3 derniers mails Outlook »*. Apollia répond avec les outils de cette connexion, et demande votre accord avant de lire le compte.
- Tentez ensuite *« Envoie un mail à <votre adresse> avec le sujet test »*. Une demande d'accord s'affiche avant l'envoi.
- Le trousseau du système contient une entrée `apollia-connector-microsoft` associée à votre adresse.

## Si ça ne marche pas

- **La fiche Microsoft 365 affiche « Configuration requise avant de connecter »** : c'est impossible sur une version qui embarque l'identifiant, et ni une variable d'environnement vide ni un champ **Client ID** vidé ne le produisent : les deux sont ignorées lorsqu'elles sont vides et l'identifiant fourni reprend la main. Si vous le voyez malgré tout, c'est que la version installée a été compilée sans l'identifiant. Vérifiez avec `apollia-os connector client-id list`, qui nomme la source du client résolu, `env`, `file`, `builtin` ou `none`. Le simple `connector list` affiche les connecteurs enregistrés, leurs services et leur nombre d'opérations, et ne dit rien de l'identifiant client.
- **Microsoft rejette l'URI de redirection** : atteignable uniquement avec votre propre inscription. Elle n'a pas sa plateforme **Applications mobiles et de bureau**, ou celle-ci ne liste pas `http://127.0.0.1`. Une inscription créée en « Web » ne fonctionnera pas. Videz le champ identifiant client pour retomber sur l'inscription d'Apollia.
- **Microsoft répond que le compte n'existe pas dans l'annuaire** : également propre à votre inscription, cela signifie que les types de comptes pris en charge ont été réglés sur un tenant unique. Recréez-la avec **Comptes dans un annuaire organisationnel quelconque et comptes Microsoft personnels**, ou videz le champ pour utiliser celle d'Apollia.
- **Bloquée par votre organisation** : selon ses réglages de consentement, une organisation peut exiger qu'un administrateur approuve Apollia avant qu'un compte professionnel lise son courrier et son agenda. Quand Microsoft le dit, la fenêtre affiche **Bloquée par votre organisation**, avec l'adresse d'accord (**Copier l'adresse**) et une demande écrite pour votre administrateur, prête à envoyer (**Copier la demande**). Microsoft 365 reste ensuite dans la liste **Connexions** comme **Bloquée par l'organisation**, *un administrateur doit d'abord l'approuver*. Envoyez l'adresse ou la demande à votre administrateur : il ouvre l'adresse, se connecte et approuve Apollia pour toute l'organisation. Connectez-vous ensuite à nouveau depuis l'entrée **Microsoft 365** d'**Ajouter une connexion** ; le blocage se lève dès qu'un compte se connecte. Si vous avez refusé le consentement avec un compte professionnel, Apollia propose aussi cette adresse, sans marquer la connexion bloquée : Microsoft ne distingue pas un refus d'un accord d'administrateur non demandé. Un compte personnel (outlook.com, hotmail.com, live.com) donne son accord lui-même et ne voit jamais ce cas. Apollia ne demande que vos propres fichiers (`Files.Read`), jamais tous les fichiers partagés avec vous, que les réglages de consentement réservent plus souvent à un administrateur.
- **`outlook.send` échoue sur un destinataire** : Microsoft Graph valide les destinataires plus strictement que Google. Apollia remonte l'erreur de Graph telle quelle, préfixée du statut HTTP. Vérifiez l'adresse cible et l'absence d'alias mort.
- **OneDrive en écriture refusé** : c'est attendu, OneDrive est en lecture seule.
- **La connexion s'arrête sur « Refusé par la politique de l'organisation »** : la politique de votre organisation interdit les connexions à des services en ligne sur ce poste, et rien n'a été envoyé à Microsoft. Seule la personne qui gère cette politique peut accorder une exception.

## Retirer un compte

Sur la fiche du compte, cliquez sur **Retirer**. La fiche liste ce qui part avec lui, comme sa connexion au compte et les outils que chaque agent tenait par lui, et ses choix OneDrive partent aussi. Confirmez avec **Retirer la connexion**. Le token est supprimé du coffre de secrets local. Pour révoquer également côté Microsoft, allez sur https://myaccount.microsoft.com et retirez l'autorisation de l'application Apollia. Voir [Mettre en pause ou retirer une connexion](pause-or-remove-a-connection.md).

> **Référence technique :** [Référence Apollia](/reference) , flow OAuth Microsoft, scopes complets, refresh proactif.
