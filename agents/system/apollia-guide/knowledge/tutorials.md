# Apollia OS — walkthroughs (apollia-guide knowledge base)

Five opinionated tutorials. Each tutorial lists the minimal steps a user
needs to go from zero to value, plus the deep-link the Guide should
suggest as an *action button* when the intent matches.

## 1. Create your first automation (daily summary)

Intent keywords: *automate*, *daily*, *summary*, *recap*, *digest*.

1. Open **Automations** → click **New automation**.
2. Describe the goal in natural language (wizard parses via
   `meta_parse_automation`).
3. Pick the agent (or let Apollia suggest one).
4. Confirm schedule + notification channel.

Action button: **Lancer l'assistant de création** → `navigate`
→ `/automations?wizard=open`.

## 2. Install an assistant

Intent keywords: *install*, *add an agent*, *assistant*.

1. Open **Agents** (operator) or **Agents** → **Marketplace** (builder).
2. Pick an assistant from the distributable catalogue.
3. Click **Install** — runtime extracts the bundle to
   `~/.apollia/agents/<name>/` and registers it.

Action button: **Ouvrir Agents** → `navigate` → `/agents`.

## 3. Connect an external tool (MCP or OAuth)

Intent keywords: *connect*, *integration*, *Linear*, *Slack*, *Gmail*, *MCP*.

1. Open **Connections** (`/integrations`).
2. Choose the provider type: OAuth, MCP stdio, MCP HTTP, webhook.
3. Follow the provider flow — secrets stored in the local keyring.

Action button: **Ouvrir les Connexions** → `navigate` → `/integrations`.

## 4. Review pending approvals

Intent keywords: *approve*, *HITL*, *pending*, *inbox*.

1. Open **Inbox** (`/inbox`) — pending HITL approvals are grouped by agent.
2. Inspect the tool call preview (args, predicted impact).
3. Approve, reject, or add a permission rule for future runs.

Action button: **Voir votre Inbox** → `navigate` → `/inbox`.

## 5. Resume or replay onboarding

Intent keywords: *onboarding*, *redo the tour*, *setup again*.

1. Open **Onboarding** (`/onboarding`).
2. Pick a phase to restart, or replay the full guided tour.

Action button: **Relancer l'onboarding** → `navigate` → `/onboarding`.
