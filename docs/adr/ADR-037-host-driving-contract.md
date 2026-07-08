# ADR-037 - Contrat de pilotage partagé pour l'intégration hôte

**Date :** 2026-07-08
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** Pré-implémentation

---

## Contexte

Le positionnement produit retenu est : Apollia est le runtime souverain **embarquable et fédérable** que des éditeurs de produits IT intègrent pour ajouter des agents autonomes auditables, en local. Le sujet central est donc l'intégration : comment une app hôte consomme une instance Apollia.

La cartographie certifiée du 2026-07-08 (source de vérité, vérifiée contre le code) établit que ce positionnement n'a **pas de contrat d'intégration produit**. C'est un trou de packaging, pas de capacité : les briques de valeur sont câblées, mais rien ne permet à un tiers de les piloter proprement.

État vérifié du code :
- L'API `/api/v1` existe (axum, écoute simultanée sur socket Unix et TCP `127.0.0.1:7771`, ~25 modules de routes), mais **sans schéma OpenAPI, sans client typé, sans documentation de contrat**. Les types requête/réponse sont des structs `serde` privés dans chaque `routes_*.rs`.
- Le client de référence (`apollia-cli/src/client.rs`) est **socket-Unix-only sur Unix** et **n'envoie jamais d'en-tête `Authorization`** : il ne peut même pas piloter le TCP authentifié. L'API est architecturée pour l'accès local même-hôte, pas pour un pilotage tiers.
- **Aucun SDK client côté hôte** dans aucun langage. Le SDK Python `sdk/apollia` sert à *écrire* des agents, pas à *piloter* le runtime.
- Auth **incohérente** : le socket Unix n'est jamais authentifié (permissions du système de fichiers uniquement) ; le TCP est protégé par token Bearer par défaut, mais le chemin embarqué force `api_token: None` (`embedded.rs:401`), donc le port TCP est servi sans auth sous Tauri.
- L'exécution des outils MCP par un agent via `ctx.tools.call('mcp:...')` **résout l'outil mais ne l'exécute pas** (chemin ToolProxy AIP non câblé). L'intégration Yumni a dû contourner par un worker REST écrit à la main.

Contrainte : un éditeur hôte peut être écrit dans n'importe quel langage. Le point d'intégration réaliste est donc l'API HTTP, pas l'API Rust in-process. C'est aussi le volet "machine API" du principe 8. Pourquoi maintenant : ce contrat est la clé de voûte du beachhead ; sans lui, "l'intégration est le produit" n'a pas de produit.

## Décision

Nous packageons l'API `/api/v1` en **contrat de pilotage partagé** : un produit stable, typé et documenté, servant à la fois la fédération (pattern Yumni : Apollia pair souverain qui dialogue avec l'hôte) et le pilotage direct. Il comprend quatre composants et une garantie :

1. **Spec OpenAPI générée depuis le code**, via `utoipa` (annotations sur les handlers et les structs de `routes_*.rs`). La spec générée est l'artefact de contrat publié ; elle ne peut pas diverger du code puisqu'elle en dérive.
2. **SDK clients hôte générés depuis l'OpenAPI**, TypeScript et Python en premier (cohérent avec Yumni : serveur MCP Node + director Python). Générés par outillage (par exemple `openapi-typescript` et `openapi-python-client`), pas écrits à la main, pour rester synchronisés avec la spec.
3. **Auth TCP cohérente** : le token Bearer est honoré partout sur TCP, y compris sur le chemin embarqué. Par défaut, l'embarqué ne bind **pas** de port TCP (socket Unix uniquement) ; s'il en bind un, il honore le token. Le socket Unix reste en confiance-locale (permissions FS), documenté comme tel.
4. **Câblage de l'exécution MCP** : le ToolProxy AIP exécute réellement les outils préfixés `mcp:` via l'executor MCP, pour que le pattern de fédération n'exige plus de contournement côté hôte.
5. **Garantie de stabilité** : `/api/v1` devient un contrat versionné. Tout changement cassant passe par `/api/v2`, jamais par une mutation silencieuse de `v1`.

Le périmètre exclut explicitement l'**embedding in-process pur** (extraire `embedded` en crate indépendante de Tauri, API Rust réutilisable) : différé en phase 2, sous un futur ADR.

## Alternatives considérées

### Option A - Fédération uniquement (rejetée)
**Pour :** le plus proche du code réel et de la preuve Yumni ; effort minimal.
**Contre :** trop étroit. Ne sert pas le pilotage direct, ne résout pas l'absence de schéma/SDK/doc, et laisse chaque intégration réinventer un pont one-off (comme le worker REST de Yumni). Ne fait pas de l'intégration un produit réplicable.

### Option B - Embedding in-process pur d'abord (rejetée)
**Pour :** le modèle "embarquer le runtime" le plus pur ; latence nulle.
**Contre :** Rust-only, donc exclut les hôtes TS/Python (dont Yumni). Gros refactor (extraire `embedded` de Tauri, fournir loader PyO3 + backend). Ne répond pas au besoin d'un hôte multi-langage. Reporté en phase 2.

### Option C - Statu quo, HTTP brut non documenté (rejetée)
**Pour :** zéro travail.
**Contre :** ce n'est pas un produit. Chaque intégrateur doit reverse-engineer `routes_*.rs`, l'auth est incohérente et exposée, rien ne garantit la stabilité. C'est exactement le trou actuel.

### Option retenue - Contrat de pilotage partagé
**Pour :** une seule fondation sert les deux modèles (fédération et pilotage) ; l'OpenAPI généré reste synchrone avec le code ; multi-langage ; c'est déjà l'usage réel (Yumni pilote Apollia via HTTP). Rend l'intégration réplicable, donc vendable.
**Compromis acceptés :** engagement de stabilité sur `/api/v1` ; ajout de dépendances de build (utoipa + générateurs).

## Conséquences

**Positives :**
- Le beachhead obtient enfin un produit d'intégration : un éditeur hôte intègre Apollia en TS/Python sans reverse-engineering.
- L'auth cohérente ferme l'exposition "TCP sans auth" du chemin embarqué.
- Le câblage MCP débloque le pattern de fédération sans contournement.
- L'OpenAPI devient aussi la référence de l'API, et nourrit la doc adopters (dérivation arc42 du chantier A).

**Négatives / Compromis :**
- Engagement de stabilité sur `/api/v1` : un changement cassant coûte désormais un `/api/v2` + une migration.
- Nouvelles dépendances de build (utoipa, générateurs OpenAPI) : surface de souveraineté à assumer. Elles sont **build-time uniquement**, pas embarquées au runtime, ce qui les rend acceptables au regard du principe 2.
- Annoter tous les `routes_*.rs` avec utoipa est un travail mécanique large.
- Générer et maintenir deux SDK ajoute de la charge CI.

**Neutres / À surveiller :**
- Le socket Unix reste non authentifié (confiance-locale) : surveiller que les hôtes distants passent bien par TCP + token.
- Langages SDK au-delà de TS/Python (Go, client Rust) à décider selon la demande.
- Cet ADR ne traite pas l'embedding in-process (phase 2) ni les garde-fous budget (chantier B séparé).

## Principes architecturaux impactés

- **Principe #8 - Human CLI, machine API** : renforce le volet "machine API" en le rendant stable, typé et documenté ; c'est sa concrétisation.
- **Principe #2 - Zéro dépendance externe** : les dépendances ajoutées sont build-time et justifiées ici ; l'API servie reste locale (socket Unix / localhost).
- **Principe #4 - Fail fast** : un contrat typé + une auth cohérente font échouer tôt les erreurs d'intégration.

## Liens

- Cartographie (source de vérité) : `docs/internal/cartography/capability-registry.md`, `docs/internal/cartography/business-one-pager.md`
- ADR liés : ADR-016 (secrets, keyring et auth de l'API locale), ADR-017 (client MCP, transport, mode serveur), ADR-024 (contrat runtime du SDK ctx), ADR-020 (architecture desktop / embedded)
- Story associée : à créer (chantier #1)
