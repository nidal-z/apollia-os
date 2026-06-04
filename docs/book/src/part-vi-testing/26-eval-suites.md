# Suites d'évaluation

`apollia.testing.mock` couvre le test **fonctionnel** : votre agent appelle les bons outils, gère les bonnes erreurs, écrit aux bons endroits. Une suite d'évaluation (eval suite) couvre la **qualité LLM** : votre agent répond pertinemment à des inputs réalistes, avec un LLM réel branché.

Apollia ne fournit pas (encore) de framework d'eval intégré. Ce chapitre décrit le pattern qu'on observe sur les agents bundled : un script Python autonome qui orchestre des inputs, capture les outputs, et produit un rapport.

---

## Quand écrire une eval suite

Pas pour chaque agent. Le critère :

- **Worker déterministe** (pdf-worker, chart-worker, etc.) : tests fonctionnels via `apollia.testing.mock` suffisent. Pas besoin d'eval.
- **Director ou agent conversationnel** : eval suite recommandée. La qualité de la réponse dépend du LLM réel et de l'orchestration, pas seulement du code.

Si votre agent contient un `apollia.react(...)`, un `@on_message` non trivial, ou un `@orchestrated`, écrivez une eval suite.

---

## Pattern : eval suite minimale

Un fichier `evals/test_coach.py`, hors du dossier `tests/` Python standard pour qu'il ne soit pas lancé par `pytest` par défaut (les evals consomment du LLM réel).

```python
"""Eval suite for coach agent, runs against a real LLM."""

import asyncio
import json
from pathlib import Path

from apollia import _internal  # accès direct au bridge pour eval
from coach import Coach


SCENARIOS = [
    {
        "id": "explain_director_pattern",
        "input": "Comment fonctionne le pattern Director ?",
        "expectations": [
            "mentionne apollia.react ou ReAct",
            "mentionne ctx.a2a ou A2A",
            "longueur de réponse entre 80 et 600 caractères",
        ],
    },
    {
        "id": "refuse_unknown_feature",
        "input": "Comment activer le mode quantum-encrypted ?",
        "expectations": [
            "indique honnêtement que la fonctionnalité n'existe pas",
            "ne fabule pas une réponse",
        ],
    },
]


async def run_scenario(scenario: dict) -> dict:
    # Bind a real ctx via the runtime bridge (out of scope for this chapter)
    agent, ctx = _internal.build_real_ctx(Coach)
    response = await agent.chat(scenario["input"], history=[], ctx=ctx)
    return {
        "id": scenario["id"],
        "input": scenario["input"],
        "response": response,
        "expectations": scenario["expectations"],
    }


async def main():
    results = []
    for scenario in SCENARIOS:
        results.append(await run_scenario(scenario))
    Path("evals/results.json").write_text(json.dumps(results, indent=2))


if __name__ == "__main__":
    asyncio.run(main())
```

> Note : `_internal.build_real_ctx` n'est pas une API publique stabilisée à v0.5. Le pattern réel pour brancher un ctx réel dans une eval suite passe par `apollia-os a2a invoke <skill_id> --args '<JSON>' --json` (ou `apollia-os run <agent> "<prompt>" --json` pour les agents conversationnels), ou par un test d'intégration côté Rust. Cette mécanique sera documentée dans la page wiki `Testing-Patterns` *(wiki disponible prochainement)*.

---

## Pattern : juge LLM

Pour évaluer une réponse en langue naturelle, le plus simple est un second LLM qui juge :

```python
JUDGE_PROMPT = """\
You are evaluating an AI assistant's response.

Question: {question}
Response: {response}

Expectations:
{expectations}

For each expectation, answer YES or NO (no commentary). Return a JSON
object: {{"expectations_met": [bool, bool, ...]}}
"""


async def judge(scenario: dict, response: str, judge_llm) -> dict:
    prompt = JUDGE_PROMPT.format(
        question=scenario["input"],
        response=response,
        expectations="\n".join(f"- {e}" for e in scenario["expectations"]),
    )
    verdict = await judge_llm.complete([{"role": "user", "content": prompt}])
    return json.loads(verdict.content)
```

Le LLM juge doit être au moins aussi capable que celui qu'il évalue. Idéalement plus capable. Pour évaluer un agent qui tourne sur Haiku, juger avec Sonnet ou GPT-4. Le coût reste modeste si la suite a quelques dizaines de scénarios.

---

## Pattern : rapport et tracking

Le résultat d'une eval est un score (% de scénarios où toutes les expectations sont remplies). Pour suivre l'évolution dans le temps :

```python
import csv
from datetime import datetime


def append_run(score: float, total: int, csv_path: str) -> None:
    with open(csv_path, "a", newline="") as f:
        writer = csv.writer(f)
        writer.writerow([
            datetime.now().isoformat(),
            score,
            total,
            "coach-v0.1.0",
        ])
```

Stockez `evals/history.csv` dans le repo. À chaque changement significatif du code ou du system prompt, relancez la suite et vérifiez que le score ne régresse pas.

---

## Coûts et fréquence

Une eval suite consomme du LLM réel. Comptez :

- ~30 scénarios par run pour avoir un signal stable.
- 1 à 3 appels LLM par scénario (la réponse de l'agent + le jugement).
- Quelques centimes par run sur un LLM cloud, gratuit sur un LLM local.

Cadence recommandée :

- À chaque modification du `system_prompt` d'un agent conversationnel ou director.
- Avant une release.
- En CI nightly si l'eval suite est mature et le coût acceptable.

---

## Quand utiliser `apollia.testing.mock` ET une eval suite

Les deux sont complémentaires.

| Cas | Outil |
|---|---|
| Vérifier que la skill `pdf.read_text` lève `FILE_NOT_FOUND` si le fichier manque | `apollia.testing.mock` + `assert_result_failed` |
| Vérifier que le coach utilise le bon prompt | `apollia.testing.mock` + assertion sur `ctx.llm.prompts` |
| Vérifier que le coach **comprend** la question et répond pertinemment | Eval suite |
| Vérifier que le director ne tombe pas dans une boucle infinie | `apollia.testing.mock` + step budget |
| Vérifier que le director **converge** sur des questions réelles | Eval suite |

Un agent bien testé a les deux.

---

## Anti-patterns

**Ne pas** mettre les evals dans `tests/` Python standard. Elles consomment du LLM, sont lentes, et ne devraient pas tourner à chaque `pytest`. Convention : `evals/` à la racine.

**Ne pas** juger un agent sur un seul scénario. Le LLM est probabiliste : un cas isolé n'est pas représentatif. 20 scénarios minimum pour un signal exploitable.

**Ne pas** mélanger eval et test fonctionnel. Une eval qui assertit aussi sur les outils appelés est plus fragile (deux causes de fail mélangées). Séparez : tests fonctionnels en mock, evals sur qualité de sortie.

**Ne pas** mettre des outputs LLM exacts en attendu (`assert response == "Bonjour Alice"`). C'est fragile. Utilisez un juge ou des assertions souples (`contains`, longueur, présence de termes).

---

## ADRs

- `ADR-023` : Decorator-first
- `ADR-012` : Binary feedback / RLHF (pose le cadre eval terrain)

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
