# Le décorateur `@orchestrated`

`@orchestrated` marque la classe entière comme **agent orchestré** : le runtime Rust (moteur ORIA, pour Observer, Reasoner, Actor) prend en charge la boucle d'exécution. L'auteur ne fournit que le `system_prompt` et, en option, un hook de post-traitement.

C'est la voie la plus déclarative du SDK. ORIA construit un plan à partir du `system_prompt`, l'exécute étape par étape, gère les appels d'outils, et restitue les résultats. L'auteur ne pilote ni la boucle ni le dispatch. Il décrit l'intention et laisse le runtime jouer.

> **Quand l'utiliser ?** Si vous voulez décrire un comportement complet en quelques phrases en langue naturelle (« trier ces emails en trois piles selon X, Y, Z et m'envoyer un résumé »), `@orchestrated` est le bon outil. Si vous voulez garder le contrôle de la boucle d'exécution (appels d'outils explicites, branches conditionnelles, ReAct customisé), préférez `@on_message` couplé à [`apollia.react`](../part-iii-the-ctx-protocol/14-ctx-a2a.md) (cf. [chapitre 4](../part-i-getting-started/04-quickstart-director.md)).

---

## Exemple minimal

```python
from apollia import agent, orchestrated

@agent(
    name="email-triage",
    version="0.1.0",
    description="Sort and route incoming emails.",
)
@orchestrated(
    system_prompt="""
    Sort and route incoming emails using the available tools.

    For each email, choose one of three actions: archive, reply with a
    short acknowledgement, or flag for human review. The tools are
    `inbox.list`, `inbox.archive`, `inbox.reply`, `inbox.flag`.
    """
)
class EmailTriage:
    async def on_plan_complete(self, step_results: dict[str, str], ctx: Ctx) -> str:
        # step_results : {step_id: text} ; les valeurs sont déjà les sorties
        # textuelles des étapes (le bridge convertit HashMap<String, String>
        # en dict[str, str]).
        return "\n\n".join(text for text in step_results.values() if text)
```

Pas de méthode `@skill` ni de `@on_message`. La classe se résume à la description orchestrée et à un hook optionnel.

---

## Signature

```python
def orchestrated(*, system_prompt: str) -> Callable[[C], C]:
```

Un seul argument, obligatoire et keyword-only :

| Paramètre | Type | Rôle |
|---|---|---|
| `system_prompt` | `str` | Description orchestrée : les instructions naturelles que ORIA utilise pour planifier et exécuter. Doit être non vide. |

Le décorateur retourne la classe inchangée. Il y attache simplement la configuration `{"system_prompt": ...}` lue plus tard par `@agent`.

### Hook optionnel : `on_plan_complete`

Si la classe définit une méthode `async def on_plan_complete(self, step_results: dict[str, str], ctx: Ctx) -> str`, ORIA l'appelle avec les résultats textuels de chaque étape une fois le plan terminé. Le retour est la réponse finale présentée à l'utilisateur.

Par défaut (sans hook), ORIA concatène les textes des étapes dans l'ordre. Vous overridez si vous voulez formater, filtrer, ou agréger différemment.

```python
async def on_plan_complete(self, step_results: dict[str, str], ctx: Ctx) -> str:
    # Les valeurs sont des strings (les sorties textuelles des étapes).
    # Pour parser une donnée structurée, c'est à l'agent de l'extraire
    # (regex, JSON dans le texte, etc.).
    counts = {"archived": 0, "replied": 0, "flagged": 0}
    for text in step_results.values():
        for action in counts:
            if action in text.lower():
                counts[action] += 1
                break
    return f"Archived {counts['archived']}, replied {counts['replied']}, flagged {counts['flagged']}."
```

---

## Mutuelle exclusion avec `@skill` et `@on_message`

`@orchestrated` est exclusif des deux autres handlers. La classe est entièrement pilotée par ORIA, donc :

- pas de `@skill` dans la classe, sinon `AgentConfigError` au load ;
- pas de `@on_message` non plus ;
- inversement, si vous voulez le moindre `@skill` ou `@on_message`, retirez `@orchestrated`.

```python
# CASSE au load
@agent(name="bad", version="0.1.0", description="…")
@orchestrated(system_prompt="…")
class Bad:
    @skill("foo.bar")
    async def bar(self, ctx): ...   # ⇒ AgentConfigError "mutuellement exclusif"
```

La règle est cohérente : ORIA gère son propre dispatcher ; cohabiter avec d'autres serait ambigu sur quelle entrée prendre.

---

## ORIA en bref

ORIA (Observer, Reasoner, Actor) est le moteur de plan dynamique côté Rust. Il fonctionne en trois temps :

1. **Observer** : lit le `system_prompt`, les outils disponibles (`tools_required` de `@agent`), et la tâche initiale.
2. **Reasoner** : appelle le LLM pour produire un plan structuré (étapes, dépendances).
3. **Actor** : exécute chaque étape via les outils, peut replanifier si une étape échoue (max 2 replanifications par défaut).

Le détail (cache de plans, observations par étape, intégration HITL) est dans la [Partie VIII](../part-viii-runtime-rust/29-runtime-overview.md) et le wiki.

> **Référence technique :** la page wiki `Briques-ORIA-Engine` détaillera plan cache, observations par étape, intégration HITL *(wiki disponible prochainement)*.

---

## Contraintes validées au load

`@orchestrated` lève `AgentConfigError` si :

- `system_prompt` n'est pas une chaîne non vide ;
- la cible décorée n'est pas une classe.

`@agent` enforce en plus :

- exclusivité avec `@skill` / `@on_message` ;
- présence d'au moins un handler (un agent qui n'aurait ni skill, ni on_message, ni orchestrated lève `AgentConfigError`).

---

## Anti-patterns

**Ne pas** appeler vous-même `ORIA` dans la classe : c'est le runtime qui pilote.

**Ne pas** muter `system_prompt` dynamiquement à chaque tour. Le manifeste est statique : ce que vous écrivez au décorateur est ce qui sera utilisé. Pour ajuster le contexte à l'exécution, exploitez `ctx.datasources` (cf. [chapitre 15](../part-iii-the-ctx-protocol/15-ctx-datasources-templates.md)).

**Ne pas** définir un `__init__` qui prend des arguments. La règle d'auto-instanciation de `@agent` (cf. [chapitre 6](06-agent-decorator.md)) s'applique également ici.

---

## ADRs

- `ADR-022` : ORIA mode orchestré (architecture)
- `ADR-035` : Observation par étape
- `ADR-098` : Decorator-first

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
