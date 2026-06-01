# ADR-043 - Décomposition atomique des outils natifs

**Date :** 2026-03-29
**Statut :** Accepté
**Sprint :** 25

---

## Contexte

Apollia OS dispose de 3 outils natifs (bash_executor, file_io, python_executor) dont la scope est trop large. Cette conception a trois conséquences négatives sur les performances agentiques :

1. **Schémas JSON ambigus pour les LLM** - L'outil `file_io` combine trois opérations (read, write, list) en un seul outil. Le LLM doit choisir entre `content`, `dir`, ou `pattern` dans le même schéma, ce qui crée une confusion sémantique. Les benchmarks internes montrent que les agents hésitent et produisent des erreurs de validation (~15% des tool calls échouent à la première tentative).

2. **Aucun outil de recherche** - Pas de grep récursif, pas de glob pattern matching. L'agent est aveugle dans un codebase : pour trouver une fonction, il doit lister les fichiers puis les lire un par un, ou passer par `bash_executor` + `grep -r`, ce qui contourne les gardes-fous sandbox.

3. **Outils manquants critiques** - Pas d'outil HTTP dédié (l'agent doit utiliser `bash_executor` + `curl`, sans validation du `network_allowlist`). Pas d'outil mémoire malgré le Memory Engine FTS5+BM25 existant depuis le Sprint 3.

**Benchmark marché :** Claude Code expose 7 outils file atomiques (Read, Write, Edit, Glob, Grep, + Bash + Agent). Cursor, Cline, et les frameworks LangChain/CrewAI ont tous convergé vers des outils dédiés à action unique. Apollia est structurellement en retard.

**Principe architecturaux concernés :**
- Principe #3 - Contrat minimal : un outil doit représenter une action sémantique claire, pas un namespace d'opérations
- Principe #4 - Fail fast : les schémas JSON ambigus produisent des erreurs au runtime, pas au resolve

## Décision

Nous décomposons les outils monolithiques en outils atomiques avec la règle : **un outil = une action sémantique = un schéma JSON sans ambiguïté**.

Transformation de la surface outil :

| Outil existant | État | Nouveaux outils |
|---|---|---|
| `file_io` | Déprécié | `file_read`, `file_write`, `file_edit`, `file_list` |
| *(absent)* | - | `file_glob`, `file_grep` |
| `bash_executor` | Conservé | - |
| `python_executor` | Conservé | - |
| *(absent)* | - | `http_fetch` |
| *(absent)* | - | `memory_search` |

**Total :** 10 outils actifs (8 nouveaux + 2 conservés).

### Détails techniques

- **file_read** : Lit un fichier avec offset/limit optionnels, retourne le contenu UTF-8 avec numéros de ligne.
- **file_write** : Écrit un fichier complet, écrase si existant.
- **file_edit** : Modification diff-based (old_text → new_text), avec option `replace_all` pour renommages.
- **file_list** : Liste un répertoire (non-récursif), retourne les entrées avec métadonnées.
- **file_glob** : Recherche récursive par pattern (`**/*.rs`), utilise la crate `glob`.
- **file_grep** : Recherche par regex dans les fichiers, supporte context lines (-A/-B/-C) et glob filter, implémentation native Rust (crate `regex`, pas de subprocess ripgrep).
- **http_fetch** : Requête HTTP GET/POST avec validation `network_allowlist` du manifest agent AVANT l'appel réseau. SandboxProfile `NetworkRestricted`.
- **memory_search** : Wrapper autour de `apollia-memory::MemorySearch`, expose FTS5 BM25 avec paramètres (namespace, query, limit, sources, min_importance).

**Dépréciation de file_io :**
- Le code reste dans `apollia-tools/src/tools/file_io.rs` avec un tag `#[deprecated]`
- Supprimé de la liste des outils enregistrés dans `apollia-runtime::Supervisor::native_tool_descriptors()`
- Les agents existants qui le référencent reçoivent un warning au resolve : "file_io is deprecated, use file_read/file_write/file_edit/file_list"

## Alternatives considérées

### Option A - Garder file_io et améliorer la description (rejetée)

**Pour :** Zéro changement structurel, compatibilité totale.

**Contre :** Le problème est dans le schéma JSON lui-même, pas dans la description. Un seul outil avec 3 modes (`"operation": "read"|"write"|"list"`) reste ambigu pour le LLM. La description peut expliquer les modes, mais le schéma reste un point de friction. Rejetée car ne résout pas le problème structurel.

### Option B - Ajouter des exemples dans les descriptions (rejetée)

**Pour :** Facile, aide le LLM par few-shot.

**Contre :** Les descriptions d'outils sont déjà longues (~200 tokens). Ajouter des exemples pour chaque mode alourdit le context window sans garantir que le LLM choisisse correctement. Le schéma ambigu reste. Rejetée car approche cosmétique qui ne change pas la structure.

### Option C - Supprimer complètement file_io sans dépréciation (rejetée)

**Pour :** Nettoyage complet du code, pas de dette technique.

**Contre :** Breaking change pour les agents existants qui référencent `file_io` dans leur code. La dépréciation offre une période de migration explicite et un warning clair. Rejetée en faveur d'une migration progressive.

### Option retenue - Outils atomiques avec dépréciation de file_io

**Pour :**
- Schémas JSON sans ambiguïté : un schéma = une action
- Meilleure performance LLM : less confusion, better tool selection
- Surface d'API complète : recherche (glob/grep), HTTP, mémoire
- Migration progressive : file_io déprécié mais fonctionnel
- Alignement marché : même philosophie que Claude Code, Cursor, Cline

**Compromis acceptés :**
- Plus d'outils à maintenir (10 vs 3) - mitigé par le fait que chaque outil est plus simple
- Migration nécessaire pour les agents existants référençant file_io - mitigé par la dépréciation progressive avec warning

## Conséquences

**Positives :**
- Réduction des erreurs de validation des tool calls (observé ~15% → ~2% dans les benchmarks internes sur 50 conversations test)
- Surface d'API complète : les agents peuvent naviguer un codebase (glob/grep), communiquer avec des APIs (http_fetch), exploiter la mémoire locale (memory_search)
- Meilleure isolation sandbox : http_fetch valide le `network_allowlist` AVANT l'appel réseau, contrairement à bash_executor + curl
- Schémas JSON autodocumentés : chaque outil a un schéma clair, les descriptions sont plus courtes et ciblées

**Négatives / Compromis :**
- 10 outils à maintenir vs 3 - chaque outil est cependant plus simple (~150 lignes vs ~400 lignes pour file_io)
- Migration nécessaire pour agents existants - le warning au resolve guide les développeurs

**À surveiller :**
- **Adoption par les agents** : vérifier dans les 3 mois que les agents utilisent bien les nouveaux outils et ne contournent pas avec bash_executor
- **Performance registry** : 10 outils enregistrés vs 3 - impact sur le resolve ? (attendu négligeable, la ToolRegistry est en HashMap, O(1))
- **Cohérence des descriptions** : maintenir une voix homogène dans les descriptions des 10 outils pour éviter la confusion

## Principes architecturaux impactés

- **Principe #3 - Contrat minimal** : renforcé - un outil = une action sémantique claire, pas un namespace d'opérations
- **Principe #4 - Fail fast** : renforcé - schémas JSON sans ambiguïté, erreurs de validation détectées au resolve plutôt qu'au runtime
- **Principe #1 - Local-first** : étendu - memory_search expose la mémoire locale FTS5 comme outil first-class

## Liens

- Story associée : STORY-310
- Spec de référence : `docs/specs/sprint-25-spec.md`
- Stories d'implémentation : STORY-312 (file_read), STORY-313 (file_write), STORY-314 (file_edit), STORY-315 (file_list), STORY-316 (file_glob), STORY-317 (file_grep), STORY-318 (http_fetch), STORY-319 (memory_search)
- ADR connexe : ADR-010 (Tool Registry - architecture de base)
