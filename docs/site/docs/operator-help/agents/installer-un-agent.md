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

6. **(Optional) Python dependencies.** If the package declares pip dependencies, a confirmation screen lists them before anything is downloaded. Nothing is installed until you confirm.

   ![Installation dialog, dependency confirmation step: the amber callout, the list of pip packages, the venv note](/img/operator-help/agents-installer-un-agent-2ter.png)

   The packages come from [pypi.org](https://pypi.org) and land in a virtual environment dedicated to this agent, under `~/.apollia/venvs/`. Your system Python is untouched, and uninstalling the package removes them with it. Read the list: it is the one moment where you see exactly what third-party code the agent will run.

7. **(Optional) Webhook configuration.** If you are asked for it, each webhook requires a **secret** (at least 32 characters) that secures the incoming calls. Three cases:
   - If the person who prepared the package gave you a secret, copy it into the field.
   - Otherwise, generate a long and unpredictable one (any robust password will do) and keep it safe, you will need it to configure the service that will call the webhook.
   - The URL shown above the field is the address this webhook will answer on: copy it with the dedicated button.

   ![Installation dialog, configure step: webhook trigger card with endpoint URL and HMAC-SHA256 secret field](/img/operator-help/agents-installer-un-agent-3.png)

8. Click **Install**. Apollia copies the package, registers the agents and activates their triggers. A final screen confirms the installation with the number of agents and triggers created.

   ![Package installed! confirmation screen with agents and triggers counters, Close button](/img/operator-help/agents-installer-un-agent-4.png)

9. Close the dialog. The package appears in the left column, under **My packages**. The agents it contains are also listed under **My assistants** (except those that are only called internally by other agents).

## Verification

- A single file → the agent appears under **My assistants** with a grey dot.
- A package → the package card appears under **My packages** with a counter such as `0/2 agents · 0/1 triggers`. Click it to see the detail.
- The **Start** button (play icon) at the right of the row is enabled.

For the next step, see [Start an agent](demarrer-un-agent.md).

## Replace an agent's file

An installed agent takes a new version of its Python file without being uninstalled first.

1. Open **My Assistants** and select the agent in the left column.

2. At the top right of the detail panel, click **Update**. A file picker opens, filtered on `.py` files, the same one as the install path.

3. Pick the new file. Apollia validates it **before** writing anything: a module the runtime refuses leaves the installed agent exactly as it was.

4. If the agent is running, the header switches to a warning: replacing the file stops it and starts it again on the new version. Confirm with **Replace and restart**, or cancel. A stopped agent skips that step, there is nothing to interrupt.

What is preserved: the install folder, the automatic-start setting and the install date. What changes: the file itself, the `.py` files and the Python sub-folders sitting next to it, and the version, which is read from the new module.

The confirmation message says which version is answering:

- *"… updated to vX. The new version will load the next time it starts."* for an agent that was stopped.
- *"… updated to vX and restarted on the new version."* when the restart went through.

If the restart failed, a red banner replaces the confirmation and names the case: either the file is installed but the agent could not be stopped, and the **previous** version is still answering, or it was stopped and did not come back up, and nothing is answering. The raw cause sits behind **Technical details** in the banner.

## Remove an agent

1. Open **My Assistants** and select the agent.

2. Click **Uninstall** at the top right. The header turns into a confirmation reading *"Permanently delete '\<name\>'?"*.

3. Tick **Also delete agent memory and data** if its memory is to go with it. Left unticked, the memory stays on disk and shows up under *Other* on the **Memory** page, since the agent that named it is gone.

4. Click **Delete**. The row disappears from the list, the database entry and the install folder go with it.

The two confirmations share the same corner of the header: arming the uninstall drops a replacement file you had just picked, so answer one at a time.

## If it does not work

- **"The folder must contain an `agent.toml` file"**: you probably selected a parent folder. Open the folder you were delivered and look for the level where `agent.toml` sits - that is the level to select.
- **Red "Invalid" badge in the preview**: the package descriptor contains an error. The red message under the badge says which one. Send it back to the person who prepared the package, it is their job to fix it.
- **"The secret must be at least 32 characters"**: your secret is too short. Type (or paste) a longer string.
- **The installed agent does not appear**: registration failed silently. Open the logs from the card to read the precise error.
- **The new file is refused on Update**: the banner shows what the loader objected to under **Technical details**. Nothing was written, the agent still runs its previous file. Send the message back to whoever prepared the agent.
- **After an Update, the agent still behaves like the old version**: read the confirmation again. On a stopped agent the new file only loads at the next start, and a failed restart says so explicitly.
- **"trigger" warnings on the final screen**: the agent is installed, but some of its triggers could not be activated. Note the detail shown and report it to the person who prepared the package.

> **For technical profiles:** [Apollia reference](/reference) (`agent.toml` format, native tools that can be enabled, structure of a package).
