# ADR-045 — Page Intégrations : wizard générique piloté par les metadata MCP Registry

**Date :** 2026-08-01
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 27

---

## Contexte

Apollia OS dispose depuis le Sprint 26 d'un client MCP complet avec API REST CRUD
(`/mcp/servers`). Les agents peuvent déjà consommer des serveurs MCP ; mais l'utilisateur
doit éditer `~/.apollia/mcp.toml` manuellement ou passer par `curl` pour configurer ces
serveurs. Cette friction est un bloquant pour les opérateurs non-techniques.

Le Sprint 27 ajoute une page **Intégrations** dans le desktop Apollia. Cette page doit :

1. Permettre de **découvrir** les serveurs MCP disponibles (catalogue)
2. Permettre de les **configurer** sans jamais toucher à `mcp.toml`
3. Stocker les **secrets** (tokens API) de façon sécurisée dans le keychain OS
4. Afficher des **disclaimers de sécurité** pour les serveurs non vérifiés

La question centrale est : comment implémenter le wizard de configuration ?

**Contraintes structurantes :**

1. **Principe #1 — Local-first** : les secrets ne quittent pas la machine ; le wizard doit
   fonctionner hors ligne (cache local du registry).
2. **Principe #2 — Zéro dépendance externe** : le binaire ne doit pas nécessiter d'accès
   réseau obligatoire pour fonctionner.
3. **Échelle** : le MCP Registry référence aujourd'hui 16 000+ serveurs. Un composant
   spécifique par connecteur est impossible à maintenir.
4. **Maintenabilité** : l'équipe est de taille solo ; l'ajout d'un nouveau connecteur ne
   doit pas nécessiter de modification du code frontend.

## Décision

Nous adoptons un **wizard générique piloté par les metadata du MCP Registry officiel**
(`registry.modelcontextprotocol.io/v0.1/servers`), complété par des **enrichissements
builtin** pour les connecteurs les plus courants.

Le wizard est un composant Svelte unique (`ConnectorWizard`) qui lit les champs requis
(auth type, paramètres) depuis les metadata du serveur sélectionné et génère
dynamiquement ses étapes. Aucun code spécifique par connecteur n'existe dans le frontend.

Les secrets collectés sont stockés dans le keychain OS via la crate `keyring` et
référencés dans `mcp.toml` par le préfixe `APOLLIA_SECRET:<service>/<key>`.

## Alternatives considérées

### Option A — Wizard par connecteur (rejetée)

Un composant Svelte dédié par service (ex. `NotionWizard.svelte`, `SlackWizard.svelte`).

**Pour :**
- UX sur-mesure par service (champs adaptés, liens vers la doc officielle, validation
  spécifique).
- Pas de dépendance à un format de metadata externe.

**Contre :**
- **Développement infini** : 16 000+ serveurs MCP sur le registry. Même les 10 connecteurs
  les plus populaires représentent 3-5 semaines de travail initial, puis une maintenance
  permanente à chaque changement d'API.
- **Effet de seuil** : tout connecteur non implémenté reste inaccessible depuis l'UI,
  réintroduisant la friction du `mcp.toml` manuel.
- Viole implicitement le Principe #2 : chaque wizard embarque une connaissance "codée en
  dur" d'une API tierce.

Rejetée : coût de maintenance incompatible avec une équipe solo.

### Option B — Pas de wizard, édition assistée de mcp.toml (rejetée)

Afficher un éditeur TOML enrichi (coloration syntaxique, validation de schéma) dans l'UI.

**Pour :**
- Zéro code à maintenir côté wizard.
- Flexibilité totale pour l'utilisateur.

**Contre :**
- **Inaccessible aux opérateurs non-techniques** : l'utilisateur cible (Operator mode)
  n'est pas développeur et ne connaît pas le format TOML ni les paramètres de
  configuration des serveurs MCP.
- Pas de découverte : l'utilisateur doit déjà connaître le nom et la configuration du
  serveur qu'il veut ajouter.
- Pas de gestion sécurisée des secrets : les tokens seraient écrits en clair dans `mcp.toml`.

Rejetée : ne résout pas le problème d'accessibilité qui motive ce sprint.

### Option C — Wizard générique piloté par metadata + enrichissements builtin (retenue)

Un seul composant wizard lit les metadata du MCP Registry et affiche les champs
dynamiquement. Pour les 6 connecteurs les plus courants (Notion, Slack, GitHub, Linear,
PostgreSQL, Filesystem), des enrichissements builtin fournissent des labels UX améliorés,
des liens vers la documentation officielle, et des valeurs par défaut pertinentes.

**Pour :**
- **Scalable** : tout nouveau serveur du MCP Registry est immédiatement configurable
  sans modification du code.
- **Local-first** : le registry est mis en cache localement ; le wizard fonctionne
  hors ligne avec le cache.
- **Sécurité** : les secrets transitent exclusivement par `keyring` ; jamais écrits en
  clair dans `mcp.toml`.
- **Maintenable** : l'ajout d'un enrichissement builtin est une entrée JSON dans un
  fichier de configuration, pas un nouveau composant Svelte.

**Compromis acceptés :**
- L'UX du wizard dépend de la qualité des metadata du MCP Registry. Un serveur sans
  description claire aura un wizard moins guidant.
- Les enrichissements builtin nécessitent une mise à jour manuelle si l'API d'un
  connecteur change (acceptable pour 6 connecteurs cibles initiaux).
- La dépendance réseau vers `registry.modelcontextprotocol.io` doit être gérée
  explicitement (timeout, fallback sur cache, mode offline).

## Conséquences

**Positives :**
- Un utilisateur Operator peut ajouter Notion, Slack ou tout autre serveur MCP en
  3 minutes sans ouvrir un terminal.
- L'ajout d'un nouveau connecteur au catalogue ne nécessite aucune modification du
  code Apollia — uniquement une entrée dans le MCP Registry officiel.
- Les secrets sont isolés dans le keychain OS ; `mcp.toml` ne contient jamais de
  valeurs sensibles en clair.
- Le mode offline est garanti par le cache local du registry (TTL 24h par défaut).

**Négatives / Compromis :**
- La crate `keyring` ajoute une dépendance OS : sur Linux, elle requiert le service
  D-Bus `org.freedesktop.secrets` (absent des environnements headless). Un fallback
  vers un fichier chiffré local est prévu comme limitation V1 documentée.
- Le format des metadata du MCP Registry peut évoluer ; la crate `apollia-desktop`
  devra être mise à jour pour suivre les breaking changes du schéma.
- Le wizard générique ne peut pas valider la sémantique des paramètres (ex. vérifier
  qu'un token Notion a bien les bonnes permissions) ; seul le test de connexion
  (`/mcp/servers/:id/test`) peut le détecter.

**À surveiller :**
- **Qualité des metadata du MCP Registry** : si le registry officiel est peu renseigné,
  envisager un registry Apollia complémentaire pour les enrichissements communautaires.
- **Évolution du schéma registry** : surveiller les breaking changes de l'API
  `registry.modelcontextprotocol.io/v0.1` dans les sprints suivants.
- **Adoption du secret store sur Linux** : mesurer le taux d'échec du keyring sur les
  distributions headless ; envisager le fallback chiffré si le taux dépasse 5 %.
- **Temps de réponse du wizard** : si le chargement des metadata depuis le cache est
  perceptible (> 200 ms), précharger le catalogue en arrière-plan au démarrage du desktop.

## Principes architecturaux impactés

- **Principe #1 — Local-first** : respecté — secrets dans le keychain OS (jamais dans
  le cloud), cache local du registry, mode offline garanti.
- **Principe #2 — Zéro dépendance externe** : respecté en mode offline grâce au cache ;
  la dépendance réseau vers le registry est optionnelle et à la seule initiative de
  l'utilisateur (rafraîchissement du catalogue).
- **Principe #4 — Fail fast** : le wizard teste la connexion au serveur MCP (step 4 sur 5)
  avant de confirmer l'ajout ; les erreurs de configuration sont détectées immédiatement.
- **Principe #8 — CLI humaine, API machine** : le wizard délègue toutes les mutations
  à l'API REST existante (`POST /mcp/servers`, `DELETE /mcp/servers/:id`) ; aucune logique
  métier n'est dupliquée côté frontend.

## Liens

- ADR précédent sur MCP : ADR-044 (client MCP, architecture et transport)
- ADR connexe : ADR-027 (desktop Tauri — processus unique, runtime embarqué)
- ADR connexe : ADR-028 (frontend Svelte, UX-first)
- Stories : STORY-345 (cette ADR), STORY-346 (registry client), STORY-347 (secret store),
  STORY-349 (resolve_env APOLLIA_SECRET:), STORY-357 (ConnectorWizard)
- Spec de référence : `docs/specs/sprint-27-spec.md` section 1
