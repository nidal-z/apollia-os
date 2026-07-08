# Mode expert Google, scopes restricted

> Pour les power users qui veulent activer la lecture complète de Gmail ou Drive avec leur propre app OAuth Google Cloud, au lieu de l'app par défaut Apollia.

## Pourquoi ce mode existe

Les scopes Google classés *restricted* (`gmail.readonly`, `gmail.modify`, `drive.readonly`, `drive`) exigent un audit CASA Tier 2 par un tiers agréé Google (5 000 à 15 000 dollars par an). Pour rester gratuit, l'app Apollia par défaut ne les demande pas, le périmètre reste limité aux scopes *sensitive* et *non sensitive*.

Si vous voulez la lecture complète de Gmail, vous pouvez créer votre propre app OAuth Google Cloud, garder le statut **Testing** (jusqu'à 100 test users), et la brancher à Apollia via une variable d'environnement. Aucun coût.

Ce mode s'adresse aux power users familiers avec Google Cloud Console. Si vous ne l'êtes pas, restez sur le périmètre Tier 1 par défaut, qui couvre déjà l'envoi, la composition, le calendrier complet et le Drive Workspace scopé.

## Procédure résumée

1. **Google Cloud Console** : créer un projet, activer Gmail / Calendar / Drive APIs, configurer OAuth consent screen en mode External + Testing, ajouter votre email comme test user, ajouter les scopes restricted souhaités, créer un OAuth client Desktop, noter le **Client ID**.
2. **Apollia** : exporter la variable d'environnement avant de lancer Apollia :
   ```bash
   export APOLLIA_GOOGLE_CLIENT_ID="123456789-abcdef.apps.googleusercontent.com"
   ```
3. **Reconnecter Google** dans Apollia. L'écran de consentement affichera **votre** app et proposera les scopes restricted.

Les outils restricted dédiés (`gmail.search`, `gmail.get`, `gmail.reply`, `gmail.list_labels`, `gdrive.search`, `gdrive.list_all`) sont à venir : ils seront détectés automatiquement via les `granted_scopes` du token une fois livrés. En attendant, Expert Mode sert surtout à brancher votre propre client OAuth, sans passer par l'app partagée Apollia.

## Vérification

- L'écran de consentement Google affiche le nom de votre app, pas "Apollia OS".
- Sous "Comptes connectés", les `granted_scopes` listés incluent le scope restricted accordé.
- Les outils qui exploitent ces scopes restricted étant à venir, la vérification se limite pour l'instant au nom de l'app et aux `granted_scopes`.

## Si ça ne marche pas

- **L'écran affiche encore "Apollia OS"** : la variable d'environnement n'est pas prise en compte par le processus qui a lancé Apollia. Relancez Apollia depuis le shell où vous avez fait `export`, ou ajoutez la variable à votre `~/.zshrc` ou `~/.bashrc`.
- **Google refuse les scopes** : votre app est encore en attente côté Google. Restez en mode **Testing** et ajoutez-vous comme test user.
- **Les outils restricted n'apparaissent pas** : reconnectez le compte après modification des scopes, les anciens tokens ne contiennent pas les nouveaux scopes.

## Responsabilité

En mode expert, vous êtes responsable de votre app OAuth. Apollia n'audite pas cette configuration. Si vous distribuez Apollia avec votre Client ID embarqué pour plus de 100 utilisateurs, Google exigera CASA Tier 2.

## Alternative

Si Google Cloud Console vous semble lourd, vous pouvez installer un serveur MCP Gmail communautaire (cherchez `mcp-server-gmail` sur npm ou GitHub). Le serveur tourne localement, utilise vos credentials Google, et expose les outils Gmail via MCP. Voir [Câbler son propre serveur MCP](cabler-son-propre-serveur-mcp.md).

> **Référence technique :** [Référence Apollia](../../reference/index.md) , procédure détaillée Google Cloud Console étape par étape, gestion des scopes, responsabilité CASA.
