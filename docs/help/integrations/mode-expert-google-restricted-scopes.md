# Mode expert Google — scopes restricted

> Pour les power users qui veulent activer la lecture complète de Gmail / Drive avec leur propre app OAuth Google Cloud, sans dépendre de l'app par défaut d'Apollia.

## Pourquoi ce mode existe

Apollia v0.1.0 ne demande pas les scopes Google classés *restricted* (`gmail.readonly`, `gmail.modify`, `drive.readonly`, `drive`) car ils exigent un audit CASA Tier 2 par un tiers agréé Google (5 000 à 15 000 $/an). Pour rester gratuit côté Apollia et côté utilisateur, le périmètre par défaut se limite aux scopes *sensitive* et *non-sensitive*.

**Si vous voulez la lecture complète de votre Gmail**, vous pouvez créer votre propre app OAuth Google Cloud et la brancher à Apollia. Vous prenez alors la responsabilité de votre app (CASA *single user* ou *internal use* — beaucoup plus léger que CASA Tier 2 multi-tenant).

Ce mode s'adresse aux power users / développeurs. Si vous n'êtes pas familier avec Google Cloud Console, restez sur le périmètre par défaut + Outlook côté Microsoft.

## Prérequis

- Un compte Google Cloud (gratuit) avec un projet dédié.
- Familiarité avec OAuth, scopes Google API, et Google Cloud Console.
- Acceptation de votre responsabilité CASA pour votre propre app.

## Étapes

### 1. Créer une app OAuth Google Cloud

1. Allez sur [Google Cloud Console](https://console.cloud.google.com/).
2. Créez un projet dédié (`apollia-personal` par exemple).
3. Dans **APIs & Services → Library**, activez : **Gmail API**, **Google Calendar API**, **Google Drive API**.
4. Dans **APIs & Services → OAuth consent screen** :
   - Type d'utilisateur : **External**
   - Statut de publication : **Testing** (pas besoin de Production tant que vous êtes seul utilisateur, jusqu'à 100 utilisateurs test).
   - Ajoutez-vous comme **Test user**.
5. Dans **APIs & Services → Credentials**, créez des identifiants **OAuth client ID** :
   - Type d'application : **Desktop app**
   - Nom : `Apollia (mon app)`
6. Notez le **Client ID** affiché — vous l'utiliserez à l'étape 3. Aucun client secret n'est requis (PKCE).
7. Toujours dans **OAuth consent screen → Scopes**, ajoutez explicitement les scopes restricted dont vous avez besoin :
   - `https://www.googleapis.com/auth/gmail.readonly` (lire la boîte mail)
   - `https://www.googleapis.com/auth/gmail.modify` (modifier les labels)
   - `https://www.googleapis.com/auth/drive.readonly` (lire tout le Drive)
   - `https://www.googleapis.com/auth/drive` (lire/écrire tout le Drive)
8. Renseignez la justification CASA *single user / internal use* demandée par Google. En mode Testing avec moins de 100 utilisateurs, Google n'exige pas l'audit Tier 2.

### 2. Configurer la déclaration CASA *single user*

Pour les apps en statut Testing avec scopes restricted, Google demande de remplir un formulaire de déclaration CASA simplifié dans **OAuth consent screen → Verification status**. C'est gratuit et automatique tant que vous restez le seul utilisateur de votre propre app.

Si vous passez en Production (>100 utilisateurs ou app distribuée), Google exigera CASA Tier 2 ou Tier 3 selon le scope. Restez en Testing pour votre usage personnel.

### 3. Brancher l'app à Apollia

Définissez la variable d'environnement `APOLLIA_GOOGLE_CLIENT_ID` avec votre Client ID :

```bash
# Dans ~/.zshrc / ~/.bashrc
export APOLLIA_GOOGLE_CLIENT_ID="123456789-abcdef.apps.googleusercontent.com"
```

Redémarrez Apollia Desktop. Lors de la prochaine connexion Google depuis **Intégrations**, l'écran de consentement affichera votre app (`apollia-personal`) au lieu de l'app Apollia officielle, et proposera les scopes restricted ajoutés.

### 4. Activer les outils restricted dans les agents

Une fois connecté avec les scopes restricted, les outils suivants apparaissent automatiquement dans le registry (la détection se fait au runtime à partir des `granted_scopes` du token) :

- `gmail.search` — recherche dans la boîte mail
- `gmail.get` — lire un message
- `gmail.reply` — répondre à un message
- `gmail.list_labels`, `gmail.modify_labels` — gestion des labels
- `gdrive.search` — recherche full text dans Drive
- `gdrive.list_all` — liste tous les fichiers accessibles

Ces outils sont absents tant que `APOLLIA_GOOGLE_CLIENT_ID` n'est pas configuré.

## Vérification

- L'écran de consentement Google affiche le nom de **votre** app (et pas "Apollia OS").
- Après connexion, sous "Comptes connectés", les `granted_scopes` listés incluent au moins un scope restricted (`gmail.readonly` par exemple).
- Lancez un agent test qui appelle `gmail.search query:"is:unread"` — résultat non vide si vous avez des mails non lus.

## Responsabilité

En mode expert, **vous êtes responsable** de votre app OAuth Google. Apollia ne supervise ni n'audite cette configuration. Concrètement :

- Vos tokens restent dans votre keyring local — pas de changement.
- Si Google révoque votre app pour usage abusif, c'est votre app qui est touchée, pas celle d'Apollia.
- Si vous distribuez Apollia avec votre Client ID embarqué, vous devez passer en Production + CASA Tier 2 (5 000-15 000 $/an).

Pour la majorité des power users qui veulent juste l'inbox de leur propre compte, le mode Testing single-user est largement suffisant et gratuit.

## Si ça ne marche pas

- **L'écran de consentement affiche encore "Apollia OS"** : `APOLLIA_GOOGLE_CLIENT_ID` n'est pas pris en compte. Vérifiez que la variable est bien set dans le shell qui lance Apollia Desktop (relancez l'app depuis ce shell après `export`).
- **Google refuse les scopes restricted** : votre app est encore en attente de validation côté Google. Restez en mode Testing et ajoutez-vous explicitement comme test user.
- **Les outils `gmail.search` / `gdrive.search` n'apparaissent pas** : reconnectez le compte après modification des scopes côté Google Cloud Console — les anciens tokens ne contiennent pas les nouveaux scopes.

## Alternative : MCP Gmail communautaire

Plutôt que de créer votre propre app Google, vous pouvez installer un serveur MCP Gmail communautaire (recherchez `mcp-server-gmail` sur npm ou GitHub). Le serveur tourne localement, utilise vos credentials Google, et expose les outils Gmail via le protocole MCP. Apollia consomme le serveur via le wizard "Ajouter un serveur MCP".

Cette voie est légèrement plus lourde (un processus de plus à gérer) mais évite de toucher à Google Cloud Console.

> **Référence technique :** [Briques-Auth](https://github.com/nidal-z/apollia-os/wiki/Briques-Auth)
