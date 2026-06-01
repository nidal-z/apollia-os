# ADR-109 - `AIPResult` devient interne au SDK

**Date :** 2026-05-19
**Statut :** Accepté
**Sprint :** Phase release v0.1.0 (refonte SDK)

---

## Contexte

`AIPResult` est aujourd'hui une classe Python **injectée magiquement**
dans le `run.__globals__` de chaque agent par le bridge Rust
(`crates/apollia-aip/src/bridge.rs`, const `AIP_TYPES_PY` lignes
41-110, injection dans `call_run()` ligne 295). L'agent appelle
`AIPResult.completed(data)` / `AIPResult.failed(code, msg, details)` /
`AIPResult.input_required(prompt, context)` puis retourne le résultat
sérialisé en dict.

**État observé au 2026-05-19** :

- Le bridge Rust contient ~70 LOC de source Python en const
  (`AIP_TYPES_PY`) - du code Python embedded dans un .rs, qui n'est
  ni testé ni linté côté Python.
- À chaque invocation, le bridge exécute ce code Python pour créer la
  classe, l'injecte dans `run.__globals__` (~10 LOC bridge.rs:295-310).
- Côté agent, `AIPResult` est un symbole **non importé** - l'IDE le
  flag comme `undefined`, l'auteur ajoute `# noqa` partout (vu
  ~340 occurrences dans le repo, cf. ADR-100).
- `AIPResult` était nécessaire historiquement parce que le bridge ne
  pouvait pas désérialiser tout shape de dict en `Result<AIPResult,
  _>` côté Rust - la classe Python forçait une discipline.
- Avec ADR-100 (exceptions au boundary) + ADR-099 (signature
  inference), le boundary devient le seul endroit qui formate
  l'AIPResult. L'agent ne le construit plus jamais directement.

Conséquence : le mécanisme d'injection `run.__globals__` n'a plus
de raison d'être. Le bridge Rust peut désérialiser un `dict` métier
ordinaire en `AIPResult::completed(data: serde_json::Value)` côté Rust,
et c'est le SDK Python (`_internal/dispatch.py`) qui décide du shape
final.

## Décision

**Nous rendons `AIPResult` interne au SDK Python. Le bridge Rust
n'injecte plus la classe dans `run.__globals__`. Le SDK
(`sdk/apollia/_internal/aip_result.py`) construit le résultat à partir
du return value du handler (ou de l'exception trappée), et le passe au
bridge sous forme de dict normalisé. L'agent ne voit plus jamais
`AIPResult`.**

Architecture cible :

```
┌────────────────────────────────────────────────────────────┐
│ Agent Python                                                │
│                                                             │
│   @skill("foo")                                             │
│   async def foo(self, x, ctx) -> dict:                      │
│       return {"result": x * 2}        ◄── métier pur        │
│       # ou: raise DomainError("X", "msg")                   │
└────────────────────────────────────────────────────────────┘
              │ return ou exception
              ▼
┌────────────────────────────────────────────────────────────┐
│ SDK boundary (_internal/dispatch.py)                        │
│                                                             │
│   try:                                                      │
│       result = await handler(*args)                         │
│       return _to_aip_result_dict(result)  # completed       │
│   except NeedHumanInput as e:                               │
│       return _to_input_required(e)                          │
│   except ApolliaError as e:                                 │
│       return _to_failed(e)                                  │
│   except Exception as e:                                    │
│       ctx.logger.exception("UNHANDLED")                     │
│       return _to_failed_unhandled(e)                        │
└────────────────────────────────────────────────────────────┘
              │ dict normalisé {status, ...}
              ▼
┌────────────────────────────────────────────────────────────┐
│ Bridge Rust (apollia-aip)                                   │
│                                                             │
│   serde_json::from_value::<AIPResult>(dict)                 │
│   ⇒ AIPResult { status: Completed, data: ... }              │
└────────────────────────────────────────────────────────────┘
```

Changements concrets :

1. **Suppression côté Rust** :
   - Const `AIP_TYPES_PY` (`bridge.rs:41-110`) supprimée.
   - Code d'injection dans `call_run()` (`bridge.rs:295-310`) supprimé.
   - Le bridge ne fait plus que la désérialisation `dict → AIPResult`.
2. **Ajout côté SDK** :
   - `sdk/apollia/_internal/aip_result.py` (~80 LOC) - fonctions
     `_to_aip_result_dict(value)`, `_to_input_required(exc)`,
     `_to_failed(exc)`, `_to_failed_unhandled(exc)`.
   - `sdk/apollia/_internal/dispatch.py` (~100 LOC) - boundary qui
     wrap le handler dans le try/except et appelle les helpers
     ci-dessus.
3. **Pas exposé en public** - `AIPResult` n'est pas importable depuis
   `apollia.*`. Le seul interface public reste les exceptions
   (`apollia.errors`) et le dict de retour.
4. **Compatibilité Rust** - le shape JSON du dict reste celui de
   `AIPResult` (status, data, code, message, details, prompt, context).
   Le bridge `serde_json::from_value` continue à fonctionner.
5. **Documentation** - book v1 mentionne que l'auteur retourne un dict
   ou lève. Plus aucune mention d'`AIPResult` côté agent.

## Alternatives considérées

### Option A - Garder `AIPResult` exposé mais ajouter une version
importable (`from apollia import AIPResult`) (rejetée)

**Pour :** rétrocompat partielle.
**Contre :** maintient deux façons (importer vs magiquement injecté).
Confusion garantie. L'auteur continue à voir `AIPResult` partout,
contraire à l'esprit ADR-100.

### Option B - Garder l'injection `run.__globals__` mais retirer
l'usage dans les agents (rejetée)

**Pour :** zéro modif bridge Rust.
**Contre :** laisse du code mort dans le bridge. Maintient le foreign
symbol injecté → IDE warning persistant.

### Option C - `AIPResult` exposé comme type sealed (NewType / Final)
documenté "interne" (rejetée)

**Pour :** typage stricter.
**Contre :** demi-mesure. Si c'est interne, autant le rendre
inaccessible.

### Option retenue - Pure suppression de l'API publique + interne SDK

**Pour :** cohérent avec ADR-100 (agent ne voit plus AIPResult), bridge
Rust simplifié (-70 LOC), code Python embedded éliminé du .rs, l'IDE
n'a plus de symbole magique non-importé à signaler.
**Compromis acceptés :** breaking total pour tout code legacy qui
construisait `AIPResult.completed(...)` (mécanique : remplacer par
`return data` ou `raise DomainError(...)`).

## Conséquences

**Positives :**

- Bridge Rust simplifié : ~70 LOC de Python embedded supprimées,
  ~30 LOC de logique injection supprimées.
- Le SDK Python devient le seul propriétaire du shape `AIPResult` côté
  Python - testable, refactorable indépendamment.
- Plus de symbole magique côté agent - fin des `# noqa` et IDE warnings.
- Cohérence avec ADR-100 : l'agent retourne `dict` ou lève. Le
  formatage `AIPResult` est invisible.
- Migration : un script `apollia migrate-aip-result` peut faire les
  remplacements mécaniques (`return AIPResult.completed(x).to_dict()`
  → `return x` ; `return AIPResult.failed(c, m).to_dict()` → `raise
  DomainError(c, m)`).

**Négatives / Compromis :**

- Tout agent legacy qui construit `AIPResult` casse. Migration LOT 13
  doit traiter ~340 occurrences. Mécanique mais volumineuse.
- Le shape JSON du dict retourné par le SDK doit rester strictement
  compatible avec le `serde_json::from_value::<AIPResult>` Rust. Tout
  champ ajouté côté SDK doit être ajouté côté `apollia-core::AIPResult`
  (ou ignoré). À documenter strictement.
- Perte du symbole "global magique" pour les tests intégration qui
  voudraient construire un `AIPResult` manuel : remplacé par helpers
  internes utilisables en test (`apollia.testing.assert_completed(...)`).

**À surveiller :**

- Cohérence shape SDK ↔ Rust : ajouter un test de round-trip
  (sérialise SDK Python → désérialise Rust AIPResult → re-sérialise →
  diff = ∅).
- Tentation des auteurs de retourner un dict `{"status": "completed",
  "data": ...}` à la main (anti-pattern qui contournerait le boundary).
  Documenter "le boundary fait ça pour toi".

## Principes architecturaux impactés

- **Principe #3 - Contrat minimal** : poussé au max. L'agent retourne
  son dict métier, point.
- **Principe #5 - Un acteur, une responsabilité** : le boundary est le
  seul endroit qui formate `AIPResult`. Plus de "qui formate quoi" à
  chercher.
- **Principe #2 - Zéro dépendance externe** : préservé. Le bridge Rust
  ne contient plus de code Python embedded difficile à maintenir.

## Liens

- ADR-100 - Exceptions au boundary (cousin direct)
- ADR-099 - Signature inference (le boundary utilise le schéma output
  pour valider le dict retourné)
- ADR-098 - Decorator-first (le décorateur installe le boundary)
- ADR-014 - Bridge PyO3 async (modifié : suppression `AIP_TYPES_PY`)
