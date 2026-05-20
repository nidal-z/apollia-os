# `apollia new` : scaffolding

`apollia new <name> --type <type>` génère un squelette d'agent prêt à éditer : un fichier `.py` avec le décorateur correct, une signature de handler, et un fichier de test associé.

C'est la voie recommandée pour démarrer un nouvel agent. Vous obtenez immédiatement un agent qui passe `apollia inspect`, et un test pytest qui passe `pytest`. Reste à écrire la logique.

---

## Usage

```bash
python -m apollia new my-agent --type worker
# Created my_agent.py
# Created test_my_agent.py
```

Quatre `--type` disponibles, alignés sur les 4 patterns canoniques (cf. [Partie I](../part-i-getting-started/02-quickstart-conversational.md)) :

| `--type` | Skeleton produit | Décorateur principal |
|---|---|---|
| `worker` | Agent A2A avec une skill par défaut | `@agent + @skill` |
| `conversational` | Agent `@on_message` | `@agent + @on_message` |
| `react` | Agent qui utilise `apollia.react` | `@agent + @skill` (avec `react` à l'intérieur) |
| `orchestrated` | Agent piloté par ORIA | `@agent + @orchestrated` |

Le nom doit être en `kebab-case` : `my-agent`, `email-router`, `weather-checker`. Le générateur transforme en `snake_case` pour le module Python et en `PascalCase` pour la classe.

---

## Anatomie du squelette généré

Le fichier généré pour `--type worker` ressemble à :

```python
"""Worker Agent my-agent."""

from apollia import DomainError, agent, skill
from apollia.types import Ctx


@agent(
    name="my-agent",
    version="0.1.0",
    description="Worker Agent my-agent",
    agent_type="worker",
    memory_namespace="my-agent",
    tags=("my-agent", "worker"),
)
class MyAgent:
    @skill("my-agent.default", description="Default skill for my-agent")
    async def default(self, input_text: str, ctx: Ctx) -> dict:
        if not input_text:
            raise DomainError("INVALID_INPUT", "input must be non-empty")
        return {"echo": input_text}
```

Et le fichier de test :

```python
"""Tests for my-agent."""

import pytest
from apollia.testing import mock, assert_result_completed

from my_agent import MyAgent


@pytest.mark.asyncio
async def test_default_skill_returns_echo() -> None:
    agent, ctx = mock(MyAgent)
    result = await agent.invoke_skill("my-agent.default", input_text="hello")
    assert_result_completed(result)
```

Vous éditez la skill, vous éditez le test, vous lancez `pytest test_my_agent.py`. Cycle de développement court.

---

## Status de l'outil dans v0.1.0

L'outil est fonctionnel mais les templates ont accumulé un peu de dette pendant la refonte SDK v0.5. Trois divergences connues à corriger dans une révision proche :

1. Certains templates ajoutent encore une ligne `agent = MyAgent()` à la fin du fichier, qui devient redondante avec l'auto-instanciation du décorateur `@agent` (cf. [chapitre 6](../part-ii-the-decorators/06-agent-decorator.md)).
2. Le template `orchestrated` utilise une ancienne forme `@orchestrated` en method decorator (au lieu de class decorator).
3. Le template `worker` passe certains arguments à `@skill` qui ne sont plus dans la signature courante (`name=`, `input_modes=`).

**Recommandation pratique pour la release v0.1.0** : utilisez `apollia new` pour avoir le squelette de base, puis nettoyez à la main en suivant le pattern canonique du [chapitre correspondant de la Partie II](../part-ii-the-decorators/06-agent-decorator.md). Lancez `apollia inspect` après nettoyage pour vérifier que le manifeste est conforme.

Un patch dédié au raffraîchissement des templates est prévu pour une révision post-v0.1.0.

---

## Personnaliser le squelette

Le générateur est un script Python (`sdk/apollia/cli/scaffold.py`). Si vous voulez un template d'agent spécifique à votre équipe (par exemple un worker qui inclut systématiquement `ctx.notify` pour les alertes), copiez le template existant, ajustez-le, et utilisez-le comme point de départ pour vos nouveaux agents.

Apollia ne propose pas (encore) de mécanisme d'override des templates par configuration. Le pattern courant : maintenir vos propres templates dans un dossier `templates-agents/` de votre repo, et créer un nouveau agent par copie + recherche/remplacement.

---

## Au-delà du squelette

Une fois le fichier généré, les étapes typiques :

1. **Écrire la logique de la skill** : remplacez le `return {"echo": input_text}` par votre vraie logique.
2. **Ajouter des `@skill` supplémentaires** si l'agent expose plusieurs capacités.
3. **Déclarer les dépendances** : `packages=(...)` pour les PyPI, `tools_required=(...)` pour les outils natifs, `secrets=(...)` pour les credentials.
4. **Annoter les paramètres** avec `Annotated[T, "..."]` et `examples=` (cf. [Partie IV](../part-iv-llm-friendly-design/19-annotated-descriptions.md)).
5. **Valider** : `python -m apollia inspect mon_agent.py`.
6. **Tester** : `pytest test_mon_agent.py`.
7. **Installer** : `apollia agent install ./mon_agent.py`.

---

## Anti-patterns

**Ne pas** considérer le squelette généré comme un produit fini. C'est un point de départ. Vous ajustez le manifeste, vous écrivez la logique, vous testez.

**Ne pas** garder la skill `default` si elle n'a pas de sens dans votre domaine. Renommez (`pdf.read_text`, `email.triage`, etc.) en respectant le namespacing dot-snake_case.

**Ne pas** retirer le test généré sans le remplacer. Avoir au moins un test fonctionnel par agent est une bonne discipline.

---

## ADRs

- `ADR-098` : Decorator-first (le template suit le canon)

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
