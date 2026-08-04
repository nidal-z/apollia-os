# Connecter Microsoft 365

> Pour tout operator qui veut brancher Outlook, Calendar et OneDrive à Apollia, que son compte soit personnel (outlook.com, hotmail.com, live.com) ou professionnel (Microsoft 365, Entra ID).

## Prérequis

- Apollia lancé.
- Un compte Microsoft, personnel ou professionnel.
- Votre profil de souveraineté n'est pas réglé sur `local_only`.
- Si votre tenant Entra ID exige une approbation administrative, l'administrateur doit pré-approuver l'application.
- Connexion internet active.

Rien à inscrire, rien à coller. Si vous venez de lire la page Google, cette différence est voulue et expliquée dans [la vue d'ensemble des intégrations](/operator-help/integrations/vue-d-ensemble-integrations).

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

1. Dans la sidebar, ouvrez **Connexions**, puis sélectionnez la carte **Microsoft 365**.

   A l'ecran : la page Connexions, avec la carte Microsoft 365 mise en évidence et le bouton Connecter un compte dans le panneau de droite.

2. Cliquez sur **Connecter un compte**. Une fenêtre s'ouvre dans Apollia et votre navigateur ouvre la page de consentement Microsoft.

3. Authentifiez-vous avec votre compte Microsoft, puis acceptez les autorisations (Mail, Calendar, Files).

   A l'ecran : la page de consentement Microsoft, avec la liste des accès demandés (Mail, Calendar, Files) et les boutons Non et Oui.

4. De retour dans Apollia, la fenêtre détecte le retour automatiquement et se ferme. Votre compte apparaît dans la sidebar avec une pastille verte.

   A l'ecran : la sidebar Connexions, avec la carte Microsoft 365 dépliée affichant le compte connecté et sa pastille verte.

## Ce que vous pouvez faire

**Outlook (mail)** :
- Lecture automatique : chercher des messages, lire un mail précis, lister vos dossiers.
- Écriture avec approbation HITL : envoyer un mail, répondre, déplacer un message.

**Calendar** :
- Lecture automatique : lister des événements, ouvrir un événement précis.
- Écriture avec approbation HITL : créer ou modifier un événement.
- Suppression avec phrase de confirmation : supprimer un événement.

**OneDrive (lecture seule v0.1.0)** :
- Chercher dans vos fichiers, lire les métadonnées, télécharger un fichier, lister les fichiers récents.
- L'écriture OneDrive et le pattern workspace folder (équivalent du `Drive/Apollia/<agent>/` Google) arriveront dans une version ultérieure.

Microsoft Teams n'est pas couvert en v0.1.0.

## Multi-comptes

Comme pour Google, vous pouvez connecter plusieurs comptes Microsoft. Chaque compte garde son token séparé dans le trousseau. Au moment où un agent appelle un outil Microsoft, il peut choisir le compte cible si plusieurs sont connectés.

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

1. Ouvrez **Réglages → Intégrations OAuth**.
2. Collez l'identifiant dans le champ identifiant client de la carte Microsoft et enregistrez. Il est stocké dans `~/.apollia/oauth-clients.toml`, lisible par votre seul utilisateur. Laissez le champ secret vide, un client public n'en a pas.
3. Cliquez sur **Tester la configuration**.

Vider à nouveau le champ rétablit l'identifiant fourni avec Apollia. Les comptes déjà connectés l'ont été auprès de l'inscription précédente : déconnectez-les et reconnectez-les après un changement.

**Alternative pour un shell ou une machine sans interface.** `APOLLIA_MICROSOFT_CLIENT_ID` prime sur le fichier, et ne vaut que pour les processus lancés depuis le shell où vous l'avez exportée. Voir [Environment variables](/reference/environment-variables).

## Vérification

- La pastille à côté du compte est verte.
- Dans le chat libre, demandez *"Liste mes 3 derniers mails Outlook"*. La réponse arrive sans demande d'approbation.
- Tentez ensuite *"Envoie un mail à <votre adresse> avec le sujet test"*. Une popup d'approbation s'affiche avant l'envoi.
- Le trousseau du système contient une entrée `apollia-connector-microsoft` associée à votre adresse.

## Si ça ne marche pas

- **La carte Microsoft 365 affiche « Configuration requise »** : c'est impossible sur une version qui embarque l'identifiant, et ni une variable d'environnement vide ni une entrée vidée à la main dans `~/.apollia/oauth-clients.toml` ne le produisent : les deux sont ignorées lorsqu'elles sont vides et l'identifiant fourni reprend la main. Si vous le voyez malgré tout, c'est que la version installée a été compilée sans l'identifiant. Vérifiez avec `apollia-os connector list`, qui nomme la source du client résolu.
- **Microsoft rejette l'URI de redirection** : atteignable uniquement avec votre propre inscription. Elle n'a pas sa plateforme **Applications mobiles et de bureau**, ou celle-ci ne liste pas `http://127.0.0.1`. Une inscription créée en « Web » ne fonctionnera pas. Videz le champ identifiant client pour retomber sur l'inscription d'Apollia.
- **Microsoft répond que le compte n'existe pas dans l'annuaire** : également propre à votre inscription, cela signifie que les types de comptes pris en charge ont été réglés sur un tenant unique. Recréez-la avec **Comptes dans un annuaire organisationnel quelconque et comptes Microsoft personnels**, ou videz le champ pour utiliser celle d'Apollia.
- **Consentement refusé à l'écran Microsoft** : un tenant Entra ID géré exige souvent une approbation au niveau organisation avant qu'une application externe puisse être utilisée. Le texte d'erreur vient de Microsoft et non d'Apollia, et il nomme la politique de tenant en cause. Demandez à votre administrateur de pré-approuver l'application, ou utilisez un compte Microsoft personnel.
- **`outlook.send` échoue sur un destinataire** : Microsoft Graph valide les destinataires plus strictement que Google. Apollia remonte l'erreur de Graph telle quelle, préfixée du statut HTTP. Vérifiez l'adresse cible et l'absence d'alias mort.
- **OneDrive en écriture refusé** : c'est attendu en v0.1.0, OneDrive est en lecture seule.
- **Le bouton Connecter est grisé** : votre profil de souveraineté est `local_only`.

## Déconnecter un compte

Sur la carte Microsoft 365, cliquez sur **Déconnecter** à côté du compte. Le token est supprimé du trousseau local. Pour révoquer également côté Microsoft, allez sur https://myaccount.microsoft.com et retirez l'autorisation de l'application Apollia.

> **Référence technique :** [Référence Apollia](/reference) , flow OAuth Microsoft, scopes complets, refresh proactif.
