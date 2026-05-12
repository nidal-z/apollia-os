# Connecter Google Workspace

> **Référence technique :** [Briques-Connectors](https://github.com/nidal-z/apollia-os/wiki/Briques-Connectors)

Apollia v0.1.0 inclut un connecteur natif Google Workspace couvrant Gmail (envoi + brouillons), Google Calendar (lecture + écriture complète) et Google Drive Workspace (espace scopé par agent).

## Périmètre gratuit v0.1.0

| Service | Opérations disponibles | Scope OAuth |
|---|---|---|
| Gmail | `send`, `compose_draft`, `list_drafts`, `delete_draft` | `gmail.send` + `gmail.compose` |
| Google Calendar | `list_events`, `get_event`, `create_event`, `update_event`, `delete_event` | `calendar.readonly` + `calendar.events` |
| Google Drive | `workspace_list`, `workspace_read`, `workspace_write`, `workspace_delete`, `workspace_share` (dossier `Drive/Apollia/<agent>/`) | `drive.file` |

> **Pourquoi pas la lecture complète de Gmail ?** Les scopes `gmail.readonly`, `gmail.modify` et `drive.readonly` sont classés "restricted" par Google et exigent un audit CASA Tier 2 (~5 000-15 000 $/an). Apollia ne demande pas ces scopes en v0.1.0 — vous pouvez les activer en [mode expert](mode-expert-google-restricted-scopes.md) avec votre propre app OAuth.

## Procédure de connexion

1. Ouvrez **Intégrations** dans Apollia Desktop.
2. Sur la carte **Google Workspace**, ajustez les permissions à demander dans la section "Permissions à demander". Par défaut toutes sont cochées.
3. Cliquez **Connecter un compte Google Workspace**. Une fenêtre navigateur s'ouvre sur l'écran de consentement Google.
4. Choisissez le compte à connecter, validez l'écran de consentement.
5. Le navigateur affiche un code d'autorisation. Copiez-le et collez-le dans le champ qui apparaît dans Apollia Desktop, puis cliquez **Finaliser la connexion**.
6. Le compte apparaît sous "Comptes connectés". L'email est résolu automatiquement via `oauth2/v3/userinfo`.

## Pattern Agent Workspace (Google Drive)

Avec le scope `drive.file`, une app OAuth ne voit que les fichiers qu'elle a créés ou que l'utilisateur a explicitement ouverts avec elle. Apollia exploite ce comportement :

- À la première connexion, un dossier racine `Apollia` est créé à la racine de votre Drive.
- À chaque création de fichier par un agent, un sous-dossier `Apollia/<agent-slug>/` est créé à la demande.
- Toutes les opérations Drive sont **strictement scopées** à ce dossier — l'agent ne peut ni lire ni écrire ailleurs dans votre Drive.

Concrètement, l'agent peut sauvegarder une note `meeting-notes.md` dans son workspace, la relire plus tard, et la supprimer — sans jamais voir le reste de votre Drive.

## Multi-comptes

Vous pouvez répéter la procédure de connexion pour ajouter d'autres comptes (un perso + un pro par exemple). Tous les comptes connectés sont listés sous "Comptes connectés" et chaque outil d'agent prend un paramètre optionnel `account` pour choisir lequel utiliser.

## Déconnecter un compte

Sur la carte Google Workspace, cliquez **Déconnecter** à côté du compte concerné. Le token est immédiatement supprimé du keyring local. Pour révoquer également côté Google, allez sur https://myaccount.google.com/permissions et retirez l'application Apollia.

## Approbation HITL

Toutes les opérations **d'écriture** déclenchent une approbation HITL dans votre boîte de réception avant exécution :

- `gmail.send`, `gmail.compose_draft`, `gmail.delete_draft`
- `gcal.create_event`, `gcal.update_event`
- `gcal.delete_event` (nécessite en plus une phrase de confirmation)
- `gdrive.workspace_write`, `gdrive.workspace_delete`, `gdrive.workspace_share`

Les opérations de **lecture seule** (`gcal.list_events`, `gdrive.workspace_list`, etc.) s'exécutent sans demande.
