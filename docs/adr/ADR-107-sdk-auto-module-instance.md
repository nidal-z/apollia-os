# ADR-107 - `@agent` instancie et expose `agent` au module

**Date :** 2026-05-19
**Statut :** Accepté
**Sprint :** Phase release v0.1.0 (refonte SDK)

---

## Contexte

Le bridge PyO3 `crates/apollia-aip/src/bridge.rs` charge un module Python
puis cherche un attribut `agent` au niveau module (`getattr(module,
"agent")`) pour récupérer l'instance à invoquer. Cette convention est
née organiquement et est documentée dans `CLAUDE.md`-side mémoire :
"Tout .py d'agent doit définir `agent = MyClass()` au niveau module ET
utiliser des imports absolus" (cf. mémoire `feedback_apollia_python_imports`).

**État observé au 2026-05-19** :

- 100 % des agents bundled finissent leur module par
  `agent = MyWorkerClass()` - c'est devenu un rite de passage.
- Friction réelle : un nouvel auteur oublie cette ligne, son agent ne se
  charge pas, et le message d'erreur côté Rust est `getattr_failed:
  'module' object has no attribute 'agent'`. Pas explicit.
- Avec le passage à decorator-first (ADR-098), l'auteur écrit déjà
  `@agent(name="...")` au-dessus de sa classe. Re-instancier la classe
  en bas du fichier est redondant et redondant. C'est exactement le
  genre de boilerplate que le décorateur peut éliminer.
- Risque : si la classe a un `__init__` qui prend des arguments,
  `MyWorkerClass()` plante au load → l'auteur doit ré-architecturer.
- En testing, créer une seconde instance pour test (`worker_for_test =
  MyWorkerClass()`) est légitime, mais aujourd'hui mélangé avec le
  `agent = MyWorkerClass()` du module.

Le contrat runtime "le module DOIT exposer un attribut `agent`" est
stable - c'est le bridge PyO3 qui en dépend, et on ne veut pas le
changer (sinon casser ADR-014 / bridge). La question est uniquement :
qui produit cet attribut ?

## Décision

**Nous adoptons un comportement automatique du décorateur `@agent` : il
instancie la classe décorée et expose l'instance comme attribut `agent`
du module qui contient la classe. L'auteur n'écrit plus `agent =
MyClass()` à la fin du module. Le contrat runtime
`getattr(module, "agent")` est strictement préservé.**

Détail technique :

```python
# Implémentation simplifiée (sdk/apollia/_internal/agent_decorator.py)
def agent(name=None, version="0.1.0", **kwargs):
    def decorator(cls):
        # 1. Validation au load (signatures @skill, manifest, etc.)
        manifest = _build_manifest(cls, name, version, **kwargs)
        cls.__apollia_manifest__ = manifest

        # 2. Instanciation
        instance = cls()

        # 3. Exposition au module
        mod = sys.modules[cls.__module__]
        if hasattr(mod, "agent") and mod.agent is not instance:
            raise RuntimeError(
                f"Module {cls.__module__} already exposes an `agent` "
                f"attribute. Apollia permits exactly one @agent class "
                f"per module."
            )
        mod.agent = instance

        return cls  # le décorateur retourne la classe, pas l'instance
    return decorator if name is None else decorator
```

Règles :

1. **Une classe `@agent` par module** - fail-fast (RuntimeError) si un
   autre `agent` existe déjà. Convention forte alignée avec
   `feedback_apollia_python_imports`.
2. **`__init__` sans arguments obligatoires** - la classe doit
   s'instancier par `cls()`. Si elle a besoin de config, elle la lit
   depuis `ctx` au premier `run()` ou utilise des class-level
   constants.
3. **Le décorateur retourne la classe** (pas l'instance) - l'auteur
   peut toujours faire `worker_for_test = MyWorker()` dans un test
   sans collision.
4. **Imports absolus encore obligatoires** - le décorateur ne peut pas
   tirer un agent depuis un sous-package relatif (`from .helpers import
   X` casse PyO3). Documenté.
5. **Compatible introspection** - `apollia inspect` (ADR-110) charge le
   module sans démarrer le runtime ; après import, `module.agent`
   existe et expose `__apollia_manifest__`.

Exemple cible avant/après :

```python
# AVANT (v0.4.0)
from apollia.agents.worker import WorkerAgent
class MyWorker(WorkerAgent):
    def manifest(self): return {...}
    async def _handle_search(self, payload): ...
    def __init__(self):
        super().__init__()
        self.register_skill("search", self._handle_search)
agent = MyWorker()  # ← obligatoire, oubli = crash silencieux

# APRÈS (v1.0)
from apollia import agent, skill
@agent(name="my-worker", version="1.0.0")
class MyWorker:
    @skill("search")
    async def search(self, query: str, ctx) -> dict: ...
# Plus de `agent = MyWorker()` - généré par @agent
```

## Alternatives considérées

### Option A - Forcer l'auteur à écrire explicite `agent = MyClass()` (statu quo) (rejetée)

**Pour :** transparence - chacun voit qu'il y a une instance.
**Contre :** boilerplate redondant maintenant qu'on a `@agent`. Source
de bugs (oubli silencieux). Cogne contre la philosophie decorator-first.

### Option B - `@agent` retourne directement l'instance (l'auteur écrit
`MyClass = agent(...)(MyClass)`) (rejetée)

**Pour :** plus simple côté décorateur.
**Contre :** casse le pattern Python habituel (le résultat d'un
décorateur de classe est généralement la classe elle-même). Les
isinstance checks et autres introspection cassent.

### Option C - Macro `apollia.run_as_main()` à appeler en fin de
module (rejetée)

**Pour :** explicite.
**Contre :** une ligne à ne pas oublier. Pire que le `agent =
MyClass()` actuel.

### Option retenue - Exposition automatique via side-effect du
décorateur

**Pour :** zéro boilerplate, le décorateur fait son job complet,
fail-fast si erreur (double `@agent`, init avec args), contrat runtime
préservé.
**Compromis acceptés :** un peu de magie (le décorateur a un
side-effect d'instanciation et de mutation du module). Documenté.
L'auteur sophistiqué peut toujours inspecter via `module.agent`.

## Conséquences

**Positives :**

- Boilerplate "ligne morte" supprimée pour 100 % des agents.
- Plus de bug "j'ai oublié `agent = MyClass()`" - le décorateur le fait.
- L'auteur écrit naturellement comme un dev FastAPI / Flask
  (`@app.route(...)` et le framework fait le reste).
- Le contrat runtime `module.agent` reste identique - zéro modif côté
  bridge PyO3 (ADR-014 préservé).
- `apollia inspect` (ADR-110) peut charger un module et accéder à
  `module.agent.__apollia_manifest__` sans gymnastique.

**Négatives / Compromis :**

- Side-effect au moment de l'import : `from my_agent import MyWorker`
  instancie déjà l'agent. Pour un test pur (classe sans instance),
  importer la classe sans décorateur n'est pas possible - l'auteur
  doit dé-couper `MyWorker` en deux (logique + classe wrap @agent).
  Acceptable, peu fréquent en pratique.
- Multi-agent par module impossible (par design). Si un auteur veut
  3 agents, 3 fichiers.
- Le décorateur doit gérer l'ordre des décorateurs : `@agent` doit être
  appliqué APRÈS `@skill` (les `@skill` annotent les méthodes, `@agent`
  les agrège). En Python, ça correspond à l'ordre d'écriture (le plus
  externe = `@agent`). Documenté.

**À surveiller :**

- Cas tordus de double-import (recharge module) : `mod.agent` est
  écrasé à chaque import. Acceptable, comportement standard Python.
- Multi-process : si un agent est instancié dans 2 processes (worker
  pool ?), chaque process aura sa propre instance - comportement
  attendu.

## Principes architecturaux impactés

- **Principe #3 - Contrat minimal** : poussé au max. L'auteur écrit
  une classe décorée, point.
- **Principe #4 - Fail fast** : double `@agent` ou `__init__` avec
  args → erreur à l'import.

## Liens

- ADR-098 - Decorator-first (parent direct)
- ADR-014 - Bridge PyO3 async (contrat `module.agent` préservé)
- Mémoire `feedback_apollia_python_imports` (contrat respecté et
  automatisé)
