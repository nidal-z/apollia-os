# ADR-082 — Tool Governance : DB unifiée, scopes HITL, ToolRegistry runtime

**Date :** 2026-04-26
**Statut :** Accepté
**Sprint :** 43

> **Note de numérotation :** la story STORY-530 référence cet ADR sous le numéro
> 076, mais ADR-076 a été attribué entre-temps à `i18n-frontend`. Le présent
> document conserve donc le numéro libre suivant disponible (082) tout en
> remplissant le rôle décrit dans la story.

---

## Contexte

Trois symptômes convergents ont motivé une refonte de la couche gouvernance
au sprint 43 :

- l'agent **veille-ia** simulait silencieusement ses résultats web : le
  backend Brave échouait en HTTP 401 à la première requête, sans fallback
  DuckDuckGo, et l'erreur ne remontait pas jusqu'à l'opérateur ;
- le bouton **« Toujours autoriser »** du dialog HITL ajoutait toujours
  des règles `scope = global` quel que soit le scope cliqué : le
  paramètre `scope` envoyé par l'IPC était ignoré côté Rust ;
- aucun mécanisme runtime n'existait pour activer/désactiver un outil
  natif. Désactiver `bash_executor` exigeait de recompiler.

À cela s'ajoutait la dispersion du stockage de gouvernance : `permissions.db`
contenait les règles et l'audit, mais les credentials de tools (clé Brave,
tokens HTTP) n'avaient aucun emplacement officiel — chaque outil les lisait
ad-hoc dans l'environnement.

## Décision

**Nous adoptons une base SQLite unique `~/.apollia/governance.db`** comme
source de vérité pour les règles de permissions, l'audit log immuable, l'état
enabled/disabled des outils et leurs credentials chiffrées AES-256-GCM. Trois
scopes HITL explicites — `session`, `project`, `global` — sont propagés
fidèlement de l'UI au moteur d'évaluation.

### 1. Une seule DB : `~/.apollia/governance.db`

Consolidation de l'ancienne `permissions.db` avec deux tables nouvelles :

- `tools (name PK, enabled, config_json, updated_at)` ;
- `tool_credentials (tool_name, key_name, value_encrypted, created_at, last_used_at)`.

La migration est transparente : si `governance.db` n'existe pas mais
`permissions.db` est présente, le fichier est copié puis renommé en
`permissions.db.bak`. La sauvegarde est conservée. La migration de schéma
est idempotente (ALTER TABLE conditionnels).

### 2. Trois scopes HITL explicites

- **Session** : `Vec<PrefixRule>` en mémoire dans `PermissionEngine`,
  jamais écrit en DB. Disparaît à l'arrêt du process.
- **Project** : persisté en DB, filtré par `project_path` canonique.
- **Global** : persisté en DB, sans filtrage de chemin.

`PrefixRuleEngine::check_with_scope` évalue les trois tiers dans l'ordre
session → project → global et retient la règle au préfixe le plus long
dans chaque tier. La commande Tauri `add_permission_prefix_rule` route
désormais le scope reçu vers la bonne API (`add_session_rule` /
`prefix_rules.add_rule`).

### 3. ToolRegistry : enable/disable runtime sans recompiler

`build_native_dispatcher` consulte `ToolRegistry::list()` au démarrage et
exclut tout outil avec `enabled = FALSE`. Un outil absent de la table
reste actif par défaut (`is_enabled` retourne `true` quand la ligne
n'existe pas), ce qui évite une migration explicite à l'introduction d'un
nouvel outil natif.

### 4. Chiffrement credentials : AES-256-GCM + `.keyfile` (chmod 600)

`ToolCredentialStore` chiffre chaque valeur avec AES-256-GCM, nonce de
12 octets généré par insertion et préfixé au ciphertext en base. La clé
maître de 32 octets vit dans `~/.apollia/.keyfile` (mode `0o600`),
créé au premier lancement. Pas de keychain OS pour le sprint 43 : le coût
d'implémentation × 3 OS est hors budget, et la threat model courante
(attaquant sans accès shell à la machine) est suffisamment couverte par
le chmod restrictif.

### 5. `web_search` : DuckDuckGo en premier, fallback sur Brave 401

DuckDuckGo est désormais ajouté inconditionnellement en tête de la liste
de backends. Brave n'est inséré qu'après lui, et seulement si la clé
résout à une valeur non-vide (credential store > variable
d'environnement). Un backend qui échoue (401, captcha, timeout) cède la
main au suivant via `WebSearchError::AllBackendsFailed` — la dégradation
gracieuse est préférée à un échec total silencieux.

## Alternatives considérées

### Multi-DB : `governance.db` + `permissions.db` séparés (rejetée)
**Pour :** isolation par responsabilité, schémas indépendants.
**Contre :** double chemin de configuration, double migration, double
backup à gérer ; la cohérence transactionnelle entre règles et audit est
plus simple sous une seule connexion.

### Keychain OS pour les credentials dès le sprint 43 (rejetée)
**Pour :** stockage idiomatique sur macOS (Keychain) / Linux (Secret
Service) / Windows (Credential Manager).
**Contre :** trois implémentations à maintenir, friction CI (headless),
hors budget sprint. Reporté à un sprint dédié.

### Brave en premier même sans clé (rejetée)
**Pour :** alignement avec la doc Brave (premium-first).
**Contre :** c'était la cause racine du bug initial : un Brave 401
plantait l'outil entier au lieu de céder à DDG.

### Option retenue — DB unique + scopes explicites + DDG-first
**Pour :** chemin de configuration unique, comportement HITL prévisible,
veille-ia fonctionnelle sans configuration.
**Compromis acceptés :** la base est plus large (4 tables vs 2) et les
sauvegardes utilisateur doivent intégrer un seul fichier de plus
(`governance.db` + `.keyfile`).

## Conséquences

**Positives :**
- Veille-ia retrouve une recherche web réelle (DDG) sans configuration.
- Les boutons « Toujours autoriser pour ce projet / cette session / partout »
  ont enfin la sémantique annoncée par leur libellé.
- L'opérateur peut désactiver `bash_executor` à chaud depuis la CLI ou
  l'UI desktop sans recompilation.
- Les credentials ne fuitent plus en clair sur disque.

**Négatives / Compromis :**
- Le `.keyfile` ajoute un fichier sensible à protéger : sa perte rend
  inaccessible les credentials chiffrées (pas de récupération possible).
- AES-256-GCM avec clé locale n'est pas équivalent à un keychain OS
  contre un attaquant qui obtient un shell utilisateur.

**À surveiller :**
- Volumétrie de l'audit log : append-only, jamais purgé ; prévoir un job
  de rétention si la base dépasse plusieurs centaines de Mo.
- Migration vers le keychain OS : repérer les régressions UX (déblocage
  systémique au lancement) avant de planifier le sprint suivant.

## Principes architecturaux impactés

- **Principe #1 — Local-first** : credentials et règles restent sur la
  machine, chiffrement local, aucune fuite réseau ajoutée.
- **Principe #4 — Fail fast** : `try_with_default_backends(require_brave)`
  remonte une erreur au boot lorsque Brave est obligatoire et non
  configuré, plutôt qu'à la première requête de l'agent.
- **Principe #5 — Un acteur, une responsabilité** : `ToolRegistry`,
  `ToolCredentialStore`, `PrefixRuleEngine` et `PermissionAuditLog`
  partagent la même base mais possèdent chacun leur propre connexion
  SQLite, sans état partagé inter-acteurs.
- **Principe #7 — Garde-fous non-négociables** : les triggers SQLite
  `no_update_audit` / `no_delete_audit` rendent l'audit log
  cryptographiquement append-only au niveau du moteur, indépendamment
  du code applicatif.
- **Principe #8 — CLI humaine, API machine** : `apollia tools` et
  `apollia permissions` exposent les mêmes capacités que l'UI desktop,
  avec sortie `--json` pour l'automatisation.

## Liens

- Story associée : STORY-530
- Stories sprint 43 : STORY-520 → STORY-529
- Spec de référence : `docs/internal/specs/tool-governance-spec.md`
- ADR précédent sur `web_search` : ADR-072
