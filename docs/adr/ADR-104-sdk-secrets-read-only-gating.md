# ADR-104 - API secrets read-only via gating manifest

**Date :** 2026-05-19
**Statut :** Accepté
**Sprint :** Phase release v0.1.0 (refonte SDK)

---

## Contexte

Apollia OS dispose d'un store de credentials chiffré AES-256-GCM
(`crates/apollia-tools/src/credentials.rs`, `ToolCredentialStore`) où
l'opérateur stocke des clés API tierces (Brave Search, OpenWeather,
Notion API token sans OAuth, etc.) via `apollia tools config <tool>
<key>=<value>`. Ces secrets sont aujourd'hui consommés **uniquement**
par les `BuiltinTool` Rust (le tool `web_search` accède à `brave_api_key`
en interne).

**État observé au 2026-05-19** :

- Aucun mécanisme pour qu'un agent Python custom accède à une clé API
  stockée. Le contournement actuel est l'agent qui hardcode sa clé dans
  une variable d'environnement (`OPENAI_API_KEY=...`), ou pire dans son
  code source.
- Les agents bundled de prestation client ont systématiquement besoin
  de **3-5 clés API** spécifiques (provider météo, provider recherche
  web premium, webhook personnalisé) qui ne sont pas des tools Rust
  natifs.
- 2 occurrences dans le repo d'agent qui lit `os.environ.get(
  "SOME_KEY")` - fuite cognitive (l'agent suppose un env, contrairement
  au principe local-first où le credential store est censé être la
  source unique).
- Côté UI desktop, la config des credentials est déjà faite via la page
  `Settings → Outils` ; l'opérateur saisit clé+valeur, c'est chiffré
  dans `governance.db` (ADR-082). Mais cette info ne traverse pas le
  bridge PyO3 vers les agents Python.
- Aucune granularité : si un agent A peut lire `brave_api_key`, peut-il
  lire `openai_api_key` ? Aujourd'hui ce serait tout ou rien - ce qu'on
  ne veut pas.

Côté OAuth (Gmail/Calendar/Drive tokens), c'est une autre histoire :
`apollia-auth` gère les tokens OAuth refresh, et l'accès agent à
`ctx.auth.get_token("google")` reste **délibérément non-exposé en
v1.0** (cf. RELEASE-MOSCOW M4-M5 - les agents accèdent à Gmail/Drive
**uniquement** via les connecteurs natifs `ctx.tools.invoke("gmail.list",
...)` qui font le refresh en interne). Pas de bypass.

## Décision

**Nous adoptons une API `ctx.secrets.get(key)` read-only, gating-protégée
par manifest. L'agent ne voit que les clés explicitement déclarées dans
`@agent(secrets=(...))`. Écriture interdite côté Python. Tokens OAuth
restent encapsulés dans les connecteurs natifs (reportés à v1.1 si
besoin émerge).**

Surface publique :

```python
class SecretsService(Protocol):
    def get(self, key: str) -> str:
        """Récupère un secret depuis le ToolCredentialStore (AES-256-GCM).
        Raise PermissionError si `key` n'est pas dans le manifest gating.
        Raise DomainError("SECRET_NOT_FOUND") si la clé n'a jamais été
        configurée par l'opérateur (cf. `apollia tools config`).
        Synchrone (lookup keyring local <1ms - pas async)."""

    def has(self, key: str) -> bool:
        """Vérifie sans raise si le secret est configuré. Toujours
        gating-protégé."""

    def list(self) -> list[str]:
        """Liste les clés autorisées par le manifest, distinguant celles
        configurées (`has() == True`) de celles non encore renseignées."""
```

Gating manifest :

```python
@agent(
    name="weather-worker",
    version="1.0.0",
    secrets=("openweather_api_key",),
)
class WeatherWorker:
    @skill("forecast")
    async def forecast(self, city: str, ctx) -> dict:
        api_key = ctx.secrets.get("openweather_api_key")
        # ... appel HTTP via stdlib `urllib.request`
```

Règles strictes :

1. **Gating obligatoire** - l'agent ne peut lire QUE les clés déclarées
   dans son manifest. Un `secrets=()` vide rend `ctx.secrets.get(...)`
   inutilisable (PermissionError sur tout).
2. **Lecture seule côté agent** - pas d'API `set`/`delete`. La config
   reste pilotée par `apollia tools config <key>=<value>` (humain) ou
   `apollia tools config <key>` (interactif).
3. **Audit** - chaque lecture est logguée via `ctx.logger` (ADR-106) au
   niveau debug avec le `agent_id` + `key` (PAS la valeur). Activable en
   audit-mode pour traçabilité forensique post-fact.
4. **Convention de nommage** - snake_case obligatoire (`brave_api_key`,
   pas `BraveApiKey`). Documenté dans le book.
5. **Première classe au sein du store** - le `ToolCredentialStore`
   gagne une notion de "scope" : `tool:<id>:<key>` pour les builtin Rust
   (existant) et `agent:<id>:<key>` pour les agents Python. Visible côté
   UI desktop dans `Settings → Outils` (avec section "Secrets agents").
6. **OAuth tokens reportés v1.1** - l'agent ne reçoit pas
   `ctx.auth.get_token("google")` en v1.0. S'il a besoin de Gmail, il
   passe par `ctx.tools.invoke("gmail.list", ...)` qui utilise le
   connecteur natif (cf. ADR-090). Cette restriction est volontaire :
   ne pas exposer un access_token brut à du code Python tiers tant que
   le trust model agent n'est pas durci au-delà d'ADR-083.

## Alternatives considérées

### Option A - Tous les secrets exposés sans gating (rejetée)

**Pour :** simple à implémenter.
**Contre :** un agent malveillant ou bugué peut lire l'OpenAI API key,
le webhook personnel, etc. Pas d'audit possible (toutes les lectures
sont légitimes). Casse le trust model ADR-083.

### Option B - Secrets dans le manifest TOML directement (rejetée)

**Pour :** un seul fichier, pas de gating runtime.
**Contre :** mélange "config secrète" et "code" - viole le local-first
qui prône la séparation stricte (le store chiffré est censé être à part
du code source, justement parce qu'on commit le code). Aussi, casse
quand l'agent est partagé entre machines (mais les secrets restent
machine-locaux).

### Option C - `ctx.secrets[key]` style dict (rejetée)

**Pour :** ergonomique.
**Contre :** moins explicit (un lookup dict raise KeyError, mais le
gating raise PermissionError - les sémantiques sont différentes, mieux
les rendre explicites avec `.get()`).

### Option retenue - `ctx.secrets.get(key)` read-only + gating manifest + OAuth reporté

**Pour :** trust model robuste (gating per-agent), audit possible, lecture
seule = surface attaque minimale, OAuth reporté évite d'exposer
prématurément des tokens à fort blast radius.
**Compromis acceptés :** les agents qui veulent appeler Google Calendar
en API custom (hors connecteur natif) ne peuvent pas en v1.0. Documenté
comme contrainte volontaire post-release pour valoriser le connecteur
natif et l'auditabilité.

## Conséquences

**Positives :**

- Les agents bundled deviennent vraiment portables : zéro `os.environ`
  attendu, tout passe par le credential store chiffré.
- Le gating manifest sert de **contrat lisible** - l'opérateur voit
  exactement quels secrets un agent demande avant de l'installer
  (cf. UI install dialog).
- Lecture synchrone (<1ms keyring lookup) - pas de friction async dans
  les handlers qui font surtout du HTTP.
- Audit trail des lectures secrets activable pour forensique.
- Alignement parfait avec ADR-082 (governance.db comme source unique
  des credentials) et ADR-083 (trust model agents Python).

**Négatives / Compromis :**

- Un agent qui veut accéder à Gmail "à la main" (sans connecteur natif)
  ne peut pas en v1.0. Friction réelle pour ~5 % des cas avancés.
  Acceptable - escape hatch via MCP officiel Notion/Slack/etc.
- L'opérateur doit configurer les secrets pré-install (UI ou CLI). Si
  agent installé avant configuration, `ctx.secrets.get` raise
  `SECRET_NOT_FOUND` au premier appel. Documenter.
- Pas de "secret rotation" automatique en v1.0 - l'opérateur doit
  manuellement remplacer la clé.

**À surveiller :**

- Demandes utilisateur pour `ctx.auth.get_token("google")` - si > 3
  signaux post-release, prioriser en v1.1 avec un mécanisme
  pré-approval per-token.
- Croissance des secrets par agent : si on dépasse 10 secrets dans un
  manifest, c'est un signal qu'il faut un autre design (probablement
  un connecteur dédié).
- Émergence du besoin "secrets shared across agents" (ex. la même
  webhook Slack pour 3 agents) - résoluble par déclaration explicite
  dans chaque manifest (préféré au shared pour traçabilité).

## Principes architecturaux impactés

- **Principe #1 - Local-first** : renforcé. Les secrets ne quittent
  jamais la machine (déjà vrai côté store, maintenant aussi côté agent).
- **Principe #7 - Garde-fous non-négociables** : gating manifest =
  contrainte runtime non-contournable côté agent.
- **Principe #4 - Fail fast** : un secret déclaré mais non-configuré
  est détectable au load via `apollia inspect` (warning visible).

## Liens

- ADR-101 - `ctx` Protocol (ajoute `ctx.secrets`)
- ADR-098 - Decorator-first (`@agent(secrets=...)`)
- ADR-082 - Tool governance (ToolCredentialStore source unique)
- ADR-083 - Trust model agents Python (alignement)
- ADR-064 - OAuth2 PKCE keyring (tokens OAuth séparés du store secrets)
- ADR-090 - Connector trait (les connecteurs natifs accèdent aux OAuth
  tokens sans exposer aux agents)
