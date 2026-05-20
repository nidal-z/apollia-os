# Connecter Google Workspace

::: warning Section en cours de refonte
Les pages **Intégrations** seront retravaillées après la release v0.1.0 : parcours UX revus, captures d'écran ajoutées, contenu enrichi. Le contenu actuel reste correct sur la mécanique, mais peut diverger de l'UI finale.
:::

> Pour les operators qui veulent donner accès à Gmail, Google Calendar et Google Drive à leurs agents — via le connecteur natif d'Apollia, sans configuration manuelle d'API.

## Prérequis

- Un compte Google Workspace ou un compte Google personnel actif.
- Le profil souveraineté n'est pas réglé sur `local_only` (sinon les connecteurs cloud sont désactivés).

Aucune configuration technique préalable n'est requise — l'application OAuth est fournie par Apollia et préconfigurée dans l'application.

## Périmètre gratuit v0.1.0

| Service | Opérations disponibles | Scope OAuth |
|---|---|---|
| Gmail | `send`, `compose_draft`, `list_drafts`, `delete_draft` | `gmail.send` + `gmail.compose` |
| Google Calendar | `list_events`, `get_event`, `create_event`, `update_event`, `delete_event` | `calendar.readonly` + `calendar.events` |
| Google Drive | `workspace_list`, `workspace_read`, `workspace_write`, `workspace_delete`, `workspace_share` (dossier `Drive/Apollia/<agent>/`) | `drive.file` |

La lecture complète Gmail / Drive nécessite des scopes Google *restricted* (audit CASA Tier 2) — disponible en *mode expert* avec votre propre app OAuth. Voir [Mode expert Google](mode-expert-google-restricted-scopes.md).

## Étapes

1. Ouvrez **Intégrations** dans Apollia Desktop.

2. Sur la carte **Google Workspace**, ajustez les permissions à demander dans la section "Permissions à demander". Par défaut toutes les permissions free-tier sont cochées.

3. Cliquez **Connecter un compte Google Workspace**. Une fenêtre navigateur s'ouvre sur l'écran de consentement Google.

4. Choisissez le compte à connecter, validez l'écran de consentement.

5. Le navigateur affiche un code d'autorisation. Copiez-le et collez-le dans le champ qui apparaît dans Apollia Desktop, puis cliquez **Finaliser la connexion**.

6. Le compte apparaît sous "Comptes connectés". L'email est résolu automatiquement via l'endpoint `oauth2/v3/userinfo`.

## Pattern Agent Workspace (Google Drive)

Avec le scope `drive.file`, une app OAuth ne voit que les fichiers qu'elle a créés ou que l'utilisateur a explicitement ouverts avec elle. Apollia exploite ce comportement :

- À la première connexion, un dossier racine `Apollia` est créé à la racine de votre Drive.
- À chaque création de fichier par un agent, un sous-dossier `Apollia/<agent-slug>/` est créé à la demande.
- Toutes les opérations Drive sont **strictement scopées** à ce dossier — l'agent ne peut ni lire ni écrire ailleurs dans votre Drive.

Concrètement, l'agent peut sauvegarder une note `meeting-notes.md` dans son workspace, la relire plus tard, et la supprimer — sans jamais voir le reste de votre Drive.

## Vérification

- Sur la carte Google Workspace, le compte apparaît sous "Comptes connectés" avec votre adresse email exacte.
- Le keyring de votre OS contient une entrée `apollia-connector-google` (visible dans **Keychain Access** sur macOS, **Gestionnaire d'identifiants** sur Windows).
- Lancez un agent test qui appelle `gmail.send` : une approbation HITL apparaît dans la boîte de réception ; après approbation, le mail est effectivement envoyé.

## Multi-comptes

Vous pouvez répéter la procédure pour ajouter d'autres comptes (un perso + un pro par exemple). Tous les comptes connectés sont listés sous "Comptes connectés" et chaque outil d'agent prend un paramètre optionnel `account` pour choisir lequel utiliser.

## Approbation HITL

Toutes les opérations **d'écriture** déclenchent une approbation HITL dans votre boîte de réception avant exécution :

- `gmail.send`, `gmail.compose_draft`, `gmail.delete_draft`
- `gcal.create_event`, `gcal.update_event`
- `gcal.delete_event` (nécessite en plus une phrase de confirmation)
- `gdrive.workspace_write`, `gdrive.workspace_delete`, `gdrive.workspace_share`

Les opérations de **lecture seule** (`gcal.list_events`, `gdrive.workspace_list`, etc.) s'exécutent sans demande.

## Si ça ne marche pas

- **L'écran de consentement Google affiche un "App non vérifiée"** : si vous êtes en *mode expert* avec votre propre app OAuth, ajoutez votre email comme test user dans la console Google Cloud → OAuth consent screen.
- **Le compte connecté ne reçoit pas les mails envoyés par l'agent** : le mail est peut-être dans le dossier *Spam* du destinataire — Google filtre plus durement les mails envoyés via l'API qu'un envoi humain. Surveillez aussi l'erreur `gmail.send` retournée dans la timeline.
- **`Connecter un compte` est désactivé** : le profil souveraineté est réglé sur `local_only`. Changez-le dans **Paramètres → Souveraineté** ou revenez plus tard.
- **L'agent tente une opération restricted (`gmail.search`, `gmail.get`)** et reçoit `ScopeMissing` : ces scopes ne sont pas disponibles en mode gratuit. Voir [Mode expert Google](mode-expert-google-restricted-scopes.md) pour les activer avec votre propre app OAuth.

## Déconnecter un compte

Sur la carte Google Workspace, cliquez **Déconnecter** à côté du compte concerné. Le token est immédiatement supprimé du keyring local. Pour révoquer également côté Google, allez sur https://myaccount.google.com/permissions et retirez l'application Apollia.

> **Référence technique :** [Briques-Auth](https://github.com/nidal-z/apollia-os/wiki/Briques-Auth)
