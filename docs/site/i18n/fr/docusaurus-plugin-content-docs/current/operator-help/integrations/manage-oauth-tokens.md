---
title: Gérer les tokens OAuth
slug: /operator-help/integrations/manage-oauth-tokens
sidebar_position: 10
---

# Gérer les tokens OAuth

> Pour tout opérateur qui veut savoir où Apollia stocke ses tokens OAuth, comment les inspecter, les révoquer, et comprendre le refresh automatique.

## Prérequis

- Au moins un compte connecté (Google, Microsoft ou serveur MCP OAuth).
- Accès à l'outil de trousseau de votre système (Keychain Access, Gestionnaire d'identifiants, `secret-tool`).

## Où sont stockés mes tokens

Par défaut, Apollia stocke tous les tokens OAuth dans le trousseau de votre système.

| Système | Backend | Comment inspecter |
|---|---|---|
| macOS | Keychain Services | Application **Trousseau d'accès**, recherche `apollia-connector-` |
| Windows | Credential Manager | **Gestionnaire d'identifiants Windows**, **Identifiants génériques** |
| Linux | Secret Service (gnome-keyring ou KWallet via D-Bus) | `secret-tool search service apollia-connector-google` |

Convention de nommage :

- Service : `apollia-connector-<provider>` (par exemple `apollia-connector-google`, `apollia-connector-microsoft`).
- User : l'identifiant du compte, typiquement l'adresse email.
<!-- claim:mcp-oauth-uses-one-keyring-service -->
- Pour les serveurs MCP OAuth, un service unique `apollia-mcp-oauth`, avec le nom du serveur dans l'emplacement du compte.

Un index `~/.apollia/connectors-index.json` énumère les comptes connectés par provider (la plupart des trousseaux ne supportent pas l'énumération native).

## Inspecter un token

1. Ouvrir l'outil trousseau du système.
2. Chercher `apollia-connector-`.
3. Double-cliquer sur l'entrée du compte concerné.
4. Le contenu est un JSON sérialisé avec `access_token`, `refresh_token`, `expires_at`, `scopes`.

## Révoquer un compte

**Côté Apollia (trousseau local)** :

1. Ouvrir **Connexions**.
2. Ouvrir la fiche du compte, sur sa propre ligne de la liste.
3. Cliquer sur **Retirer**, lire ce qui part avec lui, puis cliquer sur **Retirer la connexion**. Le token est supprimé immédiatement du coffre de secrets et de l'index. Voir [Mettre en pause ou retirer une connexion](pause-or-remove-a-connection.md).

Pour arrêter un compte un temps sans perdre son token, cliquer plutôt sur **Mettre en pause** : ses outils cessent de s'exécuter, et **Reprendre** les rend.

Le token reste valide côté Google ou Microsoft jusqu'à son expiration naturelle (typiquement une heure pour l'access token).

**Côté provider (révocation complète)** :

- Google : https://myaccount.google.com/permissions, cliquer sur Apollia, **Supprimer l'accès**.
- Microsoft : https://myaccount.microsoft.com/consent, trouver Apollia, **Supprimer l'autorisation**.

Cette opération invalide aussi le refresh token. Recommandé pour une révocation propre.

## Multi-comptes

Chaque compte vit dans une entrée trousseau distincte avec l'email comme user. Chaque compte connecté est une connexion à part entière : ses outils s'accordent à un agent compte par compte, sur la fiche de l'agent, et un appel s'exécute au nom du compte accordé et d'aucun autre.

## Refresh automatique

Apollia rafraîchit les tokens proactivement :

- Le refresh est déclenché 60 secondes avant l'expiration de l'access token.
- Une protection **singleflight** : si plusieurs appels concurrents déclenchent un refresh sur le même compte, une seule requête HTTP est envoyée vers le provider. Sans cette protection, un burst d'appels d'agent ferait N requêtes parallèles et déclencherait un rate-limit cascade.

## Changer les scopes d'un compte

Vous ne pouvez pas. L'ensemble de scopes d'un connecteur natif est figé dans l'application : la fenêtre Google demande toujours les mêmes dix alias de scope et la fenêtre Microsoft les mêmes six, et il n'y a aucune case à ajuster avant de connecter. Retirer le compte puis le connecter à nouveau rejoue exactement la même demande. Restreindre ce qu'Apollia peut faire se joue côté fournisseur, en bridant le client OAuth, ou en ne connectant pas le compte.

## Vérification

- Le compte n'apparaît plus dans **Connexions** après son retrait.
- L'outil trousseau du système ne montre plus d'entrée correspondante.
- Un appel d'outil natif sur le compte révoqué retourne `NotConnected`.

<details>
<summary>Configuration avancée</summary>

### Linux headless : les tokens de connecteur sans trousseau

Sur un Linux sans environnement graphique (container Docker, VM minimale, distribution serveur), le trousseau Secret Service n'est généralement pas disponible. Les tokens de connecteur passent par le même coffre de secrets que tous les autres secrets d'Apollia, et la variable d'environnement `APOLLIA_TOKEN_STORAGE` choisit ce coffre.

Définissez `APOLLIA_TOKEN_STORAGE=file` et `APOLLIA_TOKEN_PASSPHRASE` avant de démarrer Apollia : les tokens Google et Microsoft sont alors écrits dans des fichiers chiffrés sous `~/.apollia/secrets/`, un par compte, au lieu du trousseau. La phrase de passe est obligatoire dans ce mode. Sans elle, Apollia refuse le coffre plutôt que de se rabattre sur quelque chose de plus faible.

L'autre option sur une machine headless est de faire tourner une implémentation Secret Service (`gnome-keyring-daemon --components=secrets`, déverrouillée à l'ouverture de session) et de garder le coffre par défaut. En mode fichier, les outils de trousseau cités plus haut ne montrent rien : les tokens sont les fichiers de ce dossier.

### Audit des actions

<!-- claim:journal-is-the-execution-record -->
Chaque appel d'outil d'une exécution est enregistré dans le journal signé, `~/.apollia/audit_journal.db` : l'agent, l'outil, sa famille d'action, sa cible, sa durée et son succès. Il se lit depuis **Journal** dans la barre latérale.

### Erreurs courantes mode fichier

- **Le compte ne peut pas être enregistré sous Linux** : aucun daemon Secret Service n'est disponible et le mode fichier n'est pas défini, ou il l'est sans sa phrase de passe. Voir la section Linux headless ci-dessus.
- **Un rafraîchissement annonce qu'aucun refresh token n'est disponible** : le fournisseur n'en a pas rendu. Ce n'est pas un scope oublié : Apollia ajoute toujours `access_type=offline` pour Google et inclut toujours `offline_access` pour Microsoft, cette cause n'est donc pas atteignable depuis l'interface. L'explication habituelle est une autorisation déjà révoquée côté fournisseur. Retirez le compte, puis connectez-le à nouveau.
- **Refresh en boucle 401** : le refresh token a été révoqué côté provider. Retirez le compte, puis connectez-le à nouveau.

</details>

## Si ça ne marche pas

- **Linux, le compte ne peut pas être enregistré** : voir la section Linux headless.
- **Un rafraîchissement annonce qu'aucun refresh token n'est disponible** : reconnectez le compte. Les scopes ne sont pas en cause, Apollia demande toujours l'accès hors ligne.
- **Refresh boucle 401** : retirez le compte et connectez-le à nouveau, le refresh token a été révoqué côté provider. **Vérifier** sur la fiche du compte indique alors **À réparer**, *le compte doit se reconnecter*, et **Se reconnecter** fait de même.

> **Référence technique :** [Référence Apollia](/reference) , stockage trousseau, refresh proactif, audit governance.db.
