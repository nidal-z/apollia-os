# Connecter Microsoft 365

> Pour tout operator qui veut brancher Outlook, Calendar et OneDrive à Apollia, que son compte soit personnel (outlook.com, hotmail.com, live.com) ou professionnel (Microsoft 365, Entra ID).

## Prérequis

- Apollia lancé.
- Un compte Microsoft, personnel ou professionnel.
- Votre profil de souveraineté n'est pas réglé sur `local_only`.
- Si votre tenant Entra ID exige une approbation administrative, l'administrateur doit pré-approuver Apollia.
- Connexion internet active.

## Quel type de compte je peux utiliser

Les deux fonctionnent nativement avec l'app Apollia, sans configuration supplémentaire. L'endpoint utilisé (`/common/`) accepte indifféremment :

- les comptes personnels Microsoft (outlook.com, hotmail.com, live.com),
- les comptes professionnels ou d'éducation (Entra ID, M365 Business, M365 Developer tenant).

Voir les différences observables dans le tableau plus bas.

## Étapes

1. Dans la sidebar, ouvrez **Connexions**, puis sélectionnez la carte **Microsoft 365**.

   `[SCREENSHOT: page Connexions, carte Microsoft 365 mise en évidence avec bouton "Connecter un compte" dans le panneau de droite]`

2. Cliquez sur **Connecter un compte**. Une fenêtre s'ouvre dans Apollia et votre navigateur ouvre la page de consentement Microsoft.

3. Authentifiez-vous avec votre compte Microsoft, puis acceptez les autorisations (Mail, Calendar, Files).

   `[SCREENSHOT: page consent Microsoft, liste des accès Mail/Calendar/Files, boutons Non et Oui]`

4. De retour dans Apollia, la fenêtre détecte le retour automatiquement et se ferme. Votre compte apparaît dans la sidebar avec une pastille verte.

   `[SCREENSHOT: sidebar Connexions, carte Microsoft 365 dépliée avec le compte connecté et badge vert]`

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

## Vérification

- La pastille à côté du compte est verte.
- Dans le chat libre, demandez *"Liste mes 3 derniers mails Outlook"*. La réponse arrive sans demande d'approbation.
- Tentez ensuite *"Envoie un mail à <votre adresse> avec le sujet test"*. Une popup d'approbation s'affiche avant l'envoi.
- Le trousseau du système contient une entrée `apollia-connector-microsoft` associée à votre adresse.

## Si ça ne marche pas

- **AADSTS90094, "administrateur consent requis"** : votre tenant Entra ID exige une approbation au niveau organisation. Contactez votre administrateur pour qu'il pré-approuve Apollia, ou utilisez un compte Microsoft personnel.
- **AADSTS500011, "application introuvable dans le tenant"** : votre administrateur a restreint les apps externes. Demandez-lui de pré-approuver Apollia, ou utilisez un compte personnel.
- **`outlook.send` échoue avec `ErrorRecipientNotResolved`** : Microsoft Graph valide les destinataires plus strictement que Google. Vérifiez l'adresse cible et l'absence d'alias mort.
- **OneDrive en écriture refusé** : c'est attendu en v0.1.0, OneDrive est en lecture seule.
- **Le bouton Connecter est grisé** : votre profil de souveraineté est `local_only`.

## Déconnecter un compte

Sur la carte Microsoft 365, cliquez sur **Déconnecter** à côté du compte. Le token est supprimé du trousseau local. Pour révoquer également côté Microsoft, allez sur https://myaccount.microsoft.com et retirez l'autorisation de l'application Apollia.

> **Référence technique :** [Briques-Auth](https://github.com/Apollia-OS/apollia-os/wiki/Briques-Auth) , flow OAuth Microsoft, scopes complets, refresh proactif.
