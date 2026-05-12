# Gérer les tokens OAuth

> **Référence technique :** [Briques-OAuth-Manager](https://github.com/nidal-z/apollia-os/wiki/Briques-OAuth-Manager)

Apollia stocke tous les tokens OAuth (Google, Microsoft, et tous les MCP serveurs OAuth 2.1) dans le **keyring de votre OS**. Cette page explique où les tokens vivent, comment les inspecter, les révoquer, et changer de scopes.

## Emplacement des tokens

| OS | Backend | Inspection |
|---|---|---|
| macOS | Keychain Services | Application **Keychain Access**, chercher `apollia-connector-*` |
| Windows | Credential Manager | **Gestionnaire d'identifiants Windows → Identifiants génériques** |
| Linux | Secret Service (gnome-keyring / KWallet via D-Bus) | `secret-tool search service apollia-connector-google` |

Convention de nommage :
- Service : `apollia-connector-<provider>` (ex. `apollia-connector-google`, `apollia-connector-microsoft`)
- User : l'identifiant du compte (typiquement l'email)

Pour les MCP servers OAuth : service `apollia.mcp.<server-id>`.

## Linux headless (sans Secret Service)

Sur un serveur Linux sans daemon Secret Service (container, VM minimale, distribution serveur), le keyring crate échoue. Définissez `APOLLIA_TOKEN_STORAGE=file` pour basculer sur le fallback chiffré :

```bash
export APOLLIA_TOKEN_STORAGE=file
export APOLLIA_TOKEN_PASSPHRASE="phrase-secrète-de-votre-choix"
```

Les tokens sont alors stockés dans `~/.apollia/secrets/` chiffrés avec [age](https://age-encryption.org/) (X25519 symétrique). La passphrase est demandée une seule fois par session — un acteur dédié garde la clé en mémoire pour les appels suivants.

**Attention :** si vous oubliez la passphrase, les tokens sont perdus définitivement. Vous devrez reconnecter chaque compte.

## Multi-comptes

Vous pouvez connecter plusieurs comptes par provider. Chaque token vit dans une entrée keyring distincte avec l'email comme user. L'index `~/.apollia/connectors-index.json` énumère les comptes connectés par provider — le keyring lui-même ne supporte pas l'énumération sur la plupart des OS.

Quand un agent utilise un outil natif (`gmail.send`, `outlook.search`…), il peut passer un paramètre optionnel `account` pour choisir le compte. Sans paramètre, le compte primaire (premier connecté) est utilisé.

## Refresh automatique

Apollia rafraîchit les tokens **proactivement** :

- 60 secondes avant l'expiration, le prochain appel d'outil déclenche un refresh.
- Le refresh utilise le `refresh_token` stocké à la connexion initiale.
- **Singleflight** : si plusieurs appels concurrents déclenchent un refresh sur le même compte, une seule requête HTTP est envoyée — les autres attendent le résultat partagé. Sans cette protection, un burst de 10 appels d'agent ferait 10 requêtes parallèles vers l'AS et déclencherait un rate-limit cascade.

## Révoquer un token

### Côté Apollia (keyring local)

Dans l'UI **Intégrations**, cliquez **Déconnecter** à côté du compte. Le token est immédiatement supprimé du keyring + de l'index. **L'AS distant n'est pas notifié** — le token reste valide côté Google/Microsoft jusqu'à son expiration naturelle (typiquement 1 heure pour l'access token).

En CLI : `apollia auth revoke google --account nidal@example.com`.

### Côté provider (révocation complète)

Pour révoquer immédiatement le token côté Google / Microsoft :

- **Google** : https://myaccount.google.com/permissions → cliquer sur Apollia → **Supprimer l'accès**.
- **Microsoft** : https://myaccount.microsoft.com/consent → trouver Apollia → **Supprimer l'autorisation**.

Cette opération invalide aussi le refresh token, donc même si une copie traîne quelque part, elle devient inutilisable.

## Changer les scopes (rotation)

Si vous voulez augmenter ou réduire les permissions d'un compte déjà connecté :

1. Déconnectez le compte dans **Intégrations**.
2. Reconnectez-le en ajustant les cases à cocher avant de cliquer **Connecter**.

v0.1.0 ne supporte pas le "step-up auth" automatique (re-demander seulement les nouveaux scopes). C'est prévu pour v0.2 (SEP-835).

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
