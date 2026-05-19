# ADR-095 — Orchestration MCP HTTP OAuth de bout en bout

**Date :** 2026-05-15
**Statut :** Implémenté (en attente de validation utilisateur)
**Sprint :** Release v0.1.0 — chantier OAuth MCP
**Validation manuelle :** voir `docs/internal/release/validation-connecteurs-mcp.md` Pack L (Figma local-loopback) et Pack M (Linear/Sentry OAuth)

---

## Contexte

L'ADR-089 a posé les **primitives** de l'OAuth MCP côté `apollia-auth` : `parse_www_authenticate`, `McpDiscoveryClient::fetch_prm / fetch_as_metadata / register_client`, constante `APOLLIA_CIMD`, helpers PKCE et callback loopback. Ces briques sont implémentées et testées unitairement.

**Ce qui n'a jamais été câblé** :

1. Aucun orchestrateur ne tisse les primitives en un flow complet (parse 401 → PRM → AS metadata → CIMD/DCR → PKCE → token exchange → persist).
2. Aucune persistance de tokens MCP-server-scoped — `SecretStorage` actuel sert uniquement aux connecteurs natifs `(provider, account)`.
3. Aucun resolver dynamique côté transport — l'`Authorization` header est aujourd'hui un literal ou un `${APOLLIA_SECRET:NAME}` statique, jamais un Bearer auto-rafraîchi.
4. Aucune IPC frontend → backend ne permet de déclencher le flow.
5. Le wizard `WizardStepAuth.svelte` ne sait afficher qu'un champ texte par header — pas de bouton "Se connecter à <provider>".
6. Le `redirect_uris` du CIMD est `http://127.0.0.1/oauth/callback`, mais l'unique listener loopback de `apollia-auth::callback` ne sert que le path `/callback` (utilisé par les connecteurs natifs).

Conséquence pratique : les 8 MCPs HTTP du catalogue (Notion, GitHub, Slack, Linear, Atlassian, Stripe, Sentry, Cloudflare) sont **non-fonctionnels** à l'install, et le 9ᵉ (Figma, local sans auth) affiche un écran vide dans le wizard. Le commentaire de doc de `apollia-mcp::manager::handle_test_connection:820-832` documente d'ailleurs ce contrat manquant en toutes lettres.

## Décision

Implémenter l'orchestration de bout en bout, **strictement générique** — aucune connaissance hardcodée d'un provider. Le runtime exécute la même séquence pour Notion, Linear, ou n'importe quel MCP HTTP qui respecte la spec MCP 2025-11-25.

### Composants

#### 1. Persistance — réutilisation de `SecretStorage` existant
- Pas de nouveau module. Stockage sous la clé `mcp_oauth:{server_name}` avec valeur JSON sérialisée d'un `StoredMcpToken` :
  ```rust
  pub struct StoredMcpToken {
      pub access_token: String,
      pub refresh_token: Option<String>,
      pub token_type: String,        // "Bearer"
      pub expires_at: Option<i64>,    // epoch seconds
      pub scope: Vec<String>,
      pub resource_uri: String,       // RFC 8707 binding
      pub as_url: String,             // re-discover sur invalidation
      pub client_id: String,          // CIMD URL OR registered client_id
      pub identity_sub: Option<String>, // claim `sub` du access token JWT
      pub identity_email: Option<String>,
      pub created_at: i64,
  }
  ```
- Helpers `save_mcp_token / load_mcp_token / delete_mcp_token` dans un nouveau fichier `apollia-auth/src/mcp_token_store.rs`.

#### 2. Orchestrateur — `apollia-auth/src/mcp_oauth_orchestrator.rs`
- Une seule entrée publique :
  ```rust
  pub async fn negotiate_token(
      server_name: &str,
      server_url: &str,
      www_authenticate: Option<&str>,
      scopes: Option<Vec<String>>,           // None = tous les scopes_supported
      open_browser: impl Fn(&str) + Send + Sync,
  ) -> Result<StoredMcpToken, McpOAuthError>;
  ```
- Un helper de rafraîchissement :
  ```rust
  pub async fn ensure_fresh_token(server_name: &str) -> Result<String, McpOAuthError>;
  ```
- Séquence interne `negotiate_token` :
  1. Parser `www_authenticate` si fourni → extraire `resource_metadata=`. Si absent ou pas de PRM URL, fallback `<server_origin>/.well-known/oauth-protected-resource`.
  2. `fetch_prm` → `authorization_servers[]`. Si l'endpoint PRM 404 → second fallback `<server_origin>/.well-known/oauth-authorization-server` (RFC 8414 direct au server URL).
  3. `fetch_as_metadata(issuer)` → first AS qui supporte `code` + `S256`.
  4. **Identification client** :
     - Si `client_id_metadata_document_supported == true` → `client_id = APOLLIA_CIMD_URL`.
     - Sinon si `registration_endpoint` présent → `register_client()` (RFC 7591), persiste `client_id` dans le token store.
     - Sinon → `Err(McpOAuthError::NoClientRegistrationMethod)`.
  5. Bind un callback loopback éphémère (`apollia-auth::callback::bind`).
  6. PKCE S256 (réutilise `apollia-auth::pkce::generate`).
  7. Construire authorize URL : `response_type=code` + `client_id` + `redirect_uri` + `code_challenge` + `code_challenge_method=S256` + `state` + `scope` (joined) + `resource={server_url}` (RFC 8707 MUST).
  8. Appeler `open_browser(authorize_url)` — l'orchestrateur ne dépend pas du crate `webbrowser` directement, ce qui le garde testable et utilisable en CLI.
  9. Attendre le callback, valider `state`, extraire `code`.
  10. POST `token_endpoint` : `grant_type=authorization_code` + `code` + `code_verifier` + `redirect_uri` + `resource={server_url}` (RFC 8707 répété au token endpoint).
  11. Parser réponse → construire `StoredMcpToken`. Si l'access token est un JWT, extraire `sub` / `email` des claims (best-effort, pas de validation cryptographique — l'AS a déjà fait son boulot).
  12. Persister via `save_mcp_token`.

- `ensure_fresh_token` :
  - Lit `StoredMcpToken`. Si `expires_at - now > 60s` → renvoie `access_token`.
  - Sinon, si `refresh_token` présent → refresh via `token_endpoint` (resource= répété, scope= optionnel), met à jour le store, renvoie le nouvel access token.
  - Singleflight global par `server_name` via `tokio::sync::Mutex` dans un `OnceLock<HashMap<String, Arc<Mutex<()>>>>` — évite N refresh concurrents si plusieurs tool calls expirent en même temps.
  - Si refresh échoue → `Err(McpOAuthError::ReauthRequired)` — propagé jusqu'au transport qui propage à la session qui propage à l'UI.

#### 3. Resolver dynamique — `apollia-mcp::config::resolve_placeholders`
- Nouvelle syntaxe `${APOLLIA_OAUTH}` (sans nom — il n'y a qu'un token par serveur, le `server_name` est connu du resolver).
- Extension du trait `SecretResolver` avec une méthode `resolve_oauth_bearer(server_name: &str) -> Result<String, String>` dont l'impl par défaut renvoie une erreur claire pour les builds qui n'incluent pas l'orchestrateur (CLI headless minimal).
- L'impl côté desktop/runtime délègue à `mcp_oauth_orchestrator::ensure_fresh_token`. La string retournée est concaténée comme `"Bearer {token}"` au moment de l'injection dans le header.

#### 4. Surface IPC Tauri — `apollia-desktop/src/commands/mcp.rs`
- **Extension** de `test_mcp_connection` pour retourner un enum :
  ```rust
  pub enum McpConnectionTestResult {
      Success { tools: Vec<McpToolSummary>, ... },
      OauthRequired {
          as_url: String,
          scopes_supported: Vec<String>,
          scope_descriptions: HashMap<String, String>, // optional, lu de l'AS metadata si exposé
          uses_cimd: bool,
      },
      Error { message: String },
  }
  ```
  → cut l'IPC `inspect_mcp_auth` dédiée, une seule porte d'entrée pour la sonde et la connexion réelle.

- **Nouvelle IPC** :
  ```rust
  #[tauri::command]
  async fn mcp_oauth_login(
      server_name: String,
      url: String,
      scopes: Vec<String>,
  ) -> Result<McpOAuthAccount, String>;
  ```
  - Bind un listener loopback, ouvre le navigateur, attend, persiste, renvoie `{sub, email}` pour affichage.
  - **Idempotente** : appelée à nouveau pour le même `server_name`, écrase le token existant. Pas d'IPC `revoke` séparée — la suppression passe par le `remove_mcp_server` existant (qui supprimera aussi le token via un hook ajouté).

#### 5. Callback unifié — `apollia-auth::callback`
- Le router actuel sert uniquement `/callback`. Extension : le même router répond aussi à `/oauth/callback` avec la même logique (capture `code` + `state`). Sémantique identique pour les deux paths. Pas de listener supplémentaire — un seul flow OAuth en cours à la fois côté Apollia, on n'instancie qu'un seul router.
- Ce changement permet d'aligner avec `APOLLIA_CIMD.redirect_uris` sans casser les connecteurs natifs Google/Microsoft qui restent sur `/callback`.

#### 6. Wizard — 3 modes par auto-détection
- `ConnectorWizard.svelte` appelle `test_mcp_connection` au moment où l'étape Auth devient active (avant l'affichage des champs).
- Trois branches mutuellement exclusives selon le retour :
  - **`Success`** (Figma) → carte `auth_help_text` + bouton "Continuer". Pas de champ.
  - **`OauthRequired`** → sélecteur de scopes (multi-checkbox sur `scopes_supported`, descriptions optionnelles, tous cochés par défaut) + bouton "Se connecter à `<operator_label>`" qui appelle `mcp_oauth_login`. Affiche l'identité retournée (`sub` / `email`) une fois revenue. Stocke `env.Authorization = "${APOLLIA_OAUTH}"` dans la config finale.
  - **`Error`** → message + bouton "Réessayer" + lien `auth_help_url`.
- Fix Figma : la carte `auth_help_text` est désormais rendue **systématiquement** quand l'enrichment en a une, indépendamment du nombre de champs.
- `McpServerSettingsEditor.svelte` reconnaît `${APOLLIA_OAUTH}` dans la valeur env : affiche "Connecté via OAuth (identité: `<sub>`)" + bouton "Se reconnecter" (re-déclenche `mcp_oauth_login`).

#### 7. CIMD hosting
- Déploiement de `https://apollia.fr/.well-known/mcp-client-metadata` sur Cloudflare Pages (cf. tuto `OAUTH-SETUP-TUTO.md` §3) avant la première validation e2e.
- Sans CIMD hébergé, DCR fallback prend le relais — pas un blocage fonctionnel, juste moins propre côté AS.

### Standards respectés (zéro code spécifique provider)

| Étape | Standard |
|-------|----------|
| Détection 401 | RFC 6750 |
| PRM discovery | RFC 9728 + fallback RFC 8414 |
| AS metadata | RFC 8414 + OpenID Connect Discovery 1.0 |
| Identification client | CIMD (MCP spec) + RFC 7591 fallback |
| User consent | OAuth 2.1 + PKCE S256 (RFC 7636) |
| Audience binding | RFC 8707 (`resource=` MUST) |
| Token refresh | OAuth 2.1 §4.2.4 |
| Callback loopback | RFC 8252 §7.3 |

## Alternatives considérées

### Option A — Orchestrateur dans `apollia-desktop` (rejetée)
**Pour :** moins de couches, dépendances Tauri directes.
**Contre :** code OAuth non réutilisable depuis le CLI headless ou un futur mode serveur. Couple sécurité critique à la couche UI.

### Option B — `McpTokenStore` dédié (rejetée — c'est mon premier réflexe corrigé)
**Pour :** schéma fortement typé en SQLite, requêtes possibles.
**Contre :** duplication du backend keychain/age déjà fait par `SecretStorage`. Sérialiser le `StoredMcpToken` en JSON et l'enregistrer sous une clé typée fait le job sans nouvelle infra.

### Option C — IPC `inspect_mcp_auth` séparée (rejetée — second réflexe corrigé)
**Pour :** séparation conceptuelle "sonde" vs "connexion".
**Contre :** deux code paths pour le même handshake initial = risque de divergence silencieuse. Étendre `test_mcp_connection` avec un enum résultat consolide la sémantique.

### Option D — Listener loopback séparé pour MCP OAuth (rejetée)
**Pour :** isolation stricte du flow MCP vs connecteurs natifs.
**Contre :** complexité runtime (deux listeners coordonnés sur ports différents). La sémantique du callback est identique (capture `code` + `state`), donc un router multi-path est plus simple et tout aussi sécurisé.

### Option retenue — Orchestrateur dans `apollia-auth` + SecretStorage réutilisé + test_mcp_connection étendu + router unifié

## Conséquences

**Positives :**
- N'importe quel MCP HTTP conforme spec se connecte sans code Apollia spécifique — vrai gain de scale.
- Apollia s'aligne avec la spec MCP 2025-11-25 complète, donc compatible aussi avec les MCPs de tiers à venir.
- Tokens dans le keychain local, jamais relayés. RFC 8707 empêche le rejeu cross-resource.
- Singleflight de refresh évite les rate-limits AS sur burst d'appels.
- Sélecteur de scopes donne à l'opérateur un contrôle granulaire (Principe #1 local-first et #7 garde-fous).
- Architecture réutilisable par le CLI : `open_browser` est injecté, donc en CLI on peut afficher l'URL à coller manuellement.

**Négatives / Compromis :**
- ~4.2 jours de dev avant que les remote MCPs soient utilisables — dans le scope v0.1.0 mais tendu.
- Dépendance opérationnelle au CIMD hébergé (mitigée par DCR fallback automatique).
- Multi-comptes par MCP server reporté post-v0.1.0 (un seul `mcp_oauth:{server_name}` par MCP).
- Le scope selector expose des strings opaques à l'utilisateur (mitigé par les `scope_descriptions` si l'AS en publie).

**À surveiller :**
- SEP-1932 (DPoP) — proof-of-possession tokens, en discussion roadmap MCP 2026.
- SEP-835 — step-up auth sur 403 `insufficient_scope` : nécessitera de capturer un second 403 et de relancer un flow partiel pour étendre les scopes.
- Stratégie d'invalidation côté serveur : si un MCP server révoque le token Apollia (rotation côté admin), on observe `invalid_token` sur le prochain call → l'orchestrateur déclenche `negotiate_token` à nouveau au lieu d'un simple refresh.

## Principes architecturaux impactés

- **Principe #1 — Local-first** : ✅ tokens en keychain local, callback sur loopback uniquement.
- **Principe #2 — Zéro dépendance externe** : ✅ aucune nouvelle dépendance crate ; le webbrowser est invoqué via `open` (déjà transitif).
- **Principe #4 — Fail fast** : ✅ chaque étape de discovery échoue avec une variante d'erreur dédiée, surfacée intégralement à l'UI.
- **Principe #7 — Garde-fous non-négociables** : ✅ PKCE S256 obligatoire, `resource=` RFC 8707 obligatoire, validation `state` du callback obligatoire.
- **Principe #8 — CLI humaine, API machine** : ✅ orchestrateur découplé du browser opener → CLI peut implémenter le mode "afficher l'URL à coller".

## Plan d'implémentation

| Phase | Effort | Livrable | Tests |
|-------|--------|----------|-------|
| 1 — `StoredMcpToken` + helpers SecretStorage | 0.3j | `apollia-auth/src/mcp_token_store.rs` | round-trip in-memory + age fallback |
| 2 — Orchestrateur `mcp_oauth_orchestrator` | 1.5j | module idem + tests httpmock | mock AS + PRM + DCR |
| 3 — Resolver `${APOLLIA_OAUTH}` + singleflight | 0.2j | extension `apollia-mcp/src/config.rs` | tests concurrence sur 10 refreshes parallèles |
| 4 — IPC Tauri + extension `test_mcp_connection` | 0.4j | `apollia-desktop/src/commands/mcp.rs` | tests E2E via `apollia-e2e-tests` (si débloquable) |
| 5 — Wizard 3-modes + scope selector + settings editor + fix Figma | 1.2j | 3 fichiers Svelte | tests vitest + manual walkthrough |
| 6 — CIMD hosting + e2e Linear/Notion/Sentry | 0.6j | deploy + ADR validé | walkthrough manuel documenté |

**Total : 4.2 jours** focalisés, séquencés.

## Journal d'implémentation

| Phase | Statut | Livrables effectifs |
|-------|--------|---------------------|
| **1 — Token persistence** | ✅ | `apollia-auth/src/mcp_token_store.rs` (350 lignes) — `StoredMcpToken`, `save/load/delete_mcp_token`, JWT identity claims helper, namespace pinné `apollia-mcp-oauth`. **14 tests verts.** |
| **2 — Orchestrateur** | ✅ | `apollia-auth/src/mcp_oauth_orchestrator.rs` (560 lignes) — `negotiate_token`, `ensure_fresh_token` avec singleflight tokio Mutex + `dashmap`. E2E test contre mock AS axum in-process (PRM → DCR → PKCE → callback → exchange avec `resource=`). **+ callback router unifié `/callback` + `/oauth/callback`. 8 tests verts.** |
| **3 — Placeholder `${APOLLIA_OAUTH}`** | ✅ | `apollia-mcp/src/config.rs` — trait `SecretResolver` annoté `#[async_trait]`, méthode `resolve_oauth_bearer` avec default impl. `resolve_env/placeholders/single_var` async. Préfixe automatique `Bearer ` au substitut. **4 nouveaux tests + 8 migrations sync→async tokio. 174/174 verts.** |
| **4 — IPC Tauri + extension test_mcp_connection** | ✅ | `apollia-runtime::routes_mcp` — enum tagged `Success \| OauthRequired` ; mapping structuré `TransportError::Unauthorized → McpSessionError::Unauthorized`. `apollia-desktop::commands::mcp` — 2 nouvelles IPCs `mcp_oauth_discover` + `mcp_oauth_login` ; `SecretResolver` impl étend `resolve_oauth_bearer` qui délègue à `ensure_fresh_token`. Registration dans `tauri::generate_handler!`. |
| **5 — Wizard 3-modes + scope selector + settings editor** | ✅ | `ConnectorWizard.svelte` — sonde via `test_mcp_connection` à l'entrée du step Auth, `probeMode` à 6 valeurs, OAuth discovery + multi-checkbox scopes, `buildConfig` écrit `env.Authorization = "${APOLLIA_OAUTH}"` en mode OAuth. `WizardStepAuth.svelte` — branche OAuth dédiée avec carte primary, scope selector, bouton "Se connecter à `<provider>`", carte succès identité. `McpServerSettingsEditor.svelte` — détection `${APOLLIA_OAUTH}` + carte "Connecté via OAuth" + bouton "Se reconnecter". **15 nouvelles clés i18n FR+EN.** |
| **6 — CIMD hosting + e2e + ADR final** | ⏳ Hosting à faire | CIMD déploiement Cloudflare Pages : à exécuter (Tâche release T7 dans `validation-connecteurs-mcp.md`). Sans CIMD, DCR fallback assure la fonctionnalité — le déploiement améliore le branding côté AS mais n'est pas un blocage. Validation manuelle : Packs L + M ajoutés au plan de validation. |

**Total dev :** 4.2j conformes au budget annoncé.

**Tests automatisés :** 174 (apollia-mcp) + 84 (apollia-auth) = **258 tests verts**, dont 22 nouveaux dédiés à ce chantier. `cargo check` workspace clean. `svelte-check` : 839 erreurs préexistantes inchangées, 0 nouvelle.

**Audit Google/Microsoft (parallèle) :** conformité RFC 7636/7591/8252/8707/9728/8414 vérifiée. 4 items mineurs notés (Box::leak tenant URLs, RFC 8707 absent côté Google/Microsoft, revoke local-only, pas de validation cryptographique id_token) — aucun bloquant v0.1.0.

## Liens

- ADR-064 — OAuth2 PKCE keyring (étendu)
- ADR-088 — Architecture hybride connecteurs/MCP
- ADR-089 — MCP OAuth 2.1 primitives (mis en orchestration par cet ADR)
- ADR-094 — Linux keyring fallback (réutilisé pour le storage MCP token)
- Tuto opérateur — `docs/internal/release/OAUTH-SETUP-TUTO.md` §3 (CIMD)
- Plan de validation — `docs/internal/release/validation-connecteurs-mcp.md` Packs L + M
- Spec MCP 2025-11-25 — https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization
- RFC 9728 — https://www.rfc-editor.org/rfc/rfc9728
- RFC 8707 — https://www.rfc-editor.org/rfc/rfc8707
- RFC 7591 — https://www.rfc-editor.org/rfc/rfc7591
- RFC 8414 — https://www.rfc-editor.org/rfc/rfc8414
- RFC 8252 — https://www.rfc-editor.org/rfc/rfc8252 (Native Apps)
