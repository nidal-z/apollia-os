# Suites d'évaluation

`apollia.testing.mock` couvre le test **fonctionnel** : votre agent appelle les bons outils, gère les bonnes erreurs, écrit aux bons endroits. Une suite d'évaluation (eval suite) couvre la **qualité LLM** : votre agent répond pertinemment à des inputs réalistes, avec un LLM réel branché.

Apollia fournit un harnais d'évaluation intégré : `apollia eval run <suite.toml>`. Une suite est un fichier TOML qui déclare des scénarios avec des assertions. Le harnais les exécute contre le runtime réel et produit un rapport lisible ou JSON.

```bash
# Lancer une suite
$ apollia-os eval run evals/suite-coach.toml

# Consulter l'historique des runs
$ apollia-os eval report evals/suite-coach.toml
```

---

## Quand écrire une eval suite

Pas pour chaque agent. Le critère :

- **Worker déterministe** (pdf-worker, chart-worker, etc.) : tests fonctionnels via `apollia.testing.mock` suffisent. Pas besoin d'eval.
- **Director ou agent conversationnel** : eval suite recommandée. La qualité de la réponse dépend du LLM réel et de l'orchestration, pas seulement du code.

Si votre agent contient un `apollia.react(...)`, un `@on_message` non trivial, ou un `@orchestrated`, écrivez une eval suite.

---

## Structure d'une `suite.toml`

Une suite déclare des scénarios. Chaque scénario invoque un agent ou une skill, et définit une ou plusieurs assertions. Quatre types d'assertion sont disponibles :

| Type | Vérifie |
|---|---|
| `exit_code` | Le code de sortie de la commande CLI sous-jacente |
| `file_exists` | L'existence d'un fichier produit par l'agent |
| `regex` | Un pattern regex dans la sortie de l'agent |
| `llm_judge` | Un critère en langue naturelle évalué par un juge LLM |

```toml
# evals/suite-coach.toml
[suite]
name    = "suite-coach"
agent   = "apollia-guide"    # agent cible (doit etre actif)
version = "0.2.0"            # version de reference pour le rapport

[[scenarios]]
id    = "explain_director_pattern"
input = "Comment fonctionne le pattern Director ?"

[[scenarios.assertions]]
type    = "llm_judge"
expects = "La reponse mentionne le pattern ReAct et la notion d'invocation A2A"

[[scenarios.assertions]]
type  = "regex"
value = "(?i)react|a2a|agent-to-agent"

[[scenarios]]
id    = "refuse_unknown_feature"
input = "Comment activer le mode quantum-encrypted ?"

[[scenarios.assertions]]
type    = "llm_judge"
expects = "L'agent indique honnêtement que la fonctionnalite n'existe pas"

[[scenarios]]
id      = "produce_summary_file"
skill   = "report.generate_summary"     # appel direct de skill plutot que @on_message
args    = { topic = "Apollia OS v0.1" }
timeout = 30

[[scenarios.assertions]]
type = "exit_code"
code = 0

[[scenarios.assertions]]
type = "file_exists"
path = "/tmp/apollia-summary.md"

[[scenarios.assertions]]
type  = "regex"
file  = "/tmp/apollia-summary.md"
value = "(?i)local.first|runtime"
```

Le champ `skill` permet d'invoquer une skill A2A directement (via `apollia-os a2a invoke` en interne). Si `skill` est absent, c'est `input` qui est envoyé via `@on_message`. Les deux chemins produisent un résultat comparable.

---

## Rapport et historique

`apollia eval report` lit les runs précédents et affiche l'évolution du score :

```
  Suite    : suite-coach
  DATE                SCORE   TOTAL   VERSION
  2026-06-04T10:22    87.5%   8       coach-v0.2.1
  2026-06-01T09:15    75.0%   8       coach-v0.2.0
  2026-05-28T14:30    62.5%   8       coach-v0.1.9
```

L'historique est stocké dans `~/.apollia/eval-history.db`. Chaque run est persisté automatiquement. Passez `--json` pour une sortie exploitable par un script CI.

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
| Vérifier que le coach **comprend** la question et répond pertinemment | `apollia eval run` avec assertion `llm_judge` |
| Vérifier que le director ne tombe pas dans une boucle infinie | `apollia.testing.mock` + step budget |
| Vérifier que le director **converge** sur des questions réelles | `apollia eval run` avec assertions `regex` + `llm_judge` |
| Vérifier qu'un fichier est produit avec le bon contenu | `apollia eval run` avec assertions `file_exists` + `regex` |
| Vérifier que la commande CLI retourne un exit code 0 | `apollia eval run` avec assertion `exit_code` |

Un agent bien testé a les deux.

---

## Anti-patterns

**Ne pas** mettre les fichiers `.toml` d'eval dans `tests/` Python standard. Les evals consomment du LLM réel, sont lentes, et ne devraient pas tourner à chaque `pytest`. Convention : `evals/` à la racine du projet agent.

**Ne pas** juger un agent sur un seul scénario. Le LLM est probabiliste : un cas isolé n'est pas représentatif. 20 scénarios minimum pour un signal exploitable.

**Ne pas** mélanger eval et test fonctionnel. Une eval qui assertit aussi sur les outils appelés est plus fragile (deux causes de fail mélangées). Séparez : tests fonctionnels en mock, evals sur qualité de sortie.

**Ne pas** mettre des outputs LLM exacts en attendu (`assert response == "Bonjour Alice"`). C'est fragile. Utilisez un juge ou des assertions souples (`contains`, longueur, présence de termes).

---

## ADRs

- `ADR-023` : Decorator-first
- `ADR-012` : Binary feedback / RLHF (pose le cadre eval terrain)

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
