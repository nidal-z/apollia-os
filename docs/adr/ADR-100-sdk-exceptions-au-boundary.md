# ADR-100 - Exceptions typées au boundary, AIPResult interne

**Date :** 2026-05-19
**Statut :** Accepté
**Sprint :** Phase release v0.1.0 (refonte SDK)

---

## Contexte

Le contrat de retour des agents repose actuellement sur la construction
**explicite** d'un `AIPResult` côté Python. Chaque handler doit décider du
type de retour (`completed` / `failed` / `input_required`) et appeler la
classmethod adéquate, puis retourner `.to_dict()`.

**État observé au 2026-05-19** (recherche `AIPResult\.(completed|failed|
input_required)` dans `agents/`) :

- **~340 occurrences** sur 10 agents bundled.
- **~210 LOC de boilerplate par worker** consacrées uniquement à
  formater les retours d'erreurs (`return AIPResult.failed("CODE",
  message, details={...}).to_dict()`).
- Le code "métier" et le code "formatage erreur" sont mélangés sur la
  même indentation. Une lecture verticale d'un handler ne distingue plus
  les deux.
- `AIPResult` est **injecté magiquement** dans `run.__globals__` par le
  bridge Rust (`crates/apollia-aip/src/bridge.rs:39` const `AIP_TYPES_PY`)
  - l'auteur Python utilise un symbole non importé. IDE et linters le
  signalent comme `undefined`. La friction est réelle (auteurs ajoutent
  des `# noqa` dans tout le repo pour la museler).
- Pas de typage : `AIPResult.failed("REJECTED", "...")` accepte
  n'importe quel code (string libre), aucun catalogue, pas de
  recherche cross-agent des codes d'erreur définis.
- L'erreur "le mode est mauvais" se traduit en runtime côté Rust en
  `DeserializationError` opaque si le dict retourné a un mauvais shape.

Les frameworks modernes (FastAPI `HTTPException`, ASGI, Django REST,
gRPC `StatusCode`) prouvent qu'il est plus ergonomique de **lever des
exceptions typées** et de laisser le framework formater la réponse.
L'agent Apollia se prête au même pattern : le boundary
`bridge.rs::call_run` peut trapper une exception Python et la sérialiser
en `AIPResult` côté Rust sans que l'agent ait à connaître ce type.

## Décision

**Nous adoptons le pattern "exceptions typées au boundary" : l'agent lève
des exceptions Python typées du SDK (`DomainError`, `PayloadError`,
`NeedHumanInput`, `BudgetError`, `PermissionError`), le SDK boundary les
trappe au dispatch et formate en `AIPResult.failed(...)` /
`AIPResult.input_required(...)`. L'agent ne manipule plus jamais
`AIPResult` directement.**

Hiérarchie d'exceptions publique (`apollia.errors`) :

```
ApolliaError                       # base abstraite, jamais levée directement
├── DomainError                    # erreur métier - devient AIPResult.failed
│   └── (l'auteur peut sous-classer pour ses propres codes)
├── PayloadError                   # validation input ratée - failed (CODE="PAYLOAD")
├── PermissionError                # tool/secret/datasource non autorisé - failed (CODE="PERMISSION")
├── BudgetError                    # StepBudget dépassé - failed (CODE="BUDGET")
├── NeedHumanInput                 # devient AIPResult.input_required
└── UnsupportedAnnotationError     # load-time, jamais runtime
```

Sémantique :

1. **`DomainError(code, message, details=None)`** : l'agent signale une
   erreur métier connue (ex. `DOCX_LOCKED`, `RATE_LIMITED`,
   `MODEL_REFUSED`). Le boundary la sérialise en `AIPResult.failed`.
2. **`PayloadError(field, message, expected=None)`** : levée
   **automatiquement** par le SDK lors de la validation de signature
   (ADR-099) si l'input ne match pas le JSON Schema. L'agent peut aussi
   la lever explicitement (validation sémantique post-typage).
3. **`NeedHumanInput(prompt, context=None, suggestions=None)`** :
   sérialisée en `AIPResult.input_required` (le runtime ouvre une carte
   HITL).
4. **Toute autre exception non typée** (ex. `KeyError`, `ZeroDivisionError`,
   `httpx.ConnectError` si jamais quelqu'un en importe) : trappée comme
   `DomainError(code="UNHANDLED", message=str(exc),
   details={"traceback": ..., "type": exc.__class__.__name__})`. Avec
   `tracing::error!` côté Rust pour signal d'alarme builder.
5. **Retour normal** : l'agent retourne le `dict` métier (ou `None`). Le
   boundary l'enveloppe en `AIPResult.completed(data=...)`. Plus jamais
   `return AIPResult.completed(...).to_dict()`.

Exemple cible :

```python
@skill("extract")
async def extract(self, path: str, ctx) -> dict:
    if not Path(path).exists():
        raise DomainError("FILE_NOT_FOUND", f"Path does not exist: {path}",
                          details={"path": path})
    if Path(path).stat().st_size > 50_000_000:
        raise DomainError("FILE_TOO_LARGE", "Max 50MB",
                          details={"size": Path(path).stat().st_size})
    text = await self._read(path, ctx)
    return {"text": text, "chars": len(text)}
```

Le SDK boundary (`_internal/dispatch.py`) trap les exceptions au dispatch
et produit l'`AIPResult` que la couche PyO3 attend.

## Alternatives considérées

### Option A - Sentinelle de retour (Result Ok/Err style Rust) (rejetée)

**Pour :** explicite, pas de magie d'exceptions, style fonctionnel.
**Contre :** chaque appel imbriqué (helper, sous-fonction) doit faire
suivre un `Result` ou tout réécrire. Verbose en Python (`match
result: case Ok(v): ...`). Casse l'idiome Python.

### Option B - Sous-classer `AIPResult` côté agent (rejetée)

**Pour :** garde la classmethod actuelle.
**Contre :** ne supprime aucune duplication, ajoute une couche d'héritage,
nécessite toujours `.to_dict()`.

### Option C - Décorateur `@with_result` qui wrap le handler (rejetée)

**Pour :** transparent côté auteur.
**Contre :** déplace la magie d'un endroit à un autre. Toujours pas de
typage des codes d'erreur. Mélange mal avec le boundary PyO3 (le décorateur
intercepte avant le bridge - collision avec ADR-098 / dispatcher central).

### Option retenue - Exceptions typées trappées au boundary

**Pour :** idiome Python natif (`raise` partout), zéro boilerplate sur
les chemins de succès, l'agent ne sait pas que `AIPResult` existe, le
boundary devient le **seul** endroit qui formate les réponses (single
point of truth pour audit, logging, tracing). Sous-classer
`DomainError` permet de définir un catalogue de codes typé par agent.
**Compromis acceptés :** un peu de magie au boundary (toute exception
non-`ApolliaError` devient `UNHANDLED`) - documentée, observable via
`ctx.logger` (ADR-106).

## Conséquences

**Positives :**

- Le code métier devient **linéaire** : pas de `try/except` partout pour
  formater des `failed`. La validation devient un `raise` au début du
  handler, le succès est un `return dict`.
- **Suppression mesurée : ~210 LOC × 5 workers = ~1 050 LOC** de
  boilerplate "return AIPResult.failed" sur les agents bundled.
- L'IDE comprend que `DomainError` est un type Python régulier
  (importé, autocomplété). Plus de `# noqa` ni de symbole magique.
- Catalogue de codes d'erreur émergent - un agent peut définir
  `class DocxError(DomainError): pass` et les codes deviennent
  inspectables (`apollia inspect` ADR-110 peut les lister).
- Le boundary devient le seul endroit qui sérialise - il enrichit
  systématiquement avec `step_id`, `agent_name`, timestamp, ce qui était
  fait manuellement (ou pas) avant.
- Cohérence avec FastAPI/HTTPException → courbe d'apprentissage proche
  de zéro.

**Négatives / Compromis :**

- Toute exception inattendue (ex. `httpx.ConnectError` si l'auteur
  utilise une lib non-stdlib pour des raisons légitimes) finit en
  `UNHANDLED` sans message métier. À documenter : "trappe ce que tu
  connais, le SDK trappe le reste".
- L'auteur perd la possibilité de retourner directement un dict shape
  `{"status": "input_required", ...}` (rare, mais possible aujourd'hui).
  → Acceptable : on lève `NeedHumanInput` à la place.
- Migration des 10 agents existants : ~340 occurrences à remplacer.
  Effort estimé 1-2j sur LOT 13.

**À surveiller :**

- Émergence d'exceptions trappées en `UNHANDLED` récurrentes (ex.
  timeouts réseau) : si une famille apparaît, créer une exception SDK
  dédiée (`NetworkError`).
- Coût performance du `try/except` au boundary par appel (négligeable
  attendu, à mesurer si on dépasse 1 000 invocations/s en bench).
- Sous-classes `DomainError` côté agent : risque de doublon de codes
  inter-agents. Documenter une convention `AGENTID.CODE` si besoin.

## Principes architecturaux impactés

- **Principe #3 - Contrat minimal** : renforcé. Le handler retourne un
  `dict` métier ou lève. Plus de classmethod magique à connaître.
- **Principe #4 - Fail fast** : renforcé. Le boundary log immédiatement
  toute `UNHANDLED` avec stacktrace, ce qui rend visible des bugs
  jusqu'ici masqués par des `try/except` larges côté agent.
- **Principe #7 - Garde-fous non-négociables** : `BudgetError` levé par
  le runtime (StepBudget Rust) traverse maintenant le boundary
  proprement et n'est plus convertible en `completed` par mégarde côté
  Python.

## Liens

- ADR-098 - Decorator-first (parent direct)
- ADR-099 - Signature inference (alimente `PayloadError` automatique)
- ADR-109 - `AIPResult` interne (cousin direct - le boundary construit
  l'AIPResult depuis les exceptions)
- ADR-014 - Bridge PyO3 async (modifié : plus de `AIP_TYPES_PY` injection
  dans `run.__globals__`)
- ADR-083 - Trust model agents Python (alignement audit trail des
  erreurs)
