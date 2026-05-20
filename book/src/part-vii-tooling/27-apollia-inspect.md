# `apollia inspect`

`apollia inspect <agent.py>` charge un module agent et affiche son manifeste, sans démarrer le runtime. C'est la validation statique d'un agent : ce que voient l'opérateur, l'IDE, et le runtime au boot.

Trois usages typiques :

- **Pendant le développement** : à chaque modification du code de l'agent, lancer `inspect` pour vérifier que le manifeste reste valide et que les schémas inférés sont conformes à ce que vous voulez.
- **Avant un commit** : pré-flight pour s'assurer qu'`apollia agent install` ne va pas refuser.
- **En CI** : intégrer la commande dans le pipeline pour détecter une régression manifeste avant merge.

---

## Usage

```bash
python -m apollia inspect path/to/agent.py
```

Sortie humaine par défaut. Pour un mode machine :

```bash
python -m apollia inspect path/to/agent.py --json
```

Le JSON contient le manifeste complet, la liste des skills avec leurs schemas inférés, et les warnings éventuels (datasource manquant, secret non configuré, etc.).

---

## Ce que la commande valide

Au load du module :

- **Le module charge sans erreur Python** (imports OK, syntaxe OK).
- **`@agent` décore exactement une classe** par module.
- **La classe expose au moins un handler** (`@skill`, `@on_message` ou `@orchestrated`).
- **Les signatures `@skill` sont introspectables** en JSON Schema (pas de type non supporté).
- **Les `skill_id` matchent la regex** `^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$`.
- **Pas de duplicate `skill_id`** sur la même classe.
- **`__init__` accepte zéro argument requis** (pour l'auto-instanciation `@agent`).
- **Les ressources gated existent** : datasources déclarés ont un fichier YAML lisible, templates déclarés ont un fichier dans `templates/`.
- **Les outils déclarés dans `tools_required` ne sont pas des typos** (suggestion `did you mean ...` via Levenshtein).

Tout problème lève `AgentConfigError` à l'import. `inspect` capture l'erreur et l'affiche proprement.

---

## Sortie humaine

```
$ python -m apollia inspect pdf_worker.py

✓ Module loaded: pdf_worker.PdfWorker (v0.1.0)

Manifest:
  name:        pdf-worker
  version:     0.1.0
  description: Read, extract and parse PDF files.
  agent_type:  worker
  packages:    pypdf>=4
  tags:        pdf, worker

Skills (2):
  ├── pdf.read_text
  │     description: Extract text from a PDF, page by page.
  │     input:  {path: str, page_range: str | None = None}
  │     output: {type: object}
  │     examples: 1 (use --json for details)
  └── pdf.count_pages
        description: Count the pages of a PDF file.
        input:  {path: str}
        output: {type: object}
        examples: 1 (use --json for details)

Datasources (0): none declared
Templates (0): none declared
Secrets (0): none declared

✓ Inspection passed.
```

Code de sortie : `0`.

---

## Sortie JSON

```bash
python -m apollia inspect pdf_worker.py --json | jq .
```

```json
{
  "manifest": {
    "name": "pdf-worker",
    "version": "0.1.0",
    "description": "Read, extract and parse PDF files.",
    "agent_type": "worker",
    "packages": ["pypdf>=4"],
    "tags": ["pdf", "worker"],
    "skills": [
      {
        "id": "pdf.read_text",
        "name": "pdf.read_text",
        "description": "Extract text from a PDF, page by page.",
        "input_schema": {
          "type": "object",
          "properties": {
            "path": {"type": "string", "description": "Absolute filesystem path to the .pdf file."},
            "page_range": {"type": "string", "nullable": true}
          },
          "required": ["path"]
        },
        "output_schema": {"type": "object"},
        "examples": [{"path": "/tmp/report.pdf"}],
        "requires_approval": false,
        "dangerous": false
      }
    ],
    "execution_mode": "direct"
  },
  "warnings": []
}
```

Pratique pour piper dans `jq`, intégrer dans un CI, ou nourrir un outil tiers.

---

## Sortie en cas d'erreur

```
$ python -m apollia inspect broken_agent.py

✗ Failed to load: broken_agent.py

AgentConfigError: @skill 'pdf.read_text': method 'read_text' must be 'async def'
  at broken_agent.py:18

Exit code: 1
```

Le message indique précisément la cause et la ligne. C'est ce qui permet le fail-fast au boot du runtime : si `inspect` passe, l'agent chargera ; si `inspect` échoue, l'agent ne chargera pas non plus.

---

## En CI

```yaml
# .github/workflows/agents.yml
- name: Validate agents
  run: |
    for agent in agents/**/*.py; do
      python -m apollia inspect "$agent" || exit 1
    done
```

Ou en pre-commit Git :

```yaml
# .pre-commit-config.yaml
- repo: local
  hooks:
    - id: apollia-inspect
      name: apollia inspect
      entry: python -m apollia inspect
      language: python
      files: ^agents/.+\.py$
```

Coût négligeable : `inspect` charge un module Python, pas de runtime à démarrer.

---

## Quand `inspect` ne suffit pas

`inspect` est statique. Il ne valide pas :

- Le **comportement** runtime de la skill (les vrais appels à `ctx.llm`, `ctx.tools`, etc.). Utilisez `apollia.testing.mock` (cf. [chapitre 24](../part-vi-testing/24-testing-isomorphic-mock.md)).
- La **disponibilité** des secrets configurés (l'opérateur doit les avoir saisis). Inspect signale les manquants en warning.
- La **cohérence** entre les `examples` du `@skill` et l'`input_schema` inféré. Le SDK ne valide pas, l'auteur est responsable (cf. [chapitre 20](../part-iv-llm-friendly-design/20-examples-payloads.md)).

Pour ces dimensions, complétez par des tests fonctionnels et une eval suite.

---

## Anti-patterns

**Ne pas** utiliser `apollia inspect` comme substitut à `pytest`. C'est de la validation statique, pas du test runtime.

**Ne pas** ignorer un warning d'`inspect`. Un datasource déclaré mais absent fera planter l'agent à la première invocation. Mieux vaut le voir maintenant.

**Ne pas** boucler sur `apollia inspect` dans un script bash pour valider un dossier d'agents si la perf compte : préférez `find ... -exec` parallélisé ou un script Python qui appelle directement `apollia.cli.inspect.inspect_command()`.

---

## ADRs

- `ADR-110` : `apollia inspect` CLI

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
