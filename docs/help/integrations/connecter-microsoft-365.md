# Connecter Microsoft 365

::: warning Section en cours de refonte
Les pages **Intégrations** seront retravaillées après la release v0.1.0 : parcours UX revus, captures d'écran ajoutées, contenu enrichi. Le contenu actuel reste correct sur la mécanique, mais peut diverger de l'UI finale.
:::

> Pour les operators qui veulent donner accès à Outlook (mails + calendrier) et OneDrive à leurs agents — via le connecteur natif Microsoft Graph, avec le périmètre complet en mode gratuit.

## Prérequis

- Un compte Microsoft 365 (professionnel, scolaire, ou personnel) actif.
- Si votre tenant Azure AD exige une approbation administrative, l'administrateur doit pré-approuver l'application Apollia (cas des organisations en *consent admin only*).
- Le profil souveraineté n'est pas réglé sur `local_only`.

Aucune configuration technique préalable n'est requise — l'application OAuth Apollia est multi-tenant et préconfigurée dans l'application.

## Périmètre complet v0.1.0

Contrairement à Google, Microsoft Graph ne requiert pas d'audit CASA — toutes les opérations sont disponibles dès la connexion par défaut.

| Service | Opérations | Scope Graph |
|---|---|---|
| Outlook Mail | `search`, `get`, `send`, `reply`, `list_folders`, `move` | `Mail.Read` + `Mail.Send` |
| Outlook Calendar | `list_events`, `get_event`, `create_event`, `update_event`, `delete_event` | `Calendars.Read` + `Calendars.ReadWrite` |
| OneDrive | `search`, `get_metadata`, `download`, `list_recent` | `Files.Read.All` |

Microsoft Teams (chat + channels) est reporté à v0.2 — l'API channels est plus complexe et hors-scope v0.1.0.

## Étapes

1. Ouvrez **Intégrations** dans Apollia Desktop.

2. Sur la carte **Microsoft 365**, ajustez les permissions si besoin (toutes cochées par défaut).

3. Cliquez **Connecter un compte Microsoft 365**. Une fenêtre navigateur s'ouvre sur l'écran de consentement Microsoft.

4. Si votre administrateur Azure AD a activé le consentement utilisateur, vous validez seul. Sinon, l'administrateur doit pré-approuver l'application au niveau du tenant.

5. Le navigateur affiche un code d'autorisation. Copiez-le et collez-le dans Apollia Desktop, puis **Finaliser la connexion**.

6. Le compte apparaît avec son `userPrincipalName` (généralement votre adresse email pro).

## Vérification

- Sur la carte Microsoft 365, le compte apparaît sous "Comptes connectés" avec votre `userPrincipalName`.
- Le keyring de votre OS contient une entrée `apollia-connector-microsoft`.
- Lancez un agent test qui appelle `outlook.search query:"from:me"` : la requête s'exécute sans approbation HITL (lecture seule) et retourne les premiers mails matchés.

## Multi-tenant

Le connecteur utilise l'endpoint multi-tenant `/common/` par défaut, ce qui permet de connecter n'importe quel compte Microsoft (personnel, scolaire, professionnel) ou Azure AD. Pour un déploiement enterprise restrictif limité à un seul tenant, l'administrateur peut spécifier le tenant cible dans la configuration Apollia (option à venir dans le panneau Paramètres → Souveraineté).

## Approbation HITL

Toutes les opérations d'écriture (envoi, réponse, déplacement, création/modification d'événements, partage OneDrive…) déclenchent une approbation HITL avant exécution. Les lectures (search, get, list) s'exécutent sans demande.

`outlook_cal.delete_event` exige en plus une phrase de confirmation (suppression irréversible).

## Comparaison Google vs Microsoft en mode gratuit

| Capacité | Google v0.1.0 | Microsoft v0.1.0 |
|---|---|---|
| Lire la boîte mail | ❌ (mode expert) | ✅ |
| Rechercher dans les mails | ❌ (mode expert) | ✅ |
| Envoyer des mails | ✅ | ✅ |
| Répondre à un mail | ❌ (mode expert) | ✅ |
| Trier les mails | ❌ (mode expert) | ✅ |
| Lire / écrire l'agenda | ✅ | ✅ |
| Lire/écrire des fichiers | Workspace scopé (`Apollia/<agent>/`) | Drive complet |

Si votre flux principal est Gmail-only et que vous voulez la lecture complète, lisez [Mode expert Google](mode-expert-google-restricted-scopes.md).

## Si ça ne marche pas

- **AADSTS90094 — administrateur consent requis** : votre tenant Azure AD exige une approbation au niveau organisation. Contactez votre administrateur pour qu'il pré-approuve Apollia.
- **AADSTS500011 — application introuvable dans le tenant** : votre administrateur a restreint les apps externes au tenant. Demandez-lui de pré-approuver Apollia, ou utilisez un compte Microsoft personnel.
- **`outlook.send` échoue avec ErrorRecipientNotResolved** : Microsoft Graph valide les destinataires plus strictement que Google. Vérifiez l'adresse cible et l'absence d'alias mort.

## Déconnecter un compte

Sur la carte Microsoft 365, cliquez **Déconnecter** à côté du compte. Le token est supprimé du keyring. Pour révoquer également côté Microsoft, allez sur https://myaccount.microsoft.com et retirez l'autorisation de l'application Apollia.

> **Référence technique :** [Briques-Auth](https://github.com/nidal-z/apollia-os/wiki/Briques-Auth)
