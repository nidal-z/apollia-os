# Connecter Microsoft 365

> **Référence technique :** [Briques-Connectors](https://github.com/nidal-z/apollia-os/wiki/Briques-Connectors)

Le connecteur natif Microsoft 365 couvre Outlook Mail (lecture, recherche, envoi, réponse, gestion de dossiers), Outlook Calendar et OneDrive. Contrairement à Google, Microsoft ne requiert pas d'audit CASA — toutes les opérations sont disponibles dès la connexion par défaut.

## Périmètre complet v0.1.0

| Service | Opérations | Scope Graph |
|---|---|---|
| Outlook Mail | `search`, `get`, `send`, `reply`, `list_folders`, `move` | `Mail.Read` + `Mail.Send` |
| Outlook Calendar | `list_events`, `get_event`, `create_event`, `update_event`, `delete_event` | `Calendars.Read` + `Calendars.ReadWrite` |
| OneDrive | `search`, `get_metadata`, `download`, `list_recent` | `Files.Read.All` |

Microsoft Teams est **reporté à v0.2** — l'API channels/chat est plus complexe et hors-scope v0.1.0.

## Procédure de connexion

1. Ouvrez **Intégrations** dans Apollia Desktop.
2. Sur la carte **Microsoft 365**, ajustez les permissions si besoin (toutes cochées par défaut).
3. Cliquez **Connecter un compte Microsoft 365**. Une fenêtre navigateur s'ouvre sur l'écran de consentement Microsoft.
4. Si votre administrateur Azure AD a activé le consentement utilisateur, vous validez seul. Sinon, l'administrateur doit pré-approuver l'application au niveau du tenant.
5. Le navigateur affiche un code d'autorisation. Copiez-le et collez-le dans Apollia Desktop, puis **Finaliser la connexion**.
6. Le compte apparaît avec son `userPrincipalName` (généralement votre adresse email pro).

## Multi-tenant

Le connecteur utilise l'endpoint multi-tenant `/common/` par défaut, ce qui permet de connecter n'importe quel compte Microsoft (personnel, scolaire, professionnel) ou Azure AD. Pour un déploiement enterprise restrictif limité à un seul tenant, Nidal peut définir la variable `APOLLIA_MICROSOFT_TENANT_ID` dans la configuration.

## Approbation HITL

Toutes les opérations d'écriture (envoi, réponse, déplacement, création/modification d'événements, partage OneDrive…) déclenchent une approbation HITL avant exécution. Les lectures (search, get, list) s'exécutent sans demande.

`outlook_cal.delete_event` exige en plus une phrase de confirmation (suppression irréversible).

## Comparaison Google vs Microsoft

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

## Déconnecter un compte

Sur la carte Microsoft 365, cliquez **Déconnecter** à côté du compte. Le token est supprimé du keyring. Pour révoquer également côté Microsoft, allez sur https://myaccount.microsoft.com et retirez l'autorisation de l'application Apollia.
