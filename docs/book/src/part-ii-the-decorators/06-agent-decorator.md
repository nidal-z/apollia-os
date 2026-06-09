# Le décorateur `@agent`

Un agent Apollia est une classe Python décorée par `@agent`. Le décorateur fait trois choses au moment où le module est importé :

1. il **valide** la classe (signature `__init__`, présence d'au moins un handler) ;
2. il **construit** le manifeste statique (`__apollia_manifest__`) ;
3. il **instancie** la classe et expose l'instance comme attribut `agent` du module.

L'auteur n'écrit ni `manifest()`, ni `agent = MyClass()`, ni d'héritage de base. Tout est dérivé de la classe et de ses méthodes décorées.

---

## Exemple minimal

```python
from apollia import agent, skill
from apollia.types import Ctx

@agent(
    name="hello",
    version="0.1.0",
    description="A tiny hello-world agent.",
)
class Hello:
    @skill("hello.greet", description="Greet a person by name.")
    async def greet(self, name: str, ctx: Ctx) -> dict:
        return {"message": f"Bonjour, {name} !"}
```

Trois éléments suffisent : une classe, un appel `@agent(...)`, et au moins une méthode décorée (`@skill`, `@on_message` ou `@orchestrated`). Le runtime peut maintenant charger ce fichier, lire `module.agent.__apollia_manifest__`, et router des invocations vers `agent.greet(...)`.

---

## Paramètres

`@agent` n'accepte que des arguments nommés (keyword-only). Trois sont obligatoires, les autres sont optionnels.

### Obligatoires

| Paramètre | Type | Rôle |
|---|---|---|
| `name` | `str` | Identifiant unique de l'agent (utilisé en CLI, A2A, registre). |
| `version` | `str` | Version sémantique (`"0.1.0"`, `"1.2.3"`). |
| `description` | `str` | Phrase courte affichée dans `apollia inspect`, l'UI Desktop et les outils tiers. |

Les trois doivent être des chaînes non vides. Sinon, `AgentConfigError` au load.

### Optionnels : dépendances déclarées

| Paramètre | Type | Effet |
|---|---|---|
| `datasources` | `tuple[str, ...]` | Liste des YAML attendus dans `datasources/` (cf. [Partie III](../part-iii-the-ctx-protocol/15-ctx-datasources-templates.md)). |
| `templates` | `tuple[str, ...]` | Liste des templates Jinja2 attendus dans `templates/`. |
| `secrets` | `tuple[str, ...]` | Liste des secrets attendus dans `ctx.secrets.get(...)`. |
| `tools_required` | `tuple[str, ...]` | Outils natifs dont l'agent dépend (`("file_read", "bash_executor")`). |
| `packages` | `tuple[str, ...]` | Packages PyPI dont l'agent dépend, installés au boot dans le venv isolé. |

Le comportement diffère selon la ressource :

- **`datasources`, `templates`, `secrets`** : gating strict côté SDK Python. Tout accès à un nom non déclaré (`ctx.datasources.get("x")`) lève une erreur immédiate. Ces trois listes sont la définition exhaustive de ce que l'agent peut lire.
- **`tools_required`** : déclaration de **disponibilité** validée au boot par le runtime Rust. Si un outil listé est absent du catalogue, l'agent ne démarre pas (`RequiredToolMissing`). C'est l'application du principe fail-fast au démarrage. Le gating runtime au moment de l'appel relève du moteur de permissions (cf. [chapitre 35](../part-viii-runtime-rust/35-tools-sandbox-permissions.md)), pas du manifeste seul.
- **`packages`** : déclaration des dépendances PyPI installées dans le venv isolé de l'agent.

Bonne pratique : déclarer explicitement tout outil que l'agent appelle, même si le moteur de permissions est plus permissif. Cela permet au boot de signaler immédiatement un outil manquant, à `apollia inspect` de présenter un manifeste lisible, et à l'opérateur de comprendre la surface d'attaque d'un agent installé.

### Optionnels : métadonnées et garde-fous

| Paramètre | Type | Effet |
|---|---|---|
| `tags` | `tuple[str, ...]` | Tags libres pour discovery (`("file", "pdf")`). |
| `agent_type` | `str \| None` | Taxonomie (`"worker"`, `"system"`, …). |
| `memory_namespace` | `str \| None` | Préfixe d'isolation mémoire (par défaut `name`). |
| `shared_memory_namespaces` | `tuple[str, ...]` | Namespaces mémoire partagés en lecture/écriture. |
| `user_memory_write` | `bool` | Autorise les écritures sur `ctx.profile` (par défaut `False`). |
| `step_budget` | `dict \| None` | Override du `StepBudget` runtime (`{"max_steps": 30}`). |
| `check_commands` | `tuple[str, ...]` | Commandes de vérification exécutées après l'agent pour contrôler son résultat (tests, lint). Vérifiées par défaut. |

---

## Exemple : worker avec dépendances et gating

```python
from apollia import agent, skill, DomainError
from apollia.types import Ctx

@agent(
    name="pdf-worker",
    version="1.0.0",
    description="Read, extract and parse PDF files.",
    packages=("pypdf>=4",),
    tags=("file", "pdf"),
    agent_type="worker",
    datasources=("supported_mime_types",),
)
class PdfWorker:
    @skill("pdf.read_text", description="Extract text from a PDF file.")
    async def read_text(self, path: str, ctx: Ctx) -> dict:
        if not path.endswith(".pdf"):
            raise DomainError("UNSUPPORTED", f"Not a PDF: {path}")
        return {"text": _extract(path), "pages": _count(path)}
```

Vous découvrirez le détail de chaque mécanisme dans les parties suivantes : `@skill` au [chapitre 7](07-skill-decorator.md), `ctx.datasources` au [chapitre 15](../part-iii-the-ctx-protocol/15-ctx-datasources-templates.md), `DomainError` au [chapitre 22](../part-v-error-handling/22-domain-errors.md).

---

## Auto-instanciation (ADR-023)

À l'import, `@agent` exécute `cls()` et expose l'instance via `module.agent`. Pas de ligne `agent = MyClass()` à écrire ni à oublier. Le bridge PyO3 récupère cette instance par `getattr(module, "agent")`.

Trois conséquences pratiques :

- **`__init__` doit fonctionner sans argument.** Toute config se lit depuis `ctx` au premier appel, ou via des constantes de classe.
- **Une seule classe `@agent` par module.** Tenter d'en exposer deux lève `AgentConfigError`. Pour deux agents, deux fichiers.
- **L'import a un effet de bord.** `from agents.my_worker import MyWorker` instancie déjà l'agent. Pour des tests purs sans instance, utilisez [`apollia.testing.mock`](../part-vi-testing/24-testing-isomorphic-mock.md).

---

## Contraintes validées au load

Le décorateur applique le principe **fail-fast** : toute incohérence lève `AgentConfigError` à l'import, pas à la première invocation.

- `name`, `version`, `description` non vides.
- Chaque tuple de strings ne contient que des chaînes non vides.
- `memory_namespace` est `None` ou une chaîne non vide.
- `step_budget` est `None` ou un `dict`.
- `__init__` accepte zéro argument requis (en dehors de `self`).
- La classe expose **au moins un** des trois handlers : `@skill`, `@on_message` ou `@orchestrated`.
- `@orchestrated` est mutuellement exclusif avec `@skill` et `@on_message`.
- Un seul `@on_message` par classe.

Toute violation est rattrapée par `apollia inspect` (cf. [chapitre 27](../part-vii-tooling/27-apollia-inspect.md)) avant déploiement.

---

## Inspection après import

Le décorateur stocke deux attributs lisibles sur la classe :

```python
meta = MyWorker.__apollia_agent_meta__
# {'name': 'pdf-worker', 'version': '1.0.0', 'description': '...', 'packages': ('pypdf>=4',), ...}

manifest = MyWorker.__apollia_manifest__
# {'name': '...', 'version': '...', 'skills': [...], 'on_message': None, ...}
```

C'est ce que lit `apollia inspect`, ce que sérialise le bridge PyO3, et ce sur quoi le runtime Rust appuie ses garde-fous.

> **Référence technique :** la spec complète du manifeste, des règles de gating et du dispatch sera dans la page wiki `Briques-SDK` *(wiki disponible prochainement)*.

---

## ADRs

- `ADR-023` : Decorator-first comme contrat unifié
- `ADR-023` : Auto-instanciation et exposition au module

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
