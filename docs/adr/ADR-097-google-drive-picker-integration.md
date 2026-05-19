# ADR-097 — Google Drive Picker integration

**Status:** Accepté — 2026-05-18
**Decision-makers:** Nidal
**Related:** ADR-088 (architecture hybride connecteurs natifs + MCP), ADR-090 (Connector trait), ADR-096 (tool execution paths convergence).

## Contexte

Pré-Phase 5, l'accès Apollia à Google Drive était limité au scope OAuth gratuit `drive.file`, qui ne donne accès qu'aux fichiers/dossiers créés par Apollia elle-même. Concrètement :

- Apollia crée un dossier `Drive/Apollia/<agent>/` (chemin configurable)
- L'agent peut écrire et lire dedans
- Tout ce qui est en dehors de ce dossier reste invisible

Limitation UX critique : l'utilisateur ne peut pas faire travailler ses agents sur ses fichiers existants sans les migrer dans le dossier Apollia. Refusé comme défaut acceptable.

Trois chemins pour étendre :

| Option | Détail | Verdict |
|---|---|---|
| **A. Scopes restreints `drive.readonly` / `drive`** | Accès complet au Drive de l'user | ❌ Refusé — audit CASA Tier 2 obligatoire (~5-15k$/an + 4-6 semaines de review Google). Pas viable pour un projet open-source en démarrage |
| **B. Mode Expert** | L'user crée sa propre app Google Cloud avec les scopes restreints en mode Testing (≤100 users) | ✅ Déjà supporté (Settings → Intégrations override client_id/secret). Bypass CASA, mais setup technique inaccessible aux non-tech |
| **C. Google Drive Picker** | Widget JS officiel de Google qui permet à l'user de **désigner** des dossiers/fichiers. Apollia gagne l'accès `drive.file` à chaque dossier picker — pas de CASA, pas de scope restreint | ✅ Sélectionné — UX naturelle (l'user voit son Drive), pas de migration de fichiers requise, pas de setup technique requis |

## Décision

Implémenter **Google Drive Picker** comme mécanisme de premier choix pour étendre l'accès Drive d'Apollia au-delà de son dossier dédié.

### Modèle d'accès

Avec Picker + scope `drive.file` :
- L'user ouvre **Réglages → Intégrations → Google → Sélecteur Google**
- Le widget Google se charge avec le token OAuth d'Apollia + sa clé API Google
- L'user navigue son Drive et **picker** un ou plusieurs dossiers
- Google **étend** automatiquement le scope `drive.file` d'Apollia à chaque dossier picker (et ses descendants)
- Apollia stocke les `folder_id` dans `~/.apollia/drive-prefs.toml`
- Les agents accèdent à n'importe lequel des dossiers picker via des nouvelles ops `gdrive.list_files_in`, `gdrive.write_to_folder`, etc.

L'user peut désigner My Drive root pour un accès quasi-total, ou des dossiers ciblés — son choix.

### Composants livrés

| Composant | Fichier | Rôle |
|---|---|---|
| Storage | `apollia-auth/src/drive_prefs.rs` | `PickedFolder` struct + `add_picked_folder` / `list_picked_folders` / `remove_picked_folder` avec round-trip TOML. 6 tests dédiés |
| API key resolver | `apollia-auth/src/connector_providers.rs` | `resolve_api_key()` avec chaîne env > file > build-time (mêmes 3 sources que client_id/secret). `APOLLIA_BUILD_GOOGLE_API_KEY` + `APOLLIA_GOOGLE_API_KEY` runtime |
| API key storage | `apollia-auth/src/oauth_clients_file.rs` | Champ `api_key` ajouté à `OAuthClientEntry`. `set_api_key` / `lookup_api_key` |
| Connector methods | `apollia-connectors/src/google/drive_workspace.rs` | `list_files_in_folder(folder_id)` + `write_file_in_folder(folder_id, name, content)` — bypass du path-walking pour les dossiers picker |
| Tool ops | `apollia-connectors/src/google/mod.rs` | 4 nouvelles `OperationSpec` : `gdrive.list_picked_folders`, `gdrive.list_files_in`, `gdrive.read_file`, `gdrive.write_to_folder` |
| Runtime bridge | `apollia-runtime/src/connectors_bridge.rs` | Handlers + dispatch + input schemas pour les 4 nouvelles ops. Wrappers `GoogleOpExecutor` étendus aux nouveaux op_ids |
| Tauri commands | `apollia-desktop/src/commands/integrations.rs` | `oauth_google_picker_session` (token + api_key + app_id), `oauth_list_picked_drive_folders`, `oauth_add_picked_drive_folder`, `oauth_remove_picked_drive_folder`, `oauth_set_api_key` |
| UI Picker widget | `apollia-desktop/ui/src/components/integrations/GoogleDrivePicker.svelte` | Charge `apis.google.com/js/api.js` + `picker.js` dynamiquement, construit la `PickerBuilder` avec vues Folders + Shared, persist les picks via Tauri |
| UI Settings | `apollia-desktop/ui/src/routes/settings/Integrations.svelte` | Section "Dossiers ajoutés via le sélecteur Google" par compte, bouton "Ajouter via Google", liste des picks avec bouton Retirer. Champ API key séparé |

### Schéma de stockage

```toml
[google."nidal@example.com"]
folder_path = "Apollia"
[[google."nidal@example.com".picked_folders]]
id = "1aBcDeFgHiJkLmNoPq"
name = "Documents"
mime_type = "application/vnd.google-apps.folder"
[[google."nidal@example.com".picked_folders]]
id = "2xYzAbCdEfGhIjKlMn"
name = "Travail/AI"
mime_type = "application/vnd.google-apps.folder"
```

### Pourquoi Picker fonctionne sans CASA

Google traite explicitement le scope `drive.file` comme "accès aux fichiers que l'app crée OU que l'user ouvre avec l'app via le Picker". Le Picker est le mécanisme d'authorization mediated by user pour étendre `drive.file` à un dossier pré-existant. C'est dans la spec officielle (`developers.google.com/identity/protocols/oauth2/scopes#drivefile`). Pas d'audit requis.

## Alternatives considérées

### Refusé — scopes restreints dans le build officiel
- `drive.readonly` ou `drive` : audit CASA, $$$, délai
- Verdict : refusé pour la viabilité open-source

### Refusé — uniquement Mode Expert
- L'user crée son app, paste son client_id/secret
- Verdict : trop technique pour des non-développeurs, exclut 95% du public cible. Mode Expert reste un échappatoire mais ne doit pas être la voie par défaut

## Conséquences

### Positives

- **Aucune migration de fichiers nécessaire** pour l'user. L'expérience cible "j'ai un dossier `Projets/Q3`, je veux qu'Apollia bosse dedans" devient un clic
- **Pas de CASA, pas de scope restreint** — on reste en free-tier OAuth Google
- **Multi-dossier** : l'user peut désigner autant de dossiers qu'il veut, Apollia accumule
- **Révocable** par l'user via `drive.google.com/drive/u/0/apps` ou via le bouton Retirer dans Apollia

### Négatives / Trade-offs

- **API key Google requise** dans le build officiel — nouvelle valeur à provisionner (`APOLLIA_BUILD_GOOGLE_API_KEY`)
- **Picker JS chargé depuis Google** — dépendance externe au runtime (CDN `apis.google.com`). Tauri webview supporte le chargement par défaut, pas de CSP à ajuster. Si la machine est offline, le Picker échoue avec un message clair
- **Pas de "list every Drive file"** automatique — l'agent ne peut explorer que les dossiers que l'user a explicitement designé. C'est intentionnel (modèle d'authorization mediated by user)

### Risques

- Si Google change le comportement du `drive.file` scope ou du Picker (peu probable), l'integration casse. Mitigation : tests E2E + monitoring du flow

### Modèle de sécurité — bundle des credentials Google

**Question légitime** : on bundle le `client_id`, `client_secret` (Desktop app type) et `api_key` (Picker) dans le binaire Apollia distribué publiquement. N'est-ce pas une fuite de secrets ?

**Réponse** : non, parce qu'aucune de ces valeurs n'est un secret cryptographique au sens habituel. Google documente explicitement chacune comme publique pour les apps natives :

- **`client_secret` Desktop** — Google : *"In this context, the client secret is obviously not treated as a secret"* ([native-app docs](https://developers.google.com/identity/protocols/oauth2/native-app))
- **`api_key`** — Google : *"API keys can be embedded in publicly accessible HTML or client-side code"* ([API keys docs](https://cloud.google.com/docs/authentication/api-keys))
- **`client_id`** — toujours visible sur l'écran de consentement OAuth, identifiant publique par nature

Le modèle de sécurité Google ne repose **pas** sur l'opacité du binaire (n'importe qui peut faire `strings binary | grep apps.googleusercontent.com`). La vraie gate est ailleurs :

| Ce qui protège réellement les données user | Localisation |
|---|---|
| Token OAuth per-user (access + refresh) | OS keychain (macOS Keychain, Linux Secret Service, Windows Credential Manager) — chiffré, jamais dans le binaire |
| Consentement explicite | Écran OAuth Google côté browser, requis par interaction user |
| API key restrictions | Cloud Console — restreinte à Drive + Picker APIs uniquement, autres APIs rejettent la clé |
| App verification status | Affichée par Google sur l'écran de consentement (banner "non vérifiée" en Testing, badge vert quand vérifiée) |

**Risques résiduels et mitigations** :

| Risque | Probabilité | Impact | Mitigation |
|---|---|---|---|
| Quota exhaustion par un scraper | Modéré | Faible (Drive API : milliards de req/jour gratuites) | API restrictions (Drive + Picker only), Cloud Console alerts sur seuils |
| Brand impersonation par un fork | Faible | Modéré pendant Testing, faible après verification | App verification Google (badge officiel sur le consent screen post-launch) |
| Vol de données user | Aucun | — | Données protégées par token OAuth per-user dans le keychain, jamais bundle |

**Pattern industriel** : Claude Desktop, ChatGPT Desktop, Cursor, Windsurf, Continue.dev, Joplin, Standard Notes etc. bundlent tous leurs credentials Google de la même manière. C'est la pratique recommandée par Google pour les apps natives.

**Conclusion** : bundle assumé, documenté comme tel pour ne pas créer de fausse confidentialité, restrictions API en Cloud Console pour limiter le blast radius en cas de scraping.

## Procédure de provisioning (Nidal)

À ajouter à `docs/internal/release/OAUTH-CLIENT-IDS.md` :

1. Google Cloud Console → **APIs & Services** → **Library** → activer **Google Picker API**
2. **Credentials** → **Create credentials** → **API key**
3. Restreindre la clé :
   - **Application restrictions** : HTTP referrers (laissez vide pour Tauri ou ajoutez `tauri://localhost/*`)
   - **API restrictions** : sélectionnez **Google Drive API** + **Google Picker API**
4. Copier la clé (format `AIzaSy...`, ~39 caractères)
5. Stocker dans GitHub Secrets comme `GOOGLE_API_KEY`
6. Au build release :
   ```bash
   export APOLLIA_BUILD_GOOGLE_CLIENT_ID="..."
   export APOLLIA_BUILD_GOOGLE_CLIENT_SECRET="..."
   export APOLLIA_BUILD_GOOGLE_API_KEY="..."
   export APOLLIA_BUILD_MICROSOFT_CLIENT_ID="..."
   cargo build --release -p apollia-desktop
   ```
7. Vérifier : `strings target/release/apollia-desktop | grep -E 'AIzaSy[A-Za-z0-9_-]{33}'`

## Vérification

- `cargo test -p apollia-auth drive_prefs::` — 13/13 ✅ (5 nouveaux tests Picker)
- `cargo check --workspace` — green
- UI typecheck — 0 erreur sur `GoogleDrivePicker.svelte` + `Integrations.svelte`
- Test manuel attendu (après provisioning de la clé API) :
  - Settings → Intégrations → Google → coller la clé API → "Enregistrer la clé"
  - Cliquer "Ajouter via Google" → modal Picker s'ouvre
  - Picker un dossier → confirmation, le dossier apparaît dans la liste
  - En Chat Libre : *"Liste les dossiers que tu peux explorer dans mon Drive"* → l'agent invoque `gdrive.list_picked_folders` → retourne la liste
  - *"Liste les fichiers du dossier Travail"* → `gdrive.list_files_in(folder_id)` → renvoie les fichiers

## Suite

- Étendre le tab "Ce qu'Apollia peut faire" pour montrer "✅ Lire/écrire dans les dossiers Drive picker"
- Documenter dans `docs/help/integrations/google-drive-picker.md` côté help end-user
- Microsoft OneDrive File Picker : pattern équivalent à câbler une fois Microsoft activé
- Optionnel : ajouter `gdrive.update_file_in` pour les éditions in-place (aujourd'hui `write_to_folder` crée toujours un nouveau fichier)
