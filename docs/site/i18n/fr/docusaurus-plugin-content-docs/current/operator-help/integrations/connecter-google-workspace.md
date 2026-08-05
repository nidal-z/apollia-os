# Connecter Google Workspace

> Pour tout operator qui veut brancher Gmail, Calendar, Drive, Sheets, Docs, Slides, Forms, Tasks ou YouTube à Apollia.

Un compte `@gmail.com` personnel convient. Rien ici n'exige un abonnement Workspace, un domaine d'entreprise ou un administrateur.

## Prérequis

- Apollia lancé.
- Un compte Google, personnel ou Workspace.
- **Votre propre client OAuth**, configuré une fois, voir la section juste en dessous.
- Votre profil de souveraineté n'est pas réglé sur `local_only` (sinon les boutons cloud sont grisés).
- Connexion internet active.

:::info Google en demande plus que Microsoft
Microsoft 365 se connecte immédiatement, sans rien inscrire. Google non, et ne le peut pas : une dizaine de minutes dans la console Google Cloud viennent d'abord. La raison appartient à Google, pas à Apollia, et elle est détaillée ci-dessous.
:::

## Configurer votre client OAuth, une fois

<!-- claim:oauth-google-client-not-embedded -->
Apollia est livré sans client OAuth Google, et aucun build publié n'en embarque. Vous enregistrez votre propre application chez Google et vous en confiez les identifiants à Apollia. Comptez dix minutes la première fois.

Si vous voulez chaque écran de la console nommé clic par clic, suivez [Créer un client OAuth Google](/how-to/set-up-a-google-oauth-client) et revenez ici à l'étape « Dans Apollia ». La version courte suit.

**Pourquoi il n'y a pas de client partagé.** Google n'autorise pas une application à servir des comptes hors de son propre projet tant que son écran de consentement n'a pas passé la vérification, et les scopes classés *restricted* (`gmail.readonly`, `gmail.modify`, `gmail.compose`, `drive.readonly`, `drive`) exigent en plus un audit CASA Tier 2 par un tiers agréé Google, facturé 5 000 à 15 000 dollars par an. Une application Apollia partagée placerait aussi tous les utilisateurs derrière un seul quota et un seul écran de consentement. Votre propre client vous rend la maîtrise des deux. Microsoft n'a pas d'exigence équivalente pour un client public de bureau, et c'est là toute la différence entre les deux pages.

**Ce que coûte le statut Testing.** Laisser l'écran de consentement en **Testing** est gratuit et immédiat, avec deux limites à connaître avant de commencer : 100 test users au maximum, et **les jetons de rafraîchissement expirent au bout de sept jours**, il faudra donc reconnecter le compte environ une fois par semaine. Passer l'écran en **Production** sans vérification supprime l'expiration à sept jours mais affiche un avertissement « application non vérifiée » qu'il faut franchir. La vérification elle-même est gratuite pour les portées qu'Apollia demande par défaut, et prend plusieurs semaines.

**Dans la console Google Cloud.**

1. Créez un projet, puis activez les APIs Gmail, Calendar et Drive.
2. Configurez l'écran de consentement OAuth en mode **External**, laissez-le en statut **Testing**, et ajoutez votre propre adresse comme test user.
3. Créez un client OAuth de type **Desktop app**.
4. **Téléchargez le fichier JSON** que la console propose. Gardez-le, il sert à l'étape suivante.

<!-- claim:oauth-google-client-json-import -->
**Dans Apollia.**

1. Ouvrez **Réglages → Intégrations OAuth**.
2. Sur la carte Google, cliquez sur **Importer le JSON** et choisissez le fichier que vous venez de télécharger. L'identifiant client et le secret client en sont lus et stockés dans `~/.apollia/oauth-clients.toml`, lisible par votre seul utilisateur.
3. Cliquez sur **Tester la configuration**. Le rapport doit indiquer que le client est présent, bien formé, et que le serveur d'autorisation Google est joignable.

Si vous préférez saisir les valeurs vous-même, les deux champs de la même carte les acceptent directement.

**Pourquoi un secret client.** Google délivre un `client_secret` avec l'identifiant client pour un client Desktop, et l'exige à l'échange du code d'autorisation contre un jeton, alors même qu'Apollia utilise aussi PKCE. Apollia le stocke localement et ne l'envoie qu'à Google.

<!-- claim:oauth-connect-refuses-before-consent -->
Si l'une des deux moitiés manque, Apollia refuse la connexion avant d'ouvrir votre navigateur et vous dit laquelle, plutôt que de vous faire traverser un écran de consentement qui n'aboutirait pas.

**Alternative pour un shell ou une machine sans interface.** `APOLLIA_GOOGLE_CLIENT_ID` et `APOLLIA_GOOGLE_CLIENT_SECRET` priment sur le fichier. Elles ne valent que pour les processus lancés depuis le shell où vous les avez exportées, ce qui explique souvent qu'un client paraisse configuré sans l'être. Voir [Environment variables](/reference/environment-variables).

## Étapes

1. Dans la sidebar, ouvrez **Connexions**, puis sélectionnez la carte **Google Workspace** dans la liste des connecteurs natifs.

   ![page Connexions, carte Google Workspace sélectionnée dans la sidebar (état Non connecté), panneau de droite avec l'onglet Comptes (0) et le bouton Connecter un compte](/img/operator-help/integration-google-workspace-1.png)

2. Cliquez sur **Connecter un compte**. Une fenêtre s'ouvre dans Apollia et votre navigateur ouvre automatiquement la page de consentement Google.

3. Choisissez le compte Google à utiliser, puis acceptez les autorisations proposées (Mail, Calendar, Drive Workspace, etc.).

   ![écran de consentement Google, Apollia demande l'accès au compte, liste des autorisations (fichiers Drive de l'app, événements Calendar, envoi de mail, gestion des brouillons), avertissement app non vérifiée par Google](/img/operator-help/integration-google-workspace-2.png)

4. De retour dans Apollia, la fenêtre détecte automatiquement le retour. Une seconde étape vous propose le dossier racine Drive de l'agent (défaut **Apollia**). Validez en cliquant **Enregistrer** (ou **Défaut** pour garder la valeur proposée).

   ![dialog Dossier Google Drive dans Apollia, explication du scope drive.file, champ Chemin du dossier avec la valeur Apollia, boutons Garder le défaut et Enregistrer](/img/operator-help/integration-google-workspace-3.png)

5. La fenêtre se ferme, votre compte apparaît dans la sidebar avec une pastille verte.

   A l'ecran : la sidebar Connexions, avec la carte Google Workspace dépliée affichant le compte connecté, sa pastille verte et le bouton Déconnecter.

## Ce que vous pouvez faire

**Lectures (sans approbation)** : lister vos événements Calendar, parcourir le dossier `Apollia/` sur Drive, lire des cellules Sheets, lire du texte Docs, lister vos tâches, chercher des vidéos YouTube.

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

- **La carte Google Workspace affiche « Configuration requise »** : aucun client OAuth n'est encore configuré. Cliquez sur **Configurer les identifiants** et suivez la section en haut de cette page.
- **Apollia signale un secret client manquant** : l'identifiant client a été enregistré, mais pas son secret. Réimportez le fichier JSON de la console Google Cloud, qui porte les deux, ou collez le secret dans la carte Google de **Réglages → Intégrations OAuth**.
- **L'écran de consentement Google affiche "Cette app n'est pas vérifiée"** : c'est normal, l'application est la vôtre et reste en statut Testing. Cliquez sur **Avancé** puis **Accéder à Apollia** pour continuer.
- **Le bouton Connecter est grisé** : votre profil de souveraineté est `local_only`. Les connecteurs cloud sont désactivés dans ce mode.
- **Vous voulez la lecture complète Gmail ou Drive** : ces scopes sont restricted (audit Google CASA) et hors v0.1.0. Aucun outil Apollia ne les exploite encore, voir « À propos des scopes restricted » ci-dessous.
- **L'agent ne voit pas un fichier précis sur Drive** : il n'a accès qu'au dossier `Apollia/<agent>/`. Déposez le fichier dans ce dossier ou passez-lui l'identifiant explicite dans votre prompt.
- **Un agent demande un outil Gmail de lecture complète** : il n'en existe pas. Le catalogue d'opérations Google ne contient que les scopes non restricted, et un test le verrouille. L'agent recevra une erreur d'outil inconnu, pas une erreur de scope.

## À propos des scopes restricted

Votre écran de consentement OAuth peut proposer les scopes *restricted* (`gmail.readonly`, `gmail.modify`, `drive.readonly`, `drive`), mais **aucun outil Apollia ne les exploite encore** : le catalogue d'opérations Google n'en contient aucun, et un test le verrouille. En accorder un ne débloque aucune capacité pour l'instant. Le périmètre par défaut couvre déjà l'envoi, la composition, le calendrier complet et le Drive scopé.

**Responsabilité.** L'application OAuth est la vôtre et Apollia n'audite pas cette configuration. Si vous distribuez un build avec votre identifiant client embarqué au-delà de 100 utilisateurs, Google exigera l'audit CASA Tier 2.

**Alternative.** Si Google Cloud Console vous semble lourd, un serveur MCP Gmail communautaire (cherchez `mcp-server-gmail`) tourne localement avec vos credentials et expose les outils Gmail via MCP. Voir [Câbler son propre serveur MCP](cabler-son-propre-serveur-mcp.md).

## Déconnecter un compte

Sur la carte Google Workspace, cliquez sur **Déconnecter** à côté du compte concerné. Le token est immédiatement supprimé du trousseau local. Pour révoquer également côté Google, allez sur https://myaccount.google.com/permissions et retirez l'application Apollia.

> **Référence technique :** [Référence Apollia](/reference) , scopes complets, refresh proactif, multi-comptes, stockage trousseau.
