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
- Pour les serveurs MCP OAuth, service `apollia.mcp.<server-id>`.

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

- Le refresh est déclenché 5 minutes avant l'expiration de l'access token.
- Une protection **singleflight** : si plusieurs appels concurrents déclenchent un refresh sur le même compte, une seule requête HTTP est envoyée vers le provider. Sans cette protection, un burst d'appels d'agent ferait N requêtes parallèles et déclencherait un rate-limit cascade.

## Changer les scopes d'un compte

La v0.1.0 ne supporte pas le step-up auth automatique (demander seulement les nouveaux scopes). Procédure :

1. Déconnecter le compte dans **Connexions**.
2. Reconnecter en ajustant les cases à cocher avant de cliquer **Connecter**.

## Vérification

- Sur la carte du connecteur, le compte n'apparaît plus après déconnexion.
- L'outil trousseau du système ne montre plus d'entrée correspondante.
- Un appel d'outil natif sur le compte révoqué retourne `NotConnected`.

::: details Configuration avancée

### Mode fichier chiffré, Linux headless

Sur un Linux sans environnement graphique (container Docker, VM minimale, distribution serveur), le trousseau Secret Service n'est pas disponible. Apollia propose un stockage de secours sur fichier chiffré.

Variables d'environnement :

```bash
APOLLIA_TOKEN_STORAGE=file \
APOLLIA_TOKEN_PASSPHRASE="votre-phrase-secrète" \
apollia-os start
```

Les tokens sont stockés dans `~/.apollia/secrets/` chiffrés avec age (scrypt + ChaCha20-Poly1305). La passphrase est gardée en mémoire pour la session.

**Attention** : passphrase perdue = tokens perdus définitivement, il faut reconnecter chaque compte.

### Audit des actions

Toutes les exécutions d'outils sont loggées dans `governance.db` (table `tool_executions`) avec horodatage, agent, outil, hash du compte, statut d'approbation, latence. Consultable via **Paramètres, Historique des actions**.

Les approbations MCP (acceptations HITL durables) sont stockées séparément dans `~/.apollia/mcp-approvals.db`.

### Erreurs courantes mode fichier

- **`keyring: no entry`** : le daemon Secret Service n'est pas disponible. Bascule sur le mode fichier ci-dessus.
- **`NoRefreshToken`** au refresh : le compte a été connecté sans `offline_access` (Microsoft) ou sans `access_type=offline` (Google). Reconnectez.
- **Refresh en boucle 401** : le refresh token a été révoqué côté provider. Déconnectez puis reconnectez.

:::

## Si ça ne marche pas

- **Linux, "keyring: no entry"** : voir la section Configuration avancée pour le mode fichier chiffré.
- **`NoRefreshToken`** : reconnectez le compte, le scope `offline_access` a été oublié.
- **Refresh boucle 401** : déconnectez puis reconnectez le compte, le refresh token a été révoqué côté provider.

> **Référence technique :** [Briques-Auth](https://github.com/nidal-z/apollia-os/wiki/Briques-Auth) , stockage trousseau, refresh proactif, audit governance.db.
