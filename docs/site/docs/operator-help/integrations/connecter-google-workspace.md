# Connecter Google Workspace

> Pour tout operator qui veut brancher Gmail, Calendar, Drive, Sheets, Docs, Slides, Forms, Tasks ou YouTube à Apollia, en quelques clics.

## Prérequis

- Apollia lancé.
- Un compte Google personnel ou Workspace.
- Votre profil de souveraineté n'est pas réglé sur `local_only` (sinon les boutons cloud sont grisés).
- Connexion internet active.

## Étapes

1. Dans la sidebar, ouvrez **Connexions**, puis sélectionnez la carte **Google Workspace** dans la liste des connecteurs natifs.

   ![page Connexions, carte Google Workspace sélectionnée dans la sidebar (état Non connecté), panneau de droite avec l'onglet Comptes (0) et le bouton Connecter un compte](../_screenshots/integration-google-workspace-1.png)

2. Cliquez sur **Connecter un compte**. Une fenêtre s'ouvre dans Apollia et votre navigateur ouvre automatiquement la page de consentement Google.

3. Choisissez le compte Google à utiliser, puis acceptez les autorisations proposées (Mail, Calendar, Drive Workspace, etc.).

   ![écran de consentement Google, Apollia demande l'accès au compte, liste des autorisations (fichiers Drive de l'app, événements Calendar, envoi de mail, gestion des brouillons), avertissement app non vérifiée par Google](../_screenshots/integration-google-workspace-2.png)

4. De retour dans Apollia, la fenêtre détecte automatiquement le retour. Une seconde étape vous propose le dossier racine Drive de l'agent (défaut **Apollia**). Validez en cliquant **Enregistrer** (ou **Défaut** pour garder la valeur proposée).

   ![dialog Dossier Google Drive dans Apollia, explication du scope drive.file, champ Chemin du dossier avec la valeur Apollia, boutons Garder le défaut et Enregistrer](../_screenshots/integration-google-workspace-3.png)

5. La fenêtre se ferme, votre compte apparaît dans la sidebar avec une pastille verte.

   `[SCREENSHOT: sidebar Connexions, carte Google Workspace dépliée affichant le compte connecté, badge vert et bouton Déconnecter]`

## Ce que vous pouvez faire

**Lectures (sans approbation)** : lister vos brouillons Gmail, lister vos événements Calendar, parcourir le dossier `Apollia/` sur Drive, lire des cellules Sheets, lire du texte Docs, lister vos tâches, chercher des vidéos YouTube.

**Écritures (avec approbation HITL)** : envoyer un mail, créer un brouillon, créer ou modifier un événement Calendar, écrire un fichier Drive Workspace, ajouter ou modifier des valeurs dans Sheets, ajouter du texte à un Doc, créer un Slide, créer un formulaire, créer ou compléter une tâche.

**Suppressions (avec phrase de confirmation)** : supprimer un événement Calendar, supprimer une tâche.

## Pattern Workspace Drive

Avec le scope `drive.file`, l'application ne voit que les fichiers qu'elle a créés ou ceux que vous lui ouvrez explicitement. Apollia s'appuie sur ce comportement :

- À la première connexion, un dossier racine `Apollia` est créé à la racine de votre Drive.
- À chaque création de fichier par un agent, un sous-dossier `Apollia/<nom-agent>/` est créé à la demande.
- Les opérations Drive sont scopées à ce dossier. L'agent ne voit pas le reste de votre Drive.

L'agent peut donc sauvegarder une note `meeting-notes.md` dans son workspace, la relire plus tard, et la supprimer, sans jamais voir le reste de votre Drive.

## Multi-comptes

Vous pouvez connecter plusieurs comptes Google. Chaque compte apparaît dans la sidebar avec son adresse email. Au moment où un agent appelle un outil Google, il peut choisir le compte cible via un paramètre `account` si plusieurs comptes sont connectés.

## Vérification

- La pastille à côté du compte est verte.
- Dans le chat libre, demandez par exemple *"Liste mes 3 derniers événements Calendar"*. La réponse arrive sans demande d'approbation.
- Tentez ensuite *"Envoie un mail à <votre adresse> avec le sujet test"*. Une popup d'approbation s'affiche avant l'envoi.
- Le trousseau de votre système (Keychain sur macOS) contient une entrée `apollia-connector-google` associée à votre adresse email.

## Si ça ne marche pas

- **L'écran de consentement Google affiche "Cette app n'est pas vérifiée"** : c'est normal en mode expert avec votre propre app OAuth. Cliquez sur **Avancé** puis **Accéder à Apollia** pour continuer.
- **Le bouton Connecter est grisé** : votre profil de souveraineté est `local_only`. Les connecteurs cloud sont désactivés dans ce mode.
- **Vous voulez la lecture complète Gmail ou Drive** : ces scopes sont restricted (audit Google CASA) et hors v0.1.0 multi-tenant. Voir [Mode expert Google](mode-expert-google-restricted-scopes.md).
- **L'agent ne voit pas un fichier précis sur Drive** : il n'a accès qu'au dossier `Apollia/<agent>/`. Déposez le fichier dans ce dossier ou passez-lui l'identifiant explicite dans votre prompt.
- **L'agent reçoit l'erreur `ScopeMissing`** : il appelle un outil restricted (`gmail.search`, `gmail.get`, etc.) qui exige le mode expert. Voir [Mode expert Google](mode-expert-google-restricted-scopes.md).

## Déconnecter un compte

Sur la carte Google Workspace, cliquez sur **Déconnecter** à côté du compte concerné. Le token est immédiatement supprimé du trousseau local. Pour révoquer également côté Google, allez sur https://myaccount.google.com/permissions et retirez l'application Apollia.

> **Référence technique :** [Référence Apollia](../../reference/index.md) , scopes complets, refresh proactif, multi-comptes, stockage trousseau.
