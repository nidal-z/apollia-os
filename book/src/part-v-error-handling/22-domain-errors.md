# `DomainError`

Quand une skill échoue pour une raison métier connue (fichier introuvable, format non supporté, ressource absente), l'agent lève une `DomainError`. Le boundary du dispatcher SDK la trappe et la transforme en `AIPResult` avec `status="failed"`, code et message stables. L'agent ne manipule jamais `AIPResult` directement.

Ce chapitre couvre comment lever une `DomainError`, comment choisir les codes, et ce qui se passe côté caller (CLI, API, A2A).

---

## Pattern

```python
from apollia import agent, skill, DomainError
from apollia.types import Ctx
from pathlib import Path


@agent(name="pdf-worker", version="0.1.0", description="Read PDFs.")
class PdfWorker:
    @skill("pdf.read_text", description="Extract text from a PDF.")
    async def read_text(self, path: str, ctx: Ctx) -> dict:
        p = Path(path)
        if not p.exists():
            raise DomainError(
                "FILE_NOT_FOUND",
                f"PDF not found: {path}",
                details={"path": path},
            )
        if p.suffix.lower() != ".pdf":
            raise DomainError(
                "WRONG_EXTENSION",
                f"Expected .pdf, got {p.suffix}",
                details={"path": path, "suffix": p.suffix},
            )
        return {"text": _extract(p)}
```

Trois arguments : un `code` stable, un `message` lisible par un humain, un `details` optionnel pour donner du contexte structuré.

---

## Le contrat

```python
class DomainError(AgentError):
    def __init__(
        self,
        code: str,
        message: str,
        details: dict[str, Any] | None = None,
    ) -> None: ...
```

### `code`

Identifiant stable, snake_case ou SCREAMING_SNAKE_CASE. C'est sur ce code que les callers brancheront (parseur de logs, agent qui consomme A2A, UI Desktop qui propose une remédiation).

Bonnes valeurs :

- `"FILE_NOT_FOUND"`
- `"WRONG_EXTENSION"`
- `"RATE_LIMITED"`
- `"INVALID_FORMAT"`
- `"DEPENDENCY_MISSING"`

Mauvaises valeurs :

- `"Erreur 42"` (pas stable)
- `"Le fichier n'existe pas"` (c'est le message, pas le code)
- `"ERR_001"` (opaque, oblige à consulter une table)

**Le code est public.** Ne le changez pas sans bump de version majeur de l'agent.

### `message`

Phrase lisible par un humain, qui apparaît dans la CLI, les logs, et la timeline d'observabilité. Incluez les valeurs concrètes (path, count, limit) pour faciliter le debug.

### `details`

Dict JSON-sérialisable optionnel. Contient les valeurs qu'un programme appelant peut vouloir lire pour décider d'une remédiation. Restez en types primitifs.

---

## Ce que voit le caller

Côté CLI :

```bash
apollia invoke pdf-worker pdf.read_text path=/tmp/nope.pdf
# Failed (FILE_NOT_FOUND): PDF not found: /tmp/nope.pdf
# exit code: 1
```

Côté A2A (cf. [chapitre 14](../part-iii-the-ctx-protocol/14-ctx-a2a.md)) :

```python
result = await ctx.a2a.invoke("pdf.read_text", input={"path": "/tmp/nope.pdf"})
# Lève une exception côté caller qui propage l'erreur typée.
# L'agent caller peut catcher et brancher :
try:
    result = await ctx.a2a.invoke("pdf.read_text", input={"path": p})
except DomainError as exc:
    if exc.code == "FILE_NOT_FOUND":
        await ctx.notify.publish(f"Fichier introuvable : {p}", severity="warning")
        return {"skipped": True}
    raise
```

Côté API REST : le retour est un JSON `{"status": "failed", "error": {"code": "FILE_NOT_FOUND", "message": "...", "details": {...}}}`.

Le client (UI Desktop, autre agent, script bash) peut brancher proprement.

---

## Choisir un bon code

Trois critères :

1. **Stable.** Le code ne change pas entre deux versions de l'agent. Si la cause change, c'est un nouveau code, pas une refonte du précédent.
2. **Spécifique.** Un code par cause d'échec. `"INVALID_PAYLOAD"` est trop général, préférez `"MISSING_REQUIRED_FIELD"`, `"VALUE_OUT_OF_RANGE"`, etc.
3. **Compréhensible hors contexte.** Quelqu'un qui lit `FILE_NOT_FOUND` dans un log comprend tout de suite. Pas besoin de consulter une table.

Pattern utile : préfixer par le domaine si vous avez beaucoup de codes (`"PDF_FILE_NOT_FOUND"`, `"PDF_ENCRYPTED"`). Mais ne sur-préfixez pas si le contexte est évident depuis le skill_id.

---

## Trappes du boundary

Le boundary du dispatcher (`sdk/apollia/_internal/dispatch.py`) trappe :

- **`DomainError`** : devient `AIPResult.failed(code, message, details)`.
- **`NeedHumanInput`** : devient `AIPResult.input_required(prompt, context)`. Voir [chapitre 23](23-need-human-input.md).
- **`PayloadError`** : levée par le validateur de signature quand un payload ne matche pas le schéma. Devient `AIPResult.failed("PAYLOAD", message, details)`.
- **`SchemaError`** : levée à l'import quand une signature ne peut pas être inférée. Bloque le boot.
- **`AgentConfigError`** : levée à l'import quand `@agent` ou `@skill` est mal configuré. Bloque le boot.

Toute autre exception non typée (`ValueError`, `KeyError`, `RuntimeError`) est trappée et **transformée en `DomainError("UNHANDLED", str(exc), {"traceback": ...})`** par le boundary, avec un log d'erreur structuré. C'est un filet de sécurité, pas une voie recommandée. Préférez lever explicitement `DomainError`.

---

## Anti-patterns

**Ne pas** retourner manuellement un dict d'erreur :

```python
# NON
return {"status": "failed", "error": "file not found"}

# OUI
raise DomainError("FILE_NOT_FOUND", "PDF not found: " + path)
```

Le boundary uniformise le format. Retourner un dict customisé contourne la trappe et casse la convention.

**Ne pas** ré-utiliser le même code pour deux causes différentes :

```python
# NON
raise DomainError("INVALID_INPUT", "path is empty")
raise DomainError("INVALID_INPUT", "path is too long")

# OUI
raise DomainError("PATH_EMPTY", "path must not be empty")
raise DomainError("PATH_TOO_LONG", f"path is {len(path)} chars (max 4096)")
```

**Ne pas** lever une exception générique (`raise Exception("oops")`). Le boundary la trappe en `"UNHANDLED"`, le message est moins parlant, et le code n'est pas stable.

**Ne pas** mettre le secret dans `details`. Tout ce qui passe par `details` est sérialisé et journalisé.

---

## ADRs

- `ADR-100` : Exceptions typées au boundary
- `ADR-109` : AIPResult interne au SDK

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
