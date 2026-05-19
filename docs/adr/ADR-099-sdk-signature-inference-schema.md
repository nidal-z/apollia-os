# ADR-099 — Signature inference comme schéma I/O

**Date :** 2026-05-19
**Statut :** Accepté
**Sprint :** Phase release v0.1.0 (refonte SDK)

---

## Contexte

Aujourd'hui, dans `sdk/apollia/` v0.4.0, l'auteur d'agent doit
**décrire trois fois** la même chose pour exposer un skill :

1. La signature Python de la méthode handler (`async def extract(self,
   path, page_range=None)`).
2. La validation du payload dans le corps du handler (`if not isinstance(
   payload, dict): return AIPResult.failed("PAYLOAD", ...)`,
   `path = payload.get("path")`, etc.).
3. Le manifest TOML déclarant `[[skills.extract_text.input]]` avec types,
   defaults, et descriptions.

Exemple concret (`agents/veille-ia/workers/web-search-worker.py`, lignes
~120-180) : la méthode `_handle_search` reçoit un `dict` brut, extrait
4 champs (`query`, `max_results`, `recency`, `lang`) avec `payload.get(...)`,
vérifie 3 invariants à la main, et retourne un autre dict — le TOML voisin
re-décrit exactement la même structure dans `[[skills.search.input]]`.

**Friction mesurée :**

- **~30-50 LOC de validation payload par skill**, dupliquées sur ~20
  skills du repo, soit **~700 LOC de validation manuelle**.
- Les manifests TOML divergent silencieusement du code : 3 mismatchs
  détectés en grep (manifest annonce un champ `top_k` que le handler ne
  lit pas, ou inversement).
- Pas d'IDE feedback : l'auteur appelle son propre handler sans
  autocomplete (le `payload: dict` typage masque tout).
- `apollia inspect` (ADR-110) ne peut pas générer un JSON Schema utile
  car la vérité est éparpillée entre code Python et TOML.

Or Python expose nativement, depuis Python 3.10+, des outils suffisants
pour rendre la signature source unique :

- `inspect.signature(fn)` — liste les paramètres, annotations, defaults.
- `typing.get_type_hints(fn)` — résout les annotations (y compris
  `from __future__ import annotations`).
- Conversion type Python → JSON Schema : `str → {"type": "string"}`,
  `int → {"type": "integer"}`, `str | None → {"type": "string",
  "nullable": true}`, `list[str] → {"type": "array", "items": {...}}`,
  `Literal["a", "b"] → {"enum": ["a", "b"]}`, `Path → {"type": "string",
  "format": "path"}`.

FastAPI, Typer, Pydantic ont prouvé que ce mapping est viable et
ergonomique pour la grande majorité des cas.

## Décision

**Nous adoptons la signature Python comme source unique du schéma I/O des
skills, on_message et orchestrated handlers. Le SDK introspecte
`inspect.signature(handler)` au moment du `@agent` pour générer le JSON
Schema input + output, valider les payloads entrants côté boundary, et
populer `__apollia_manifest__` sans nécessiter de TOML ni de TypedDict
côté agent.**

Règles concrètes :

1. **Paramètres input** : tous les paramètres positionnels/keyword sauf
   `self` et `ctx`. Le paramètre `ctx` est détecté par nom (convention)
   ou type (`Ctx`/`ApolliaCtx` protocol) et exclu du schéma I/O.
2. **Types supportés sans configuration** : `str`, `int`, `float`,
   `bool`, `bytes` (encodé base64), `list[T]`, `dict[str, T]`, `T | None`,
   `Optional[T]`, `Literal[...]`, `Enum` (sérialisé par valeur),
   `datetime.date`, `datetime.datetime` (ISO 8601), `pathlib.Path`
   (string + flag).
3. **Defaults** : extraits de `inspect.Parameter.default` → ajoutés au
   JSON Schema sous `default` ; absence de default ⇒ champ `required`.
4. **Docstring** : la première ligne de la docstring de la méthode devient
   `description` du skill ; les sections `Args:` (style Google) sont
   parsées pour les descriptions par champ. Format Sphinx / NumPy
   non supporté en v1 (Google-only).
5. **Output** : annotation de retour `-> dict` ou `-> SomeTypedDict`
   ou `-> SomeDataclass`. Si `-> dict` brut ⇒ schéma output =
   `{"type": "object", "additionalProperties": true}` (signal "à
   restreindre dans une v1.x").
6. **Fallback dataclass / TypedDict** pour les cas complexes (input
   nested à 3+ niveaux, polymorphisme, structures partagées entre
   skills) — l'auteur peut écrire `@dataclass class SearchQuery` et
   annoter `async def search(self, query: SearchQuery, ctx)`. Le SDK
   l'introspecte récursivement.
7. **Échec au load** : un type non supporté (ex. `numpy.ndarray`) lève
   `apollia.errors.UnsupportedAnnotationError` à l'import, avant tout
   appel — aligné principe #4.

Le manifest TOML disparaît pour le cas standard. Il survit uniquement comme
**format de packaging** (ADR-110 / `apollia.toml` pour l'installation et
les métadonnées), pas comme contrat I/O.

## Alternatives considérées

### Option A — Manifest TOML restant source de vérité (rejetée)

**Pour :** lecture par tiers (Rust, autres langages) sans interpréter
Python.
**Contre :** maintient les triples descriptions actuelles. Mismatchs
silencieux persistent. Pas d'autocomplete IDE côté auteur.

### Option B — Pydantic v2 BaseModel par skill (rejetée)

**Pour :** validation runtime industrielle, schéma JSON gratuit,
écosystème connu.
**Contre :** dépendance externe interdite (principe #2 — `pydantic` n'est
pas stdlib). Verbosité élevée : un `class SearchInput(BaseModel)` par
skill multiplie les LOC. La signature reste obligatoire en double si on
veut le typage du handler propre.

### Option C — Schéma JSON déclaré dans la docstring (rejetée)

**Pour :** zéro nouvelle annotation, fonctionne sans introspection
sophistiquée.
**Contre :** docstring devient un mini-DSL fragile à parser ; aucune aide
IDE ; les outils de refactoring (rename de champ) ne touchent pas la
docstring.

### Option retenue — `inspect.signature` + types stdlib

**Pour :** zéro nouvelle dépendance, autocomplete IDE native sur le
handler (l'auteur appelle directement ses méthodes avec types), un seul
point de vérité, `apollia inspect` (ADR-110) génère un JSON Schema
fiable. Mapping Python→JSON Schema est ergonomique pour 95 % des cas.
**Compromis acceptés :** les 5 % restants (types complexes, polymorphisme,
schémas réutilisés) passent par `@dataclass` ou `TypedDict` — qui restent
stdlib. Pas de validation runtime aussi exhaustive que Pydantic (ex. pas
de regex sur strings, pas de bornes numériques), mais ces contrôles
restent disponibles via assertions explicites côté handler.

## Conséquences

**Positives :**

- Suppression de **~700 LOC de validation payload** sur les agents du
  repo (mesure ciblée post-migration LOT 13).
- L'auteur écrit `async def search(self, query: str, max_results: int = 10,
  ctx)` — sa signature DEVIENT le contrat A2A.
- Autocomplete IDE sur les appels internes (`self._handle_search(query=...,
  max_results=5)`) puisque la méthode a un vrai typage.
- `apollia inspect <agent.py>` (ADR-110) affiche le JSON Schema input/output
  exact sans démarrer Python en mode runtime.
- Manifests TOML non requis pour l'usage standard ⇒ courbe d'apprentissage
  réduite.
- Le boundary (ADR-100, ADR-109) peut désormais valider l'input avant
  d'invoquer le handler, et formater une `PayloadError` claire si type
  mismatch.

**Négatives / Compromis :**

- Types non supportés (ex. `numpy`, `pandas.DataFrame`) doivent passer
  par bytes + sérialisation explicite. Aligné avec le principe #2 mais
  augmente la friction pour les cas ML.
- Le mapping Python→JSON Schema est implémenté côté SDK et doit être
  maintenu. Estimé ~250 LOC dans `_internal/schema_inference.py`.
- Validation runtime moins riche que Pydantic (regex/bornes/contraintes
  custom). À assumer — un assert + DomainError reste lisible.

**À surveiller :**

- Émergence de patterns "types non supportés" récurrents (UUIDs,
  Decimal) — ajouter au mapping si > 3 demandes.
- Performance de l'introspection sur agents à 20+ skills (mesurer au
  `import`). Si > 100 ms, mettre en cache `__apollia_manifest__`.
- Docstring format Google : si auteurs utilisent NumPy/Sphinx, prévoir
  un parser plus tolérant en v1.x.

## Principes architecturaux impactés

- **Principe #3 — Contrat minimal** : poussé à son maximum. La signature
  Python EST le contrat — rien d'autre à déclarer.
- **Principe #4 — Fail fast** : renforcé. Tout type non mappable échoue
  à l'import.
- **Principe #2 — Zéro dépendance externe** : respecté strictement
  (stdlib `inspect` + `typing`).
- **Principe #8 — CLI humaine, API machine** : la commande
  `apollia inspect` (ADR-110) lit directement les manifests générés —
  alignement machine ; les erreurs d'inférence sont human-friendly
  (ligne, fichier, paramètre fautif).

## Liens

- ADR-098 — Apollia AgentKit decorator-first (parent direct)
- ADR-100 — Exceptions typées au boundary (valide les payloads avant
  d'invoquer le handler)
- ADR-109 — `AIPResult` interne (consomme le schéma output pour
  sérialiser)
- ADR-110 — `apollia inspect` (lit les manifests générés)
- ADR-083 — Trust model agents Python (signature validation alignée)
