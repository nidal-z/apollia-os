# ADR-110 - Commande `apollia inspect <agent.py>`

**Date :** 2026-05-19
**Statut :** Accepté
**Sprint :** Phase release v0.1.0 (refonte SDK)

---

## Contexte

Aujourd'hui, valider qu'un agent Python est conforme au runtime Apollia
nécessite de **le démarrer pour de vrai** via `apollia agent start
<id>` ou `apollia chat-libre <id>`. Si le manifeste est mal formé, si
une signature de skill est incohérente, si une datasource déclarée
n'existe pas, l'erreur n'apparaît qu'au premier appel - ou pire,
silencieusement dégrade le comportement.

**État observé au 2026-05-19** :

- Aucun outil "inspect" en CLI. L'auteur d'agent doit faire un cycle
  complet : install → start → invoke skill → lire les logs Rust.
- Le manifeste TOML (s'il existe) n'est validé qu'au boot du runtime,
  pas avant.
- Les skills A2A déclarés ne sont visibles qu'après démarrage (via
  `ctx.a2a.list_skills` côté agent ou commandes Tauri).
- Les datasources et templates déclarés (ADR-103) ne sont vérifiés
  qu'à la première utilisation.
- Les secrets déclarés (ADR-104) ne sont vérifiés qu'à la lecture.
- Erreurs typiques masquées : skill_id en doublon, signature
  `@skill` avec type non-mappable (ADR-099), datasource manquante,
  template Jinja2 syntax error.

Or grâce au design decorator-first (ADR-098) et signature inference
(ADR-099), tout est **statiquement introspectable** au load Python :
charger le module, accéder à `module.agent.__apollia_manifest__`,
parcourir les skills, valider les schemas. Pas besoin de démarrer le
runtime Rust.

Cette propriété est un cadeau du nouveau design. Il faut l'exploiter
via un outil CLI dédié.

## Décision

**Nous adoptons une commande `apollia inspect <chemin_agent>` qui
charge le module Python en isolation (sans démarrer le bridge Rust ni
le runtime), introspecte `agent.__apollia_manifest__`, et affiche un
rapport complet : manifest, skills (id + description + JSON Schema
input/output), packages requis, datasources/templates/secrets déclarés,
permissions tools, warnings et erreurs. Sortie human-readable par
défaut, `--json` pour pipeline.**

Surface CLI :

```bash
$ apollia inspect agents/veille-ia/workers/web-search-worker.py

✓ Module loaded: agents.veille-ia.workers.web-search-worker
✓ @agent class: WebSearchWorker (v2.0.0)

Manifest:
  name:         veille-ia.web-search
  version:      2.0.0
  description:  Recherche web multi-source avec déduplication
  packages:     [] (stdlib only)

Skills (3):
  ├── search
  │   description:  Recherche web globale
  │   input:        {query: str, max_results: int = 10, lang: str = "fr"}
  │   output:       {results: list[dict], count: int}
  ├── deep_search
  │   description:  Recherche multi-passes avec re-ranking
  │   input:        {query: str, depth: int = 3}
  │   output:       {results: list[dict], passes: int}
  └── extract_url
      description:  Extraction de contenu d'une URL
      input:        {url: str}
      output:       {title: str, text: str, lang: str}

Datasources (2):
  ✓ sources       (datasources/sources.yaml - 203 entrées)
  ✗ topics        MISSING (datasources/topics.yaml introuvable)

Templates (1):
  ✓ search-report  (templates/search-report.md.j2)

Secrets (1):
  ⚠ brave_api_key  declared, not yet configured
                   (run: apollia tools config brave_api_key=...)

Permissions:
  tools.allow: ["web_search", "file_read", "http_request"]

Warnings (1):
  - Skill "deep_search" has untyped `dict` return - consider TypedDict
    for clearer client schemas.

Errors (1):
  ✗ Datasource "topics" declared in manifest but file missing.
    Path: agents/veille-ia/datasources/topics.yaml

✗ Inspection failed (1 error)
```

Détails techniques :

1. **Chargement isolé** - le module Python est chargé via `importlib.
   util.spec_from_file_location()` dans un subprocess Python pur (ou
   via un mode CLI Rust qui invoque Python sans le bridge complet). Le
   décorateur `@agent` (ADR-107) instancie l'agent ; `apollia inspect`
   accède à `module.agent.__apollia_manifest__`.
2. **Sans runtime** - pas d'EventBus, pas d'acteurs, pas d'API HTTP.
   `ctx` n'est jamais instancié. Le PyO3 bridge n'est pas chargé.
   Inspection pure = lecture statique.
3. **Validation systématique** :
   - Skill_id unicité.
   - Signatures (`@skill`, `@on_message`, `@orchestrated`) inférables
     en JSON Schema (ADR-099) - sinon erreur.
   - Datasources YAML existent ET parsent (validation `serde_yaml`).
   - Templates Jinja2 existent (pas validation syntax - laissé à
     `minijinja` au load runtime).
   - Secrets déclarés vs configurés dans le store local (warning si
     non-configurés).
   - Tools déclarés dans permissions vs catalogue tools natif.
4. **Code retour** :
   - `0` = OK (peut avoir warnings).
   - `1` = inspection error (manifest invalide).
   - `2` = arg/file error (chemin invalide, etc.).
5. **Output formats** :
   - Default : human-readable (avec couleurs si TTY).
   - `--json` : sortie JSON structurée pour pipelines/IDE/Tauri UI.
   - `--quiet` : seulement erreurs/warnings.
6. **Use cases** :
   - Pre-commit hook : refuser un commit si `apollia inspect` fail.
   - CI : `apollia inspect agents/*/workers/*.py` dans la pipeline.
   - Dev quotidien : feedback rapide avant `apollia agent install`.
   - UI desktop : `Install Package Dialog` peut afficher le rapport
     d'inspection avant install.

## Alternatives considérées

### Option A - Valider seulement au boot runtime (statu quo) (rejetée)

**Pour :** zéro outil supplémentaire.
**Contre :** cycle de feedback lent (~5-10s pour démarrer). Erreurs
fragmentées dans les logs Rust. Pas exploitable en pre-commit/CI.

### Option B - Outil externe `apollia-lint` séparé (rejetée)

**Pour :** modulaire.
**Contre :** dupplique la logique d'introspection. Maintenance
parallèle. Moins discoverable que `apollia inspect`.

### Option C - Web UI desktop affichant l'inspection (rejetée)

**Pour :** beau visuellement.
**Contre :** ne sert pas le CI/pre-commit. Auteur en terminal n'a rien.

### Option retenue - Commande CLI `apollia inspect` first-class

**Pour :** intégrée à la CLI existante (cohérence ADR-x sur CLI),
exécutable en CI/pre-commit/dev/IDE, output JSON pour intégration tools
tiers, feedback rapide (<1s typique).
**Compromis acceptés :** l'inspection ne détecte pas les erreurs
runtime (ex. token API expiré, datasource invalide à mid-execution).
Couvre le statique seulement - clairement documenté.

## Conséquences

**Positives :**

- Feedback < 1s pour valider la conformité d'un agent - vs ~5-10s du
  cycle "install + start + invoke".
- Pre-commit hook trivial à ajouter dans le repo et chez les
  utilisateurs : refuse un agent mal formé avant qu'il n'arrive en
  prod.
- CI propre : `apollia inspect agents/*/**/*.py` dans `.github/
  workflows`.
- Onboarding builder : un nouvel auteur lance `apollia inspect` après
  chaque modif et apprend par feedback rapide.
- Implémente concrètement le **principe #4 - Fail fast** au niveau
  ergonomique.
- Synergique avec ADR-098 (decorator-first introspectable), ADR-099
  (signature inference génère le schéma), ADR-103/104 (gating
  vérifiable statiquement), ADR-110 (skills A2A listables).

**Négatives / Compromis :**

- Implémentation : ~400 LOC côté CLI (commande clap + renderer texte
  + renderer JSON + driver subprocess Python). Estimé 1-1.5j sur
  LOT 11.
- Si l'agent fait des side-effects au load (rare mais possible - un
  `print("loading")` ou un fetch HTTP au top-level), `inspect` les
  déclenche. Documenter : "le load doit être pure".
- Subprocess Python implique de connaître le chemin Python du runtime
  Apollia (la `venv` interne). Le CLI gère cela.

**À surveiller :**

- Adoption pré-commit côté builders externes - si faible, prévoir un
  template pre-commit dans le starter kit agent.
- Output JSON : stabiliser le schéma (versionned `schema_version: 1`)
  pour ne pas casser les outils tiers.
- Cas d'agents avec décorateurs custom (`@cachetools.cached`, etc.) :
  `inspect` doit unwrap proprement les décorateurs autour de `@skill`.

## Principes architecturaux impactés

- **Principe #4 - Fail fast** : matérialisation directe au niveau
  ergonomique builder.
- **Principe #8 - CLI humaine, API machine** : `apollia inspect` est
  les deux à la fois (human-readable + `--json`).
- **Principe #3 - Contrat minimal** : le contrat de l'agent est
  introspectable sans démarrer le runtime - preuve qu'il est vraiment
  minimal et statique.

## Liens

- ADR-098 - Decorator-first (rend l'introspection possible)
- ADR-099 - Signature inference (alimente le rapport schémas)
- ADR-103 - Datasources & templates (vérifications statiques ajoutées)
- ADR-104 - Secrets read-only (vérifications statiques ajoutées)
- ADR-107 - Auto module instance (rend `module.agent` toujours
  disponible)
- ADR-082 - Tool governance (le rapport croise les permissions tools)
