# Gérer les tokens OAuth

::: warning Section en cours de refonte
Les pages **Intégrations** seront retravaillées après la release v0.1.0 : parcours UX revus, captures d'écran ajoutées, contenu enrichi. Le contenu actuel reste correct sur la mécanique, mais peut diverger de l'UI finale.
:::

> Pour les operators qui veulent savoir où Apollia stocke leurs tokens OAuth, comment les inspecter, les révoquer et basculer sur un stockage chiffré sur disque (Linux headless).

## Prérequis

- Au moins un compte connecté via **Intégrations** (Google ou Microsoft) ou via un serveur MCP OAuth 2.1.
- Accès à l'outil keyring de votre OS (Keychain Access, Gestionnaire d'identifiants, `secret-tool`).

## Emplacement des tokens

Apollia stocke tous les tokens OAuth (Google, Microsoft, et tous les MCP serveurs OAuth 2.1) dans le **keyring de votre OS**.

| OS | Backend | Inspection |
|---|---|---|
| macOS | Keychain Services | Application **Keychain Access**, chercher `apollia-connector-*` |
| Windows | Credential Manager | **Gestionnaire d'identifiants Windows → Identifiants génériques** |
| Linux | Secret Service (gnome-keyring / KWallet via D-Bus) | `secret-tool search service apollia-connector-google` |

Convention de nommage :

- Service : `apollia-connector-<provider>` (ex. `apollia-connector-google`, `apollia-connector-microsoft`)
- User : l'identifiant du compte (typiquement l'email)

Pour les MCP servers OAuth : service `apollia.mcp.<server-id>`.

## Linux headless (sans Secret Service) — mode avancé

> Cette section concerne uniquement les cas serveur Linux sans environnement graphique (container Docker, VM minimale, distribution serveur sans daemon Secret Service). Sur un poste de travail standard (macOS, Windows, Linux desktop), ignorez-la.

Sur un Linux sans daemon Secret Service, le keyring système n'est pas disponible. Apollia propose un stockage de secours sur fichier chiffré, activable au lancement :

```bash
APOLLIA_TOKEN_STORAGE=file \
  APOLLIA_TOKEN_PASSPHRASE="phrase-secrète-de-votre-choix" \
  apollia-desktop
```

Les tokens sont alors stockés dans `~/.apollia/secrets/` chiffrés avec [age](https://age-encryption.org/) (scrypt + ChaCha20-Poly1305). La passphrase est demandée au lancement et gardée en mémoire pour la session.

**Attention :** si vous oubliez la passphrase, les tokens sont perdus définitivement. Vous devrez reconnecter chaque compte.

## Étapes — Inspecter un token

1. Ouvrez l'outil keyring de votre OS (Keychain Access sur macOS).
2. Cherchez `apollia-connector-` pour lister les entrées Apollia.
3. Double-cliquez sur l'entrée correspondant au compte que vous voulez inspecter.
4. Le contenu est du JSON sérialisé avec `access_token`, `refresh_token`, `expires_at`, `scopes`.

## Étapes — Révoquer un token

### Côté Apollia (keyring local)

1. Ouvrez **Intégrations** dans Apollia Desktop.
2. Cliquez **Déconnecter** à côté du compte concerné.
3. Le token est immédiatement supprimé du keyring + de l'index `~/.apollia/connectors-index.json`.

**L'AS distant n'est pas notifié** — le token reste valide côté Google/Microsoft jusqu'à son expiration naturelle (typiquement 1 heure pour l'access token).

En CLI : `apollia auth revoke google --account nidal@example.com`.

### Côté provider (révocation complète)

Pour révoquer immédiatement le token côté Google / Microsoft :

- **Google** : https://myaccount.google.com/permissions → cliquer sur Apollia → **Supprimer l'accès**.
- **Microsoft** : https://myaccount.microsoft.com/consent → trouver Apollia → **Supprimer l'autorisation**.

Cette opération invalide aussi le refresh token, donc même si une copie traîne quelque part, elle devient inutilisable.

## Refresh automatique

Apollia rafraîchit les tokens **proactivement** :

- 60 secondes avant l'expiration, le prochain appel d'outil déclenche un refresh.
- Le refresh utilise le `refresh_token` stocké à la connexion initiale.
- **Singleflight** : si plusieurs appels concurrents déclenchent un refresh sur le même compte, une seule requête HTTP est envoyée — les autres attendent le résultat partagé. Sans cette protection, un burst de 10 appels d'agent ferait 10 requêtes parallèles vers l'AS et déclencherait un rate-limit cascade.

## Multi-comptes

Vous pouvez connecter plusieurs comptes par provider. Chaque token vit dans une entrée keyring distincte avec l'email comme user. L'index `~/.apollia/connectors-index.json` énumère les comptes connectés par provider — le keyring lui-même ne supporte pas l'énumération sur la plupart des OS.

Quand un agent utilise un outil natif (`gmail.send`, `outlook.search`…), il peut passer un paramètre optionnel `account` pour choisir le compte. Sans paramètre, le compte primaire (premier connecté) est utilisé.

## Changer les scopes (rotation)

Si vous voulez augmenter ou réduire les permissions d'un compte déjà connecté :

1. Déconnectez le compte dans **Intégrations**.
2. Reconnectez-le en ajustant les cases à cocher avant de cliquer **Connecter**.

v0.1.0 ne supporte pas le *step-up auth* automatique (re-demander seulement les nouveaux scopes). C'est prévu pour v0.2 (SEP-835).

## Vérification

- Sur la carte Google/Microsoft, le compte concerné n'apparaît plus après déconnexion.
- L'outil keyring de votre OS ne montre plus d'entrée correspondante.
- Tout appel d'outil natif (`gmail.send` etc.) sur le compte révoqué retourne `NotConnected`.

## Audit des accès

Toutes les opérations d'outils SaaS sont loggées dans `governance.db` (table `tool_executions`) avec :

- horodatage
- agent_id
- tool_id (ex. `gmail.send`)
- hash du compte utilisé (privacy)
- input_hash, output_summary
- status d'approbation
- latence

Consultable via **Historique des actions** dans l'UI Desktop ou via `apollia tool-governance audit --tool 'gmail.*'`.

## Si ça ne marche pas

- **Linux headless : `keyring: no entry`** : le daemon Secret Service n'est pas disponible. Lancez Apollia avec le mode fichier chiffré (cf. section *Linux headless* ci-dessus).
- **`AuthError::NoRefreshToken`** lors d'un refresh : le compte a été connecté sans le scope `offline_access` (Microsoft) ou sans `access_type=offline` (Google). Reconnectez le compte.
- **Le refresh boucle (401 répétés)** : le refresh token a été révoqué côté provider (par vous ou par leur politique de sécurité). Déconnectez puis reconnectez le compte.

> **Référence technique :** [Briques-Auth](https://github.com/nidal-z/apollia-os/wiki/Briques-Auth)
