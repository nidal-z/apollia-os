# Onboarding — Tables des etapes du tour guide

> *Tables de reference exhaustives des etapes du tour guide pour chaque profil.*

---

## Profil Operator — 8 etapes

| # | Cle | Route | Action requise | Message Companion (i18n key) | Condition de completion |
|---|---|---|---|---|---|
| 1 | `dashboard` | `dashboard` | observer | `onboarding.tour.op.dashboard.message` | Navigation detectee |
| 2 | `agents` | `agents` | cliquer Demarrer | `onboarding.tour.op.agents.message` | Agent passe en ACTIVE |
| 3 | `chat` | `chat` | envoyer un message | `onboarding.tour.op.chat.message` | Message envoye |
| 4 | `triggers` | `triggers` | creer un trigger | `onboarding.tour.op.triggers.message` | Trigger cree |
| 5 | `approvals` | `approvals` | observer | `onboarding.tour.op.approvals.message` | Navigation detectee |
| 6 | `notifications` | `notifications` | observer | `onboarding.tour.op.notifications.message` | Navigation detectee |
| 7 | `observability` | `observability` | observer | `onboarding.tour.op.observability.message` | Navigation detectee |
| 8 | `graduation` | — | cliquer CTA | `onboarding.tour.op.graduation.message` | Phase `graduation` atteinte |

### Messages companion courts (onboarding_v2.tour.op.step_N)

| Cle | Valeur FR |
|---|---|
| `onboarding_v2.tour.op.step_1` | C'est votre tableau de bord. Vous voyez vos agents actifs et l'activite recente. |
| `onboarding_v2.tour.op.step_2` | Cliquez Demarrer pour activer csv-data-worker, votre premier agent de demo. |
| `onboarding_v2.tour.op.step_3` | Un message est prerempli. Cliquez sur Envoyer ou appuyez sur Entree. |
| `onboarding_v2.tour.op.step_4` | Les declencheurs automatisent vos agents. Le formulaire est prerempli, cliquez Creer. |
| `onboarding_v2.tour.op.step_5` | Certaines actions demandent votre accord. Gerez-les ici avant qu'elles s'executent. |
| `onboarding_v2.tour.op.step_6` | Restez informe des evenements importants via desktop ou webhook. |
| `onboarding_v2.tour.op.step_7` | Suivez l'activite de vos agents, les couts LLM et les journaux d'outils. |
| `onboarding_v2.tour.op.step_8` | Vous maitrisez l'essentiel d'Apollia OS. Explorez librement ! |

---

## Profil Builder — 10 etapes

| # | Cle | Route | Action requise | Message Companion (i18n key) | Condition de completion |
|---|---|---|---|---|---|
| 1 | `dashboard` | `dashboard` | observer | `onboarding.tour.bld.dashboard.message` | Navigation detectee |
| 2 | `agents` | `agents` | demarrer agent | `onboarding.tour.bld.agents.message` | Agent passe en ACTIVE |
| 3 | `agent_detail` | `agents` (detail) | ouvrir detail | `onboarding.tour.bld.agent_detail.message` | Detail ouvert |
| 4 | `memory` | `memory` | chercher namespace | `onboarding.tour.bld.memory.message` | Recherche effectuee |
| 5 | `chat` | `chat` | envoyer un message | `onboarding.tour.bld.chat.message` | Message envoye |
| 6 | `integrations` | `integrations` | observer | `onboarding.tour.bld.integrations.message` | Navigation detectee |
| 7 | `triggers` | `triggers` | observer | `onboarding.tour.bld.triggers.message` | Navigation detectee |
| 8 | `pipelines` | `pipelines` | observer | `onboarding.tour.bld.pipelines.message` | Navigation detectee |
| 9 | `observability` | `observability` | observer | `onboarding.tour.bld.observability.message` | Navigation detectee |
| 10 | `graduation` | — | cliquer CTA | `onboarding.tour.bld.graduation.message` | Phase `graduation` atteinte |

### Messages companion courts (onboarding_v2.tour.bld.step_N)

| Cle | Valeur FR |
|---|---|
| `onboarding_v2.tour.bld.step_1` | Votre espace de travail. Chaque agent est un module Python avec un contrat manifest() + run(). |
| `onboarding_v2.tour.bld.step_2` | Demarrez csv-data-worker. Observez comment le runtime charge le module et l'enregistre. |
| `onboarding_v2.tour.bld.step_3` | Chaque agent expose manifest() pour les metadonnees et run() pour la boucle principale. |
| `onboarding_v2.tour.bld.step_4` | Les agents ecrivent en memoire a leur initiative. Cherchez ce qu'ils ont stocke. |
| `onboarding_v2.tour.bld.step_5` | Envoyez ce message pour voir comment un agent traite une requete liee a la memoire. |
| `onboarding_v2.tour.bld.step_6` | Connectez des outils externes via MCP. Vos agents les appellent sans dependances Python. |
| `onboarding_v2.tour.bld.step_7` | Cron, intervalle, filewatch, webhook. Chaque trigger appelle run() automatiquement. |
| `onboarding_v2.tour.bld.step_8` | Connectez des agents en graphes DAG. Fan-out, fan-in, branches — moteur de pipeline. |
| `onboarding_v2.tour.bld.step_9` | Chaque appel d'outil, ecriture memoire et token LLM est enregistre dans l'audit trail. |
| `onboarding_v2.tour.bld.step_10` | Vous connaissez l'architecture. Creez votre premier agent avec manifest() + run(). |

---

## Spotlight selectors

Les selectors CSS utilisés pour le spotlight de chaque etape sont injectés par le backend via `TourStep.spotlight_selector`.

| Etape | Selector |
|---|---|
| dashboard | `[data-testid="dashboard-header"]` |
| agents | `[data-testid="agents-list"]` |
| agent_detail | `[data-testid="agent-detail-panel"]` |
| memory | `[data-testid="memory-search"]` |
| chat | `[data-testid="chat-input"]` |
| integrations | `[data-testid="integrations-page"]` |
| triggers | `[data-testid="trigger-table"]` |
| pipelines | `[data-testid="pipeline-list"]` |
| observability | `[data-testid="observability-tabs"]` |
| approvals | `[data-testid="approvals-list"]` |
| notifications | `[data-testid="notification-channels"]` |

Pour les etapes de type `observe`, le spotlight est optionnel (`spotlight_selector: null`).
