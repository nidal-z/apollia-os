# ADR-111 — Vision API typée + memory export/import

**Date :** 2026-05-19
**Statut :** Accepté
**Sprint :** Phase release v0.1.0 (refonte SDK)

---

## Contexte

Deux capabilities runtime existantes côté Rust ne sont **pas typées
côté SDK Python** ou pas exposées du tout :

**1. Vision (messages multimodaux LLM)**

L'enum côté Rust `apollia-llm::MessageContent` supporte deux variantes
(`Text(String)`, `Image { base64: String, mime: String }`) côté
providers cloud (Anthropic, OpenAI, Vertex). Côté llama-cpp local
(`project_local_llm_engine`), seul `Text` est consommé — la vision
n'est pas dispo en local mais l'API doit la supporter pour les
providers cloud.

**État observé** :

- `sdk/apollia/stubs/llm.py` (136 LOC) — `complete(messages: list[dict])`
  accepte des dicts amorphes. Aucun TypedDict pour `LlmMessage`,
  `MessageContent`, `ImageContent`. L'auteur qui veut envoyer une
  image doit reverse-engineer le shape depuis le code Rust.
- 1 occurrence dans le repo (`agents/veille-ia/workers/synthesis-worker.py`)
  d'un dict `{"role": "user", "content": [{"type": "text", "text":
  "..."}, {"type": "image", ...}]}` manuel — aucune validation.
- Aucune documentation book/wiki ne montre comment envoyer une image.
- Pas d'helper côté SDK (`apollia.types.image_from_path(p)`, etc.).

**2. Memory export/import**

ADR-066 (sprint 27) spec l'export/import de la mémoire SQLite en JSON
pour backup, debug, migration. Côté Rust, c'est implémenté
(`apollia-memory::export_json`, `import_json`). Côté Python : **pas
branché**. `ctx.memory` (stub) ne l'expose pas.

**État observé** :

- `sdk/apollia/stubs/memory.py:132` ne mentionne ni `export` ni
  `import_data`.
- Aucun agent bundled ne sait sauvegarder sa mémoire en Python (cas
  d'usage légitime : checkpoint d'un agent long-running, debug en
  test).
- L'ADR-066 reste un "écrit mais pas exploité" — un poids mort de la
  spec.

Les deux problèmes sont distincts mais regroupés ici parce que :

- Ils touchent le même service côté SDK (vision = `ctx.llm`, memory
  export = `ctx.memory`).
- Ils ajoutent du typage public partagé (`apollia.types.*`).
- Ils sont chacun trop petits pour un ADR dédié, et trop importants
  pour être ignorés.

## Décision

**Nous adoptons (1) les TypedDicts publics `LlmMessage`,
`MessageContent`, `TextContent`, `ImageContent` dans
`apollia.types.llm`, documentés et utilisables par les agents pour
construire des messages multimodaux ; (2) l'extension `ctx.memory`
avec `export() -> dict` et `import_data(data: dict)`, bouclant ADR-066
côté SDK.**

### Partie 1 : Vision API typée

```python
# apollia/types/llm.py
from typing import Literal, TypedDict

class TextContent(TypedDict):
    type: Literal["text"]
    text: str

class ImageContent(TypedDict):
    type: Literal["image"]
    image_base64: str           # data:image/jpeg;base64,XXX OR raw base64
    mime_type: str              # "image/jpeg", "image/png", "image/webp"

# Future-proofing (v1.1+)
# class ToolUseContent(TypedDict): ...
# class ToolResultContent(TypedDict): ...

MessageContent = TextContent | ImageContent

class LlmMessage(TypedDict):
    role: Literal["system", "user", "assistant"]
    content: str | list[MessageContent]  # str = text-only convenience
```

Helpers publics dans `apollia.types`:

```python
def text(s: str) -> TextContent: ...
def image_from_path(path: str) -> ImageContent:
    """Charge l'image, encode base64, déduit le mime depuis l'extension."""
def image_from_bytes(data: bytes, mime: str) -> ImageContent: ...
def image_from_url(url: str) -> ImageContent:
    """Fetch côté Rust (ctx.tools.invoke('http_request', ...)), encode
    base64, retourne ImageContent."""
```

Usage cible :

```python
from apollia.types import text, image_from_path

messages = [
    {"role": "system", "content": "You analyze screenshots."},
    {"role": "user", "content": [
        text("What's on this screenshot?"),
        image_from_path("/tmp/screen.png"),
    ]},
]
response = await ctx.llm.complete(messages, model="claude-sonnet-4")
```

Côté providers : `apollia-llm` route automatiquement vers les bons
endpoints multimodaux (Anthropic vision, OpenAI gpt-4o, Vertex Gemini).
Local llama-cpp : `complete()` raise `DomainError("VISION_UNSUPPORTED",
"Local llama.cpp does not support vision; switch provider")` au
boundary si une `ImageContent` est détectée. Cf. mémoire
`project_local_llm_engine`.

### Partie 2 : Memory export/import

```python
class MemoryService(Protocol):
    # ... méthodes existantes (remember, recall, search, forget) ...

    async def export(self) -> dict:
        """Exporte toute la mémoire de l'agent (épisodique + sémantique +
        procédurale) en dict JSON-sérialisable. Format ADR-066 v1."""

    async def import_data(self, data: dict) -> None:
        """Importe une mémoire exportée. Merge (pas replace) — les
        entrées existantes avec le même `id` sont écrasées.
        Raise DomainError("MEMORY_INVALID_FORMAT") si shape incorrect."""
```

Format JSON aligné strictement avec ADR-066 (schema_version: 1) — pas
de nouveau format, juste exposition Python du round-trip Rust.

Cas d'usage :

- Test d'agent : `setUp` import un fichier `fixtures/memory.json`,
  `tearDown` clear.
- Checkpoint long-running : agent fait `export()` toutes les 100
  itérations dans `ctx.tools.invoke("file_write", ...)`.
- Migration agent v1→v2 : un agent export, puis un autre import +
  transforme.

## Alternatives considérées

### Pour la vision

**A. Laisser les agents construire des dicts amorphes (statu quo)**
(rejetée)
**Pour :** zéro effort.
**Contre :** pas de typage, pas d'helper, friction réelle, mismatchs
silencieux providers.

**B. Pydantic BaseModel** (rejetée)
**Pour :** validation runtime.
**Contre :** principe #2.

**C. Dataclasses** (rejetée)
**Pour :** stdlib.
**Contre :** moins ergonomique en JSON (sérialisation manuelle), moins
fluide avec dicts existants côté providers.

### Pour memory export/import

**A. Reporter à v1.1** (rejetée)
**Pour :** moins de scope v1.
**Contre :** ADR-066 reste pendant. Pas d'historique de raison de
report. Pertinent immédiatement pour les tests intégration LOT 14.

**B. Limiter à `import_data` (sans export)** (rejetée)
**Pour :** simple.
**Contre :** asymétrique, casse le round-trip qui était le but ADR-066.

### Option retenue (vision + memory)

**Pour :** TypedDicts stdlib, helpers ergonomiques, mémoire boucle la
boucle ADR-066, surface API mineure ajoutée, alignement avec ADR-101
(Ctx Protocol exhaustif).
**Compromis acceptés :** TypedDicts ne valident pas runtime — un dict
mal formé n'est détecté qu'au passage côté Rust LLM provider. Acceptable
(les helpers couvrent 95 % des cas).

## Conséquences

**Positives :**

- Vision exploitable proprement par les agents v1.0 (use case
  immédiat : agent d'analyse de screenshots pour le QA UI desktop).
- ADR-066 enfin bouclé côté Python — round-trip mémoire testable.
- Helpers `image_from_path` / `image_from_bytes` éliminent la boilerplate
  base64 + détection mime.
- IDE autocomplete sur la structure `LlmMessage` — réduction des
  bugs typo de provider.
- Cohérence avec ADR-101 : `ctx.memory` et `ctx.llm` deviennent
  pleinement spec'd.

**Négatives / Compromis :**

- `image_from_path` lit le fichier → couplage I/O dans un helper.
  Acceptable (alternatif : `image_from_bytes(Path(p).read_bytes(),
  mime="...")`).
- Local llama-cpp ne supporte pas vision → l'auteur qui développe en
  local + déploie en cloud doit penser à switcher provider. Documenté.
- Memory export d'un agent à mémoire volumineuse (~MB) renvoie un dict
  Python lourd. Pas d'optimisation streaming en v1.0 (acceptable —
  mémoire SQLite typique d'un agent ne dépasse pas quelques MB).

**À surveiller :**

- Émergence du besoin `ToolUseContent` / `ToolResultContent` pour les
  providers qui exposent du native tool calling — extension naturelle
  des MessageContent en v1.1.
- Format ADR-066 v2 si évolution du schéma mémoire — `schema_version`
  permet la migration.
- Streaming export (genérateur Python) si agents accumulent trop de
  mémoire. Non v1.0.

## Principes architecturaux impactés

- **Principe #2 — Zéro dépendance externe** : préservé (TypedDicts
  stdlib, base64 stdlib, mimetypes stdlib).
- **Principe #6 — Mémoire à initiative de l'agent** : l'agent invoque
  `export()` / `import_data()` explicitement. Pas d'auto-export.
- **Principe #3 — Contrat minimal** : vision = TypedDict léger,
  helpers optionnels.

## Liens

- ADR-101 — `ctx` Protocol (`ctx.llm` et `ctx.memory` enrichis)
- ADR-066 — Memory export/import format (cette ADR le boucle côté SDK)
- ADR-078 — Meta-LLM orchestrator (consommateur naturel de la vision
  multimodale si exploitée)
- Mémoire `project_local_llm_engine` (contrainte llama-cpp text-only
  documentée)
