# ADR-103 — Datasources YAML et templates Jinja2 accessibles au runtime

**Date :** 2026-05-19
**Statut :** Accepté
**Sprint :** Phase release v0.1.0 (refonte SDK)

---

## Contexte

Les agents Apollia déclarent depuis longtemps des **datasources** (fichiers
YAML versionnés contenant des données statiques : taxonomies, prompts
système, listes de sources, schémas de référence) et des **templates**
(fichiers Jinja2 utilisés pour formater des prompts LLM, des reports,
des emails). Ces ressources existent dans le packaging des agents :

```
agents/veille-ia/
├── datasources/
│   ├── topics.yaml          # taxonomie des sujets de veille
│   ├── sources.yaml         # 200+ flux RSS curés
│   └── prompts.yaml         # phrases système versionnées
├── templates/
│   ├── digest.md.j2         # template digest hebdo
│   └── source-summary.md.j2 # template synthèse par source
└── workers/web-search-worker.py
```

**État observé au 2026-05-19** :

- Le packaging est conscient de ces fichiers (CLI `apollia agent install`
  les copie dans `~/.apollia/agents/<id>/`).
- **Côté runtime Python : aucune API**. Les agents lisent les YAML par
  `ctx.tools.invoke("file_read", path="datasources/topics.yaml")` puis
  parsent eux-mêmes (`yaml.safe_load`).
- Aucune YAML lib stdlib → certains agents importent `yaml` (PyYAML),
  contredisant le principe #2 sans manifest explicite.
- Pour les templates Jinja2, c'est pire : aucun agent ne s'en sert
  effectivement parce que importer `jinja2` casse les workers
  zero-deps. Le dossier `templates/` reste cosmétique.
- 4 occurrences de `file_read("datasources/...")` dans les agents
  bundled — toutes ré-implémentent leur propre cache local en variable
  de classe (`if self._topics is None: self._topics = yaml.safe_load(...)`).
- Aucune validation : si le YAML est cassé, l'agent crash au premier
  `recall()` du tool — pas au load (viole #4 fail-fast).
- Aucun gating sécurité : un agent peut lire **n'importe quel** YAML
  du workspace par `ctx.tools.invoke("file_read", ...)` — pas
  d'isolation entre les datasources de A et celles de B.

Or les datasources et templates sont le **pilier #3 et #1 du business
model agent forge** (cf. mémoire `project_business_model` : livrables
prestation = code + templates + datasources + règles + README). On
ne peut pas livrer un v0.1.0 où les deux piliers cosmétiques de
packaging ne servent à rien à l'exécution.

## Décision

**Nous adoptons deux nouveaux services SDK : `ctx.datasources` (chargement
YAML versionnés et cachés) et `ctx.templates` (rendu Jinja2 sandboxé).
L'accès est gating-protégé par manifest (`@agent(datasources=(...),
templates=(...))`). Côté Rust, ajout du crate `minijinja` au workspace
pour le rendu Jinja2 sans dépendance Python.**

Surface publique :

```python
class DatasourcesService(Protocol):
    async def get(self, name: str) -> Any:
        """Charge `datasources/<name>.yaml` du package agent.
        Cache la valeur après premier load (LRU au niveau processus).
        Raise PermissionError si <name> n'est pas dans le gating manifest.
        Raise DomainError("DATASOURCE_NOT_FOUND") si fichier absent.
        Raise DomainError("DATASOURCE_INVALID") si YAML invalide."""

    def list(self) -> list[str]:
        """Liste les datasources autorisées par le manifest."""

class TemplatesService(Protocol):
    async def render(self, name: str, /, **vars) -> str:
        """Rend `templates/<name>.j2` avec les variables fournies.
        Sandboxé (pas d'accès `os`, `subprocess`, etc. côté template).
        Raise PermissionError si non gating.
        Raise DomainError("TEMPLATE_NOT_FOUND") si fichier absent.
        Raise DomainError("TEMPLATE_RENDER_ERROR") si rendu échoue."""

    def list(self) -> list[str]:
        """Liste les templates autorisés."""
```

Gating manifest :

```python
@agent(
    name="veille-ia",
    version="2.0",
    datasources=("topics", "sources", "prompts"),
    templates=("digest", "source-summary"),
)
class VeilleIA:
    async def run(self, task, ctx):
        topics = await ctx.datasources.get("topics")  # dict YAML
        digest = await ctx.templates.render("digest",
                                            week=42, items=[...])
```

Architecture runtime :

1. **Crate Rust** : `apollia-aip` charge au démarrage du runtime
   Python le contenu de `datasources/*.yaml` (validé) et la liste de
   `templates/*.j2`. Stocké en `Arc<DatasourceStore>` partagé entre
   acteurs.
2. **YAML parsing côté Rust** (`serde_yaml`) — pas de PyYAML côté
   Python. La donnée traverse PyO3 en `PyAny` (dict/list/str/int/etc.).
3. **Jinja2 rendu côté Rust** (`minijinja` v2 ajouté au workspace
   `Cargo.toml`) — sandboxé par défaut (pas d'auto-import, pas de
   filter dangereux), `{% include %}` désactivé.
4. **Gating** : le manifest généré par `@agent(...)` (cf. ADR-098) liste
   les noms autorisés. Toute lecture hors liste lève `PermissionError`
   au boundary.
5. **Fail-fast au load** : si une datasource déclarée n'existe pas, ou
   si un YAML est invalide, l'agent ne charge pas (principe #4).
6. **Pas d'écriture** : `ctx.datasources` est read-only. Pour générer
   du contenu dynamique, l'agent passe par sa propre mémoire ou
   `ctx.memory.remember(...)`.

## Alternatives considérées

### Option A — Laisser les agents lire les fichiers via `file_read` (status quo) (rejetée)

**Pour :** zéro nouvelle API.
**Contre :** ne résout aucun problème (parsing manuel, pas de cache, pas
de gating, fail tardif). Templates Jinja2 inutilisables sans dep externe.

### Option B — YAML/Jinja2 côté Python (PyYAML + jinja2) (rejetée)

**Pour :** simple à implémenter, libs matures.
**Contre :** viole le principe #2 (zéro dep externe Python). Forcerait
le SDK à dépendre de PyYAML et jinja2 → contradiction avec la promesse
"stdlib-only" affichée dans le packaging.

### Option C — Charger une seule fois au boot et exposer via dict global (rejetée)

**Pour :** simplissime, pas d'async.
**Contre :** mémoire bloquante (un YAML de 50 MB chargé pour tous les
agents même non-utilisateurs). Pas de gating per-agent.

### Option retenue — Services nestés `ctx.datasources` + `ctx.templates` lazy + Rust-side parsing

**Pour :** stdlib-only côté Python, parsing rapide côté Rust, gating
manifest, fail-fast au load, sandbox Jinja2 sécure, cache LRU implicite,
templates Jinja2 enfin utilisables.
**Compromis acceptés :** ajout `minijinja` v2 au workspace Cargo
(~120 KB binaire). Documenté comme dépendance principielle Rust (pas
Python). Si un auteur a besoin d'un filter Jinja2 personnalisé, on
n'expose pas l'enregistrement custom en v1 — feature reportée v1.x.

## Conséquences

**Positives :**

- Le pilier "datasources + templates" du business model agent forge
  devient pleinement utilisable au runtime — fin de la friction
  packaging vs runtime.
- L'agent écrit `await ctx.templates.render("digest", week=42)` —
  trivial, sans deps. L'IDE autocomplète les variables (via stub).
- Gating manifest protège les datasources : un agent qui a accès à
  `topics.yaml` ne peut pas lire `prompts.yaml` d'un autre agent par
  glissement de path.
- Élimination de PyYAML et jinja2 du code agent — alignement strict
  principe #2.
- Mise en cache automatique : un YAML de 200 sources chargé une fois,
  réutilisé par tous les appels du même agent.
- Fail-fast au load : un agent avec un YAML cassé refuse de démarrer,
  visible immédiatement par le builder.

**Négatives / Compromis :**

- Ajout `minijinja` v2 dans le workspace Cargo (jusqu'ici uniquement
  utilisé en interne par `apollia-llm` pour les system prompts ; à
  factoriser).
- L'auteur ne peut plus utiliser de filter Jinja2 custom en v1 (la
  sandbox `minijinja` autorise ce qu'on lui dit). Reporter à v1.x.
- Le manifest devient légèrement plus verbeux (3 paramètres datasources
  à déclarer) — acceptable, c'est exactement la zone de friction qu'on
  veut éclairer.

**À surveiller :**

- Performance Jinja2 sur templates volumineux (> 100 KB rendu) —
  benchmarker. Si > 100 ms, mettre en cache compilé.
- Émergence de besoins "datasources mutables" (l'agent veut updater
  topics.yaml) — explicitement non-supporté en v1, à revoir si
  > 3 demandes.
- Croissance du gating manifest si un agent a 20+ datasources : envisager
  un wildcard `datasources=("*",)` (déconseillé par défaut).

## Principes architecturaux impactés

- **Principe #2 — Zéro dépendance externe** : renforcé côté Python
  (zéro PyYAML/jinja2 dans le SDK). Ajout côté Rust (`minijinja`)
  conforme au workspace.
- **Principe #4 — Fail fast** : datasources et templates validés au
  load.
- **Principe #7 — Garde-fous non-négociables** : gating manifest =
  équivalent du StepBudget pour les datasources (non contournable
  côté agent).
- **Principe #3 — Contrat minimal** : la déclaration manifest reste
  optionnelle (un agent qui n'utilise ni datasources ni templates n'a
  rien à ajouter).

## Liens

- ADR-101 — `ctx` Protocol (ajoute 2 services à la surface)
- ADR-098 — Decorator-first (`@agent(datasources=..., templates=...)`)
- ADR-104 — Secrets read-only (modèle gating identique)
- ADR-082 — Tool governance (similar gating pattern)
- ADR-110 — `apollia inspect` (affiche les datasources/templates
  déclarés)
