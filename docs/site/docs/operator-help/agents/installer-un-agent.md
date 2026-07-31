# Install an agent

> For any operator who wants to add an agent to Apollia: starting from a file or a folder you received (by email, through a service engagement, from a Git repository…), register it in the application in a few clicks.

## Prerequisites

- An AI provider is connected (green dot in the top bar).
- You received **one of the two following deliverables**, already present on your disk:
  - **A single Python file** (for example `my-agent.py`) - a simple agent that does one thing.
  - **A complete folder** (for example `mon-package/`) - a set that can contain several agents and their scheduling.

> Apollia has no online catalog and no install-from-the-web (yet): everything starts from a local file. The **Connections** page is only for MCP servers, not for agents.

## Single file or folder: how do you tell?

If the person who delivered the agent gave you **a single `.py` file**, it is a simple agent - use the **New assistant** path.

If they gave you **a folder**, open it: if it contains a file named `agent.toml` at its root, it is a **package**. A package makes it possible to group several agents that work together (a main agent and its assistants) and to schedule their automatic triggering (every morning at 7am, on every new file dropped, and so on). In that case use the **Install a package** path.

If in doubt, ask the person who delivered the agent.

## Steps - Install a single Python file

1. In the sidebar, open **My Assistants**. The page lists your existing assistants, and the **New assistant** button is at the top right.

   ![My Assistants page: list on the left, detail of the selected agent on the right, "New assistant" button at the top right](/img/operator-help/agents-installer-un-agent-1.png)

2. At the top right, click **New assistant**. A file picker opens, filtered on `.py` files.

3. Choose the file you were delivered and confirm. Apollia copies it into its installation folder and registers the agent.

4. The new agent appears in the left column, under **My assistants**, with a grey dot (**stopped** status). You can now start it.

## Steps - Install a package (folder)

1. In the sidebar, open **My Assistants**.

2. At the top right, click **Install a package**. An **Install an agent package** window opens.

3. Click **Choose a folder** and select the folder you were delivered. Apollia reads its descriptor - if something is off (folder without `agent.toml`, invalid manifest), an error message tells you precisely what.

4. **Package preview.** Apollia shows a summary: name, version, author, the list of the agents in the package, their triggers (if any) and the number of dependencies. Take the time to check that it matches what you expect.

   Some packages declare verifications: at the `supervised` level and above, the runtime automatically checks that the agent produced the expected result. This is planned by the package author, you have nothing to configure.

   ![Installation dialog, preview step: Agents and Triggers sections, green Valid badge](/img/operator-help/agents-installer-un-agent-2.png)

   If the package declares a **webhook** trigger, an extra row flags it with an orange "config" badge and the bottom button becomes **Configure →**.

   ![Preview with a webhook trigger requiring configuration, Configure → button](/img/operator-help/agents-installer-un-agent-2bis.png)

5. Click **Install**. If the package contains **webhook** triggers to set up, the button shows **Configure →** instead, see the next step.

6. **(Optional) Webhook configuration.** If you are asked for it, each webhook requires a **secret** (at least 32 characters) that secures the incoming calls. Three cases:
   - If the person who prepared the package gave you a secret, copy it into the field.
   - Otherwise, generate a long and unpredictable one (any robust password will do) and keep it safe, you will need it to configure the service that will call the webhook.
   - The URL shown above the field is the address this webhook will answer on: copy it with the dedicated button.

   ![Installation dialog, configure step: webhook trigger card with endpoint URL and HMAC-SHA256 secret field](/img/operator-help/agents-installer-un-agent-3.png)

7. Click **Install**. Apollia copies the package, registers the agents and activates their triggers. A final screen confirms the installation with the number of agents and triggers created.

   ![Package installed! confirmation screen with agents and triggers counters, Close button](/img/operator-help/agents-installer-un-agent-4.png)

8. Close the dialog. The package appears in the left column, under **My packages**. The agents it contains are also listed under **My assistants** (except those that are only called internally by other agents).

## Verification

- A single file → the agent appears under **My assistants** with a grey dot.
- A package → the package card appears under **My packages** with a counter such as `0/2 agents · 0/1 triggers`. Click it to see the detail.
- The **Start** button (play icon) at the right of the row is enabled.

For the next step, see [Start an agent](demarrer-un-agent.md).

## If it does not work

- **"The folder must contain an `agent.toml` file"**: you probably selected a parent folder. Open the folder you were delivered and look for the level where `agent.toml` sits - that is the level to select.
- **Red "Invalid" badge in the preview**: the package descriptor contains an error. The red message under the badge says which one. Send it back to the person who prepared the package, it is their job to fix it.
- **"The secret must be at least 32 characters"**: your secret is too short. Type (or paste) a longer string.
- **The installed agent does not appear**: registration failed silently. Open the logs from the card to read the precise error.
- **"trigger" warnings on the final screen**: the agent is installed, but some of its triggers could not be activated. Note the detail shown and report it to the person who prepared the package.

> **For technical profiles:** [Apollia reference](/reference) (`agent.toml` format, native tools that can be enabled, structure of a package).
