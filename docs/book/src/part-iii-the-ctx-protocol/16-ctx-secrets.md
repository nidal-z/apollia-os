# `ctx.secrets`

`ctx.secrets` (Protocol `SecretsInterface`) est la surface en lecture seule pour récupérer les credentials d'un agent : clés d'API, jetons OAuth, mots de passe d'intégrations.

Le stockage est chiffré (AES-256-GCM) dans une base locale gérée par `apollia-auth`, avec fallback keyring système (macOS Keychain, libsecret sur Linux). Les agents ne voient jamais le storage : ils demandent par nom et reçoivent la valeur ou `None`.

---

## API

```python
def get(self, key: str) -> str | None: ...
def has(self, key: str) -> bool: ...
```

Usage :

```python
@skill("weather.fetch", description="Fetch weather for a city.")
async def fetch(self, city: str, ctx: Ctx) -> dict:
    api_key = ctx.secrets.get("openweather_api_key")
    if not api_key:
        raise DomainError("CONFIG", "openweather_api_key not configured")
    url = f"https://api.openweathermap.org/data/2.5/weather?q={city}&appid={api_key}"
    response = await ctx.tools.call("web_read", input={"url": url})
    return _parse(response["content"])
```

`get` retourne :

- la valeur (`str`) si le secret est configuré et déclaré dans le manifeste,
- `None` si le secret est déclaré mais pas encore configuré (l'agent doit lever une `DomainError("CONFIG", ...)` ou retomber sur un comportement dégradé).

`has(key)` est un raccourci pour `get(key) is not None`. Utile pour brancher tôt :

```python
if not ctx.secrets.has("anthropic_api_key"):
    raise DomainError("CONFIG", "anthropic backend requires anthropic_api_key")
```

---

## Déclaration dans `@agent`

Les secrets sont **gated** : un agent ne peut accéder qu'à ceux qu'il a déclarés.

```python
from apollia import agent, skill, DomainError

@agent(
    name="weather-worker",
    version="0.1.0",
    description="Fetch weather data from public APIs.",
    secrets=("openweather_api_key",),
)
class WeatherWorker:
    @skill("weather.fetch")
    async def fetch(self, city: str, ctx: Ctx) -> dict: ...
```

`ctx.secrets.get("not_declared")` lève une erreur. Le manifeste est la liste exhaustive des credentials que cet agent peut lire.

`apollia inspect` (cf. [chapitre 27](../part-vii-tooling/27-apollia-inspect.md)) liste les secrets déclarés et signale ceux qui ne sont pas configurés (avertissement, pas erreur).

---

## Comment l'opérateur configure un secret

L'opérateur n'édite pas un fichier. Trois voies :

- **App Desktop :** écran « Secrets », recherche par nom, saisie de la valeur. Stockage chiffré local.
- **CLI :** `apollia-os tools credentials set` (prompt interactif pour la clé et la valeur). Alternative pour les secrets cloud LLM : variable d'environnement passée via `apollia-os llm backends create --api-key-env VAR_NAME`.
- **Flow OAuth :** pour les intégrations OAuth (Google, Microsoft), l'opérateur clique sur « Connect » dans l'UI ; le runtime gère le flow PKCE et stocke le token.

L'agent ne participe à aucun de ces flows. Il consomme la valeur au runtime via `ctx.secrets.get(...)`.

---

## OAuth et refresh transparent

Pour les credentials OAuth, le runtime gère le refresh automatiquement. Si l'access token a expiré, `ctx.secrets.get("google_oauth_access_token")` retourne un token rafraîchi (côté `apollia-auth`, transparent pour l'agent).

L'agent peut donc supposer que la valeur retournée est valide au moment de l'appel. Si le refresh a échoué (refresh token expiré, compte révoqué), `get` retourne `None` et c'est à l'agent de gérer l'absence.

---

## Pas d'écriture

Le Protocol n'expose **aucune** méthode d'écriture. C'est volontaire : un agent compromis ne doit pas pouvoir réécrire les credentials de l'utilisateur. La configuration se fait toujours par un canal séparé (UI, CLI), à l'initiative de l'opérateur.

Pour les écritures vers le profil utilisateur (non sensibles, par exemple `user.preferred_language`), c'est `ctx.profile.set(...)` qui est utilisé, et seulement si l'agent a `user_memory_write=True` dans son manifeste (cf. [chapitre 18](18-ctx-other-services.md)).

---

## Anti-patterns

**Ne pas** hardcoder une clé d'API dans le code (`API_KEY = "sk-..."`). C'est l'erreur de sécurité la plus banale et la plus grave. Utilisez toujours `ctx.secrets.get(...)`.

**Ne pas** logguer la valeur d'un secret. `ctx.logger.info("starting", extra={"key": api_key})` finit dans `tracing` et potentiellement dans un fichier de log. Si vous voulez vérifier la présence, loggez juste le booléen `has`.

**Ne pas** retomber silencieusement quand `get` retourne `None`. Levez une `DomainError("CONFIG", ...)` claire : l'opérateur saura immédiatement qu'il doit configurer le secret.

**Ne pas** passer le secret en argument à un autre agent via A2A. Chaque agent doit déclarer ses propres secrets et les récupérer lui-même. Cela respecte le gating et l'audit trail.

---

## ADRs

- `ADR-016` : OAuth2 PKCE keyring
- `ADR-016` : Linux keyring fallback strategy
- `ADR-024` : SDK secrets read-only gating

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
