---
title: Connecter Google Workspace
slug: /operator-help/integrations/connect-google-workspace
sidebar_position: 3
---

# Connecter Google Workspace

> Pour tout opérateur qui veut brancher Gmail, Calendar, Drive, Sheets, Docs, Slides, Forms ou Tasks à Apollia.

Un compte `@gmail.com` personnel convient. Rien ici n'exige un abonnement Workspace, un domaine d'entreprise ou un administrateur.

## Prérequis

- Apollia lancé.
- Un compte Google, personnel ou Workspace.
- **Votre propre client OAuth**, configuré une fois, voir la section juste en dessous.
- La politique de votre organisation, si elle en a déployé une, n'interdit pas les connexions distantes. Sinon la connexion est refusée au clic.
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

1. Créez un projet, puis activez les APIs que l'étape de consentement demandera : Gmail, Calendar, Drive, Sheets, Docs, Slides, Tasks et Forms. Chacune vit derrière sa propre API dans la console, et un service dont l'API est éteinte échoue à l'appel même si le scope a été accordé.
2. Configurez l'écran de consentement OAuth en mode **External**, laissez-le en statut **Testing**, et ajoutez votre propre adresse comme test user.
3. Créez un client OAuth de type **Desktop app**.
4. **Téléchargez le fichier JSON** que la console propose. Gardez-le, il sert à l'étape suivante.

<!-- claim:oauth-google-client-json-import -->
**Dans Apollia.**

1. Activez le mode développeur, dans **Réglages**, section **Développeur** : le client ne se saisit que là. Ouvrez ensuite **Connexions**, cliquez sur **Ajouter une connexion**, choisissez **Google Workspace** sous **Proposés par Apollia**, cliquez sur **Ajouter**, puis sur **Se connecter**. Sans client, Apollia s'arrête sur **Identifiant OAuth manquant** ; cliquez sur **Saisir le client OAuth**. La fiche Google Workspace s'ouvre sur sa section **Client Google**.
2. Cliquez sur **Importer le JSON du client** et choisissez le fichier que vous venez de télécharger. L'identifiant client et le secret client en sont lus. L'identifiant client est rangé dans un fichier de configuration de votre poste ; le secret client est rangé dans le trousseau du système, jamais dans un fichier.
3. Cliquez sur **Tester le client**. La réponse attendue est **La configuration semble valide.**

Si vous préférez saisir les valeurs vous-même, les champs **Client ID** et **Secret client** de la même section les acceptent directement.

**Pourquoi un secret client.** Google délivre un `client_secret` avec l'identifiant client pour un client Desktop, et l'exige à l'échange du code d'autorisation contre un jeton, alors même qu'Apollia utilise aussi PKCE. Apollia le range dans le trousseau du système et ne l'envoie qu'à Google.

<!-- claim:oauth-connect-refuses-before-consent -->
Si l'une des deux moitiés manque, Apollia refuse la connexion avant d'ouvrir votre navigateur et vous dit laquelle, plutôt que de vous faire traverser un écran de consentement qui n'aboutirait pas.

**Alternative pour un shell ou une machine sans interface.** `APOLLIA_GOOGLE_CLIENT_ID` et `APOLLIA_GOOGLE_CLIENT_SECRET` priment sur ce qui a été saisi sur la fiche. Elles ne valent que pour les processus lancés depuis le shell où vous les avez exportées, ce qui explique souvent qu'un client paraisse configuré sans l'être. Voir [Environment variables](/reference/environment-variables).

## Étapes

1. Dans la barre latérale, ouvrez **Connexions**, cliquez sur **Ajouter une connexion**, et choisissez **Google Workspace** sous **Proposés par Apollia**. Cliquez sur **Ajouter**.

2. À l'étape **Brancher**, cliquez sur **Se connecter**. Une fenêtre s'ouvre dans Apollia et votre navigateur ouvre automatiquement la page de consentement Google.

3. Choisissez le compte Google à utiliser, puis acceptez les autorisations proposées (Mail, Calendar, Drive Workspace, etc.).

4. De retour dans Apollia, la fenêtre détecte automatiquement le retour. Une seconde étape vous propose le dossier racine Drive de l'agent (défaut **Apollia**). Validez en cliquant **Enregistrer** (ou **Garder le défaut** pour garder la valeur proposée).

5. À **Qui peut s'en servir**, cochez les agents qui peuvent se servir du compte ; **Apollia, dans le Chat** vient coché. Cliquez sur **Continuer**.

6. **Essai** confirme que Google Workspace est ajouté, avec **Essayer** et **Terminé**. Le compte apparaît dans la liste **Connexions**, sur sa propre ligne, avec son adresse.

   À l'écran : la fiche du compte Google dans Connexions, avec son état, ses agents et son dossier Drive.

## Ce que vous pouvez faire

**Lectures** : lister vos événements Calendar, parcourir le dossier `Apollia/` sur Drive, lire des cellules Sheets, lire du texte Docs, lister vos tâches. Lire votre compte, c'est lire des données personnelles : Apollia demande donc votre accord, et répondre **Toujours pour cet agent** fait cesser la question pour cet agent.

Deux opérations YouTube existent dans le catalogue et sont inutilisables : l'étape de consentement ne demande jamais le scope YouTube, donc aucune des deux ne peut s'authentifier. Considérez YouTube comme absent.

**Écritures et envois (avec votre accord)** : envoyer un mail, créer un brouillon, créer ou modifier un événement Calendar, écrire un fichier Drive Workspace, ajouter ou modifier des valeurs dans Sheets, ajouter du texte à un Doc, créer un Slide, créer un formulaire, créer ou terminer une tâche.

**Suppressions (avec votre accord, comme toute écriture)** : supprimer un événement Calendar, supprimer une tâche. Il n'y a aucune phrase à taper.

## Pattern Workspace Drive

Avec le scope `drive.file`, l'application ne voit que les fichiers qu'elle a créés ou ceux que vous lui ouvrez explicitement. Apollia s'appuie sur ce comportement :

- Le dossier racine `Apollia` est créé à la première écriture, pas à la connexion. L'étape où vous le nommez ne fait qu'enregistrer le chemin.
- À chaque création de fichier par un agent, un sous-dossier `Apollia/<nom-agent>/` est créé à la demande.
- Les opérations Drive sont scopées à ce dossier. L'agent ne voit pas le reste de votre Drive.

L'agent peut donc sauvegarder une note `meeting-notes.md` dans son workspace, la relire plus tard, et la supprimer, sans jamais voir le reste de votre Drive.

## Multi-comptes

Vous pouvez connecter plusieurs comptes Google : sur la fiche d'un compte, **Ajouter un autre compte** connecte le suivant, et chacun a sa propre ligne dans la liste avec son adresse email. Chaque compte connecté est une connexion à part entière. Ses outils se donnent à un agent compte par compte, sur la fiche du compte avec **Donner à un agent** ou sur la fiche de l'agent, et un appel s'exécute au nom du compte donné et d'aucun autre.

## Vérification

- Le compte a sa ligne dans **Connexions**, et sa fiche indique **Fonctionne** une fois vérifiée ou utilisée.
- Sur la fiche, cliquez sur **Essayer** et demandez par exemple *« Liste mes 3 derniers événements Calendar »*. Apollia répond avec les outils de cette connexion, et demande votre accord avant de lire le compte.
- Tentez ensuite *« Envoie un mail à <votre adresse> avec le sujet test »*. Une demande d'accord s'affiche avant l'envoi.
- Le trousseau de votre système (Keychain sur macOS) contient une entrée `apollia-connector-google` associée à votre adresse email.

## Si ça ne marche pas

- **La fiche Google Workspace affiche « Configuration requise avant de connecter »** : aucun client OAuth n'est encore configuré. Activez le mode développeur et suivez la section en haut de cette page.
- **Apollia signale un secret client manquant** : l'identifiant client a été enregistré, mais pas son secret. Réimportez le fichier JSON de la console Google Cloud, qui porte les deux, ou collez le secret dans le champ **Secret client** de la section **Client Google**, visible avec le mode développeur activé, dans le **Détail technique** de la fiche du compte ou sur la fiche Google Workspace.
- **L'écran de consentement Google affiche "Cette app n'est pas vérifiée"** : c'est normal, l'application est la vôtre et reste en statut Testing. Cliquez sur **Avancé** puis **Accéder à Apollia** pour continuer.
- **La connexion s'arrête sur « Refusé par la politique de l'organisation »** : la politique de votre organisation interdit les connexions à des services en ligne sur ce poste, et rien n'a été envoyé à Google. Seule la personne qui gère cette politique peut accorder une exception ; rien dans Apollia ne la lève.
- **Vous voulez la lecture complète Gmail ou Drive** : ces scopes sont restricted (audit Google CASA). Aucun outil Apollia ne les exploite encore, voir « À propos des scopes restricted » ci-dessous.
- **L'agent ne voit pas un fichier précis sur Drive** : il n'a accès qu'au dossier `Apollia/<agent>/`. Déposez le fichier dans ce dossier ou passez-lui l'identifiant explicite dans votre prompt.
- **Un agent demande un outil Gmail de lecture complète** : il n'en existe pas. Le catalogue d'opérations Google ne contient que les scopes non restricted, et un test le verrouille. L'agent recevra une erreur d'outil inconnu, pas une erreur de scope.

## À propos des scopes restricted

Votre écran de consentement OAuth peut proposer les scopes *restricted* (`gmail.readonly`, `gmail.modify`, `gmail.compose`, `drive.readonly`, `drive`), mais **aucun outil Apollia ne les exploite encore** : le catalogue d'opérations Google n'en contient aucun, et un test le verrouille. En accorder un ne débloque aucune capacité pour l'instant. Le périmètre par défaut couvre déjà l'envoi, la composition, le calendrier complet et le Drive scopé.

**Responsabilité.** L'application OAuth est la vôtre et Apollia n'audite pas cette configuration. Si vous distribuez un build avec votre identifiant client embarqué au-delà de 100 utilisateurs, Google exigera l'audit CASA Tier 2.

**Alternative.** Si Google Cloud Console vous semble lourd, un serveur MCP Gmail communautaire (cherchez `mcp-server-gmail`) tourne localement avec vos credentials et expose les outils Gmail via MCP. Voir [Câbler son propre serveur MCP](cabler-son-propre-serveur-mcp.md).

## Retirer un compte

Sur la fiche du compte, cliquez sur **Retirer** : la fiche liste ce qui part avec lui, puis **Retirer la connexion** confirme. Le token est supprimé du coffre de secrets local. Voir [Mettre en pause ou retirer une connexion](pause-or-remove-a-connection.md). Pour révoquer également côté Google, allez sur https://myaccount.google.com/permissions et retirez l'application Apollia.

> **Référence technique :** [Référence Apollia](/reference) , scopes complets, refresh proactif, multi-comptes, stockage trousseau.
