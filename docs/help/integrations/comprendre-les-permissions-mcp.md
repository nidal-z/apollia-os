# Comprendre les permissions MCP

> **Référence technique :** [Briques-MCP-Client](https://github.com/nidal-z/apollia-os/wiki/Briques-MCP-Client) · [ADR-082 Tool Governance](https://github.com/nidal-z/apollia-os/wiki/ADR-082)

Apollia applique trois couches de garde-fous quand un agent veut utiliser un outil — natif ou exposé par un serveur MCP. Cette page explique comment elles se composent.

## Couche 1 — Trust levels du serveur

Le **trust level** classe la confiance qu'on accorde à la source d'un serveur MCP :

| Trust level | Signification | Exemples |
|---|---|---|
| `verified_official` | Maintenu par l'éditeur du SaaS lui-même. | Notion MCP, Slack MCP, GitHub MCP, Atlassian Rovo. |
| `community_verified` | Open-source largement adopté, publié dans le registre officiel MCP. | `@modelcontextprotocol/server-postgres`, `mcp-server-fetch`. |
| `community` | Listé au registre mais non vérifié. | Toute publication tierce non-curée. |
| `custom` | Ajouté manuellement par vous (via le wizard ou `mcp-overrides.json`). | Vos serveurs internes. |

Le trust level est affiché comme badge dans le catalogue. Il **n'influence pas automatiquement** les permissions — c'est un indicateur visuel pour vous, l'opérateur.

## Couche 2 — Approval policy par outil

Chaque outil exposé par un serveur ou par un connecteur natif déclare une **approval policy** (cf. ADR-082) :

| Policy | Comportement | Cas d'usage |
|---|---|---|
| `auto_approve` | L'outil s'exécute immédiatement, sans demande utilisateur. | Lectures seules sans effet de bord : `gmail.list_drafts`, `gcal.list_events`, `gdrive.workspace_read`, `outlook.search`. |
| `always_require_approval` | Une demande HITL arrive dans votre boîte de réception. L'outil n'exécute qu'après votre **Approuver**. | Toutes les écritures externes : `gmail.send`, `gcal.create_event`, `outlook.reply`, `gdrive.workspace_write`. |
| `confirm_phrase` | Approbation HITL + il faut **taper une phrase de confirmation** (ex. le nom du fichier à supprimer). | Suppressions irréversibles : `gcal.delete_event`, `outlook_cal.delete_event`. |

Les policies par défaut sont posées par le crate qui expose l'outil. Vous pouvez les **assouplir ou durcir** par tool dans `governance.db` (CLI `apollia tool-governance set-policy …` ou via l'UI **Paramètres → Gouvernance des outils**).

Conseil : laisser les défauts. Le coût d'une approbation supplémentaire est faible ; le coût d'un mail envoyé par erreur peut être élevé.

## Couche 3 — Sovereignty profile

Le profil **souveraineté** est une décision globale qui prime sur tout le reste :

| Profile | Comportement |
|---|---|
| `cloud_allowed` (défaut) | Tous les connecteurs natifs cloud sont actifs. Tous les MCP HTTP/SSE distants sont actifs. |
| `local_only` | Les connecteurs cloud Google/Microsoft sont **désactivés** (les sessions actives sont préservées en lecture). Les serveurs MCP HTTP/SSE distants sont **désactivés**. Seuls les serveurs MCP stdio purement locaux restent disponibles (Filesystem, Memory, SQLite, Git, Time). |

Quand un agent tente d'utiliser un outil bloqué par le profil, il reçoit l'erreur `SovereigntyBlocked` au lieu d'une exécution silencieuse — il peut donc soit demander à l'utilisateur de changer de profil, soit choisir un outil alternatif.

## Spécificités MCP : sampling, elicitation, roots

Trois capabilities MCP côté **client** ont leur propre régime de permissions :

### sampling

Un serveur MCP peut demander au **client** de faire un appel LLM via `sampling/createMessage`. Apollia route ces appels via le `LlmRouter` local, **après une approbation HITL pré-call** :

- Le prompt complet apparaît dans votre boîte de réception sous "À traiter".
- Vous voyez quel serveur source la demande.
- **Approuver** déclenche l'appel LLM ; **Refuser** retourne `cancelled` au serveur.

En plus, chaque serveur a un budget de 100 sampling/heure par défaut — au-delà, l'appel est rejeté côté Apollia sans même demander.

### elicitation

Un serveur peut demander une saisie utilisateur structurée via `elicitation/create` avec un schéma JSON. Apollia route cette demande dans le pipeline existant `chat.user_input_required` — vous voyez un formulaire dynamique dans la boîte de réception. Pas de différence pour vous avec une demande `ask_user` venant d'un agent local.

### roots

Apollia annonce ses **roots filesystem** au serveur via `roots/list`. Le serveur ne peut adresser que les chemins inclus dans cette liste — typiquement le workspace de l'agent actif (`~/.apollia/agents/<slug>/workspace/`) et le répertoire du projet courant.

Sans cette annonce, un serveur MCP stdio mal-conçu pourrait lire `/etc/passwd`. Avec, l'isolation est garantie au niveau protocole.

## Audit trail

Toutes les exécutions d'outils sont loggées dans `governance.db`. Vous pouvez :

- Consulter l'historique : **Paramètres → Historique des actions**.
- Filtrer par outil : `apollia tool-governance audit --tool 'gmail.*' --since '7d'`.
- Exporter pour conformité : `apollia tool-governance export --format csv > audit-2026-05.csv`.
