# `ctx.profile`, `ctx.workspace`, `ctx.stt`, `ctx.notify`

Quatre services regroupés. Aucun ne mérite un chapitre entier, mais chacun est utile dans son contexte.

---

## `ctx.profile` : profil utilisateur canonique

Le profil utilisateur global, stocké au niveau de la machine et lisible par tous les agents. C'est la mémoire « qui est l'utilisateur » (nom, langue préférée, rôle, fuseau horaire, etc.).

```python
@property
def writable(self) -> bool: ...

async def get(self, key: str) -> str | None: ...
async def has(self, key: str) -> bool: ...
async def all(self) -> dict[str, str]: ...
def schema_keys(self) -> list[str]: ...

async def set(self, key: str, value: str) -> None: ...
async def update(self, entries: dict[str, str]) -> None: ...
```

### Lecture (toujours autorisée)

Tout agent peut lire le profil :

```python
name = await ctx.profile.get("user.name")
lang = await ctx.profile.get("user.preferred_language")
all_keys = await ctx.profile.all()
```

### Écriture (gated)

Les méthodes `set` et `update` ne fonctionnent que si l'agent a déclaré `user_memory_write=True` :

```python
@agent(
    name="onboarding",
    version="0.1.0",
    description="…",
    user_memory_write=True,
)
class Onboarding:
    @on_message
    async def chat(self, message: str, history, ctx: Ctx) -> str:
        if "je m'appelle" in message.lower():
            name = _extract_name(message)
            await ctx.profile.set("user.name", name)
            return f"Bonjour {name} !"
```

Si l'agent appelle `set` sans `user_memory_write=True`, le runtime lève une erreur claire à l'invocation. La règle est conservatrice : un seul agent (l'onboarding) devrait avoir l'autorisation d'écriture dans les conditions par défaut.

`writable` (booléen) permet de vérifier le statut :

```python
if ctx.profile.writable:
    await ctx.profile.set("user.preferred_language", "fr")
```

### Convention `user.*`

Les clés du profil utilisateur sont préfixées `user.` par convention (`user.name`, `user.preferred_language`, `user.timezone`). C'est la frontière logique avec `ctx.memory` qui est scopée à l'agent. La règle est documentée dans la mémoire utilisateur globale (cf. principes architecturaux).

---

## `ctx.workspace` : rules et sections d'APOLLIA.md

Quand un workspace contient un fichier `APOLLIA.md` à sa racine, le runtime le parse au boot et expose le contenu via `ctx.workspace`. C'est la voie standard pour mettre des règles ou de la configuration partagée entre agents au niveau projet.

```python
@property
def rules(self) -> str | None: ...           # alias de apollia_md
@property
def apollia_md(self) -> str | None: ...      # contenu brut du fichier

def get(self, title: str) -> str | None: ...  # section par titre

@property
def sections(self) -> list[dict[str, str]]: ...  # toutes les sections
```

Usage typique :

```python
rules = ctx.workspace.rules
if rules:
    system_prompt = f"{base_prompt}\n\nWorkspace rules:\n{rules}"
```

Ou pour récupérer une section précise :

```python
brief = ctx.workspace.get("Marketing brief")
# brief = contenu de la section "# Marketing brief" dans APOLLIA.md, ou None
```

Le pattern de **fallback workspace puis datasource** (cf. [chapitre 15](15-ctx-datasources-templates.md)) :

```python
topics = ctx.workspace.get("Topics") or ctx.datasources.get("topics")
```

Le workspace prend priorité quand il est présent. Le datasource sert de défaut embarqué avec l'agent.

`sections` retourne la liste complète si vous voulez itérer. Chaque entrée est `{"title": "...", "body": "..."}`.

---

## `ctx.stt` : Speech-to-Text

Surface de transcription audio, backed par `apollia-stt` (whisper-rs en local par défaut).

```python
async def transcribe(
    self,
    path: str,
    *,
    language: str | None = None,
    backend: str | None = None,
) -> str: ...

async def status(self) -> dict[str, Any]: ...
```

Usage :

```python
text = await ctx.stt.transcribe("/tmp/meeting.wav", language="fr")
```

`path` est un chemin local vers un fichier audio (WAV, MP3, FLAC, M4A). Le moteur whisper-rs est packagé avec le runtime et tourne sur CPU ou GPU selon la configuration. Pas d'appel réseau par défaut.

`language` est un code ISO 639-1 (`"fr"`, `"en"`). Si omis, whisper auto-détecte (légèrement plus lent).

`status` retourne l'état du backend (modèles disponibles, temps moyen, dernière erreur). Utile pour diagnostiquer.

Cas d'usage : un agent qui reçoit un fichier audio en input et doit le transcrire avant analyse LLM.

---

## `ctx.notify` : notifications

Pour pousser une notification à l'utilisateur (desktop, webhook, autres canaux à venir).

```python
async def publish(
    self,
    message: str,
    *,
    severity: str = "info",
    title: str | None = None,
    channel: str | None = None,
) -> None: ...
```

Usage :

```python
await ctx.notify.publish(
    "Cycle de veille terminé : 3 alertes critiques détectées.",
    severity="warning",
    title="Veille IA",
)
```

`severity` : `"info"`, `"warning"`, `"error"`. Le rendu de la notification dans l'app Desktop varie selon le niveau.

`channel` : `None` (utilise les canaux configurés au niveau session, par défaut le desktop), `"desktop"`, `"webhook"`. Les webhooks sont configurés au niveau opérateur (URL + headers).

`publish` est `async` mais retourne dès que la notification est mise en queue. Pas besoin d'attendre la délivrance pour continuer.

Bonne pratique : utilisez `notify` pour les événements **dignes d'attention humaine** (résultat critique d'une veille, échec d'un workflow long). Pour la trace technique courante, c'est `ctx.logger` ou `ctx.events.emit_thought`.

---

## Anti-patterns

**Ne pas** stocker des informations sensibles dans `ctx.profile` (mots de passe, clés). C'est `ctx.secrets`.

**Ne pas** écrire à `ctx.profile.set` depuis un agent qui n'est pas explicitement le manager du profil. La règle par défaut : seul l'onboarding agent écrit. Les autres lisent.

**Ne pas** transcrire des fichiers volumineux (> 200 Mo) en bloc avec `ctx.stt`. Découpez en chunks audio plus petits si la durée dépasse 30 min : whisper-rs est rapide, mais reste séquentiel.

**Ne pas** spammer `ctx.notify`. Une notification par tâche significative, pas une par sous-étape. Le canal est respecté quand il reste rare.

---

## ADRs

- `ADR-014` : Notifications trait + channel JSON
- `ADR-011` : Global user memory
- `ADR-009` : STT embarqué (whisper-rs)
- `ADR-010` : Workspace context assembly
- `ADR-011` : User profile redesign

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
