# Gérer les tokens OAuth

> Pour tout operator qui veut savoir où Apollia stocke ses tokens OAuth, comment les inspecter, les révoquer, et comprendre le refresh automatique.

## Prérequis

- Au moins un compte connecté (Google, Microsoft ou serveur MCP OAuth).
- Accès à l'outil de trousseau de votre système (Keychain Access, Gestionnaire d'identifiants, `secret-tool`).

## Où sont stockés mes tokens

Apollia stocke tous les tokens OAuth dans le trousseau de votre système.

| Système | Backend | Comment inspecter |
|---|---|---|
| macOS | Keychain Services | Application **Trousseau d'accès**, recherche `apollia-connector-` |
| Windows | Credential Manager | **Gestionnaire d'identifiants Windows**, **Identifiants génériques** |
| Linux | Secret Service (gnome-keyring ou KWallet via D-Bus) | `secret-tool search service apollia-connector-google` |

Convention de nommage :

- Service : `apollia-connector-<provider>` (par exemple `apollia-connector-google`, `apollia-connector-microsoft`).
- User : l'identifiant du compte, typiquement l'adresse email.
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
2. Sélectionner le compte.
3. Cliquer **Déconnecter**. Le token est supprimé immédiatement du trousseau et de l'index.

Le token reste valide côté Google ou Microsoft jusqu'à son expiration naturelle (typiquement une heure pour l'access token).

**Côté provider (révocation complète)** :

- Google : https://myaccount.google.com/permissions, cliquer sur Apollia, **Supprimer l'accès**.
- Microsoft : https://myaccount.microsoft.com/consent, trouver Apollia, **Supprimer l'autorisation**.

Cette opération invalide aussi le refresh token. Recommandé pour une révocation propre.

## Multi-comptes

Chaque compte vit dans une entrée trousseau distincte avec l'email comme user. Quand un agent appelle un outil natif (`gmail.send`, `outlook.search`, etc.), il peut passer un paramètre `account` pour choisir le compte. Sans paramètre, le compte primaire (premier connecté) est utilisé.

## Refresh automatique

Apollia rafraîchit les tokens proactivement :

- Le refresh est déclenché 60 secondes avant l'expiration de l'access token.
- Une protection **singleflight** : si plusieurs appels concurrents déclenchent un refresh sur le même compte, une seule requête HTTP est envoyée vers le provider. Sans cette protection, un burst d'appels d'agent ferait N requêtes parallèles et déclencherait un rate-limit cascade.

## Changer les scopes d'un compte

La v0.1.0 ne supporte pas le step-up auth automatique (demander seulement les nouveaux scopes). Procédure :

1. Déconnecter le compte dans **Connexions**.
2. Reconnecter en ajustant les cases à cocher avant de cliquer **Connecter**.

## Vérification

- Sur la carte du connecteur, le compte n'apparaît plus après déconnexion.
- L'outil trousseau du système ne montre plus d'entrée correspondante.
- Un appel d'outil natif sur le compte révoqué retourne `NotConnected`.

<details>
<summary>Configuration avancée</summary>

### Linux headless : les tokens de connecteur exigent un trousseau

Sur un Linux sans environnement graphique (container Docker, VM minimale, distribution serveur), le trousseau Secret Service n'est pas disponible, et **les comptes de connecteur ne peuvent pas être stockés en `v0.1.0-preview`**. Connecter Google ou Microsoft sur une telle machine échoue au moment d'enregistrer le token.

Un backend fichier chiffré existe dans le code, sélectionné par `APOLLIA_TOKEN_STORAGE=file`, et il ne s'applique pas ici : le chemin de stockage des tokens de connecteur appelle le trousseau système en direct au lieu de passer par le store sélectionnable, donc poser la variable ne change rien pour les comptes Google ou Microsoft. Ne comptez pas dessus comme contournement.

Les options sur une machine headless sont de faire tourner une implémentation Secret Service (`gnome-keyring-daemon --components=secrets`, déverrouillée à l'ouverture de session), ou de connecter les comptes depuis une machine de bureau.

### Audit des actions

Toutes les exécutions d'outils sont loggées dans `~/.apollia/audit.db`, table `tool_invocations` : agent, tâche, exécution, nom de l'outil, empreinte de l'entrée, profil de bac à sable, durée et code de sortie. Elle enregistre ce qui a tourné, pas qui l'a approuvé. Consultable depuis la page **Observabilité** de la barre latérale, onglet **Piste d'audit**.

Les approbations MCP (acceptations HITL durables) sont stockées séparément dans `~/.apollia/mcp_approvals.db`.

### Erreurs courantes mode fichier

- **`keyring: no entry`** : le daemon Secret Service n'est pas disponible. Voir la section Linux headless ci-dessus : il n'y a pas de contournement en `v0.1.0-preview`.
- **`NoRefreshToken`** au refresh : le compte a été connecté sans `offline_access` (Microsoft) ou sans `access_type=offline` (Google). Reconnectez.
- **Refresh en boucle 401** : le refresh token a été révoqué côté provider. Déconnectez puis reconnectez.

</details>

## Si ça ne marche pas

- **Linux, "keyring: no entry"** : voir la section Linux headless.
- **`NoRefreshToken`** : reconnectez le compte, le scope `offline_access` a été oublié.
- **Refresh boucle 401** : déconnectez puis reconnectez le compte, le refresh token a été révoqué côté provider.

> **Référence technique :** [Référence Apollia](/reference) , stockage trousseau, refresh proactif, audit governance.db.
