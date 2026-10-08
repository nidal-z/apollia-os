# Apollia, tutorials

## Create an agent without code

On the Agents screen, choose "Create an agent" and describe what you want it
to do, in your own words: who it works for, what it reads, what it produces.
Apollia drafts its sheet (mission, examples, accesses). Read it, correct what
is wrong, and create it. You can then talk to it at once, and adjust it later
from its sheet.

## Talk with an agent

Open the Chat screen, pick the agent, and write your request in your own
words. When the agent wants to do something sensitive, such as sending an
email or changing a file, Apollia asks you first; the question also appears on
the To review screen.

## Get a model

Open the AI screen and choose "Discover models". Pick what you want to do;
the models that fit this computer come first, and each card says the memory
it asks. Download one, then load it: the load window says whether it fits.
An agent without a model of its own uses the default model.

## Connect a tool or an account

Open the Connections screen and add a connection from the selection: a
mailbox, a folder, an MCP server, a Google or Microsoft account. Then open the
sheet of an agent and grant it the connection, family of action by family of
action.

## Install a package someone sent you

On the Agents screen, install the `.apo` file. Read the preview: the agents
and workflows it contains, what each one asks to use, and who published it.
Install only if that matches what you expect. From the command line, with the
runtime started:

```bash
apollia-os package preview /path/to/package.apo
apollia-os package install /path/to/package.apo
```

## Adjust an agent

Open its sheet on the Agents screen and choose "Adjust". You can change its
mission, its examples, its model, the accesses it is granted and the sources
it reads. Going back to the package defaults removes your adjustments. From
the command line:

```bash
apollia-os agent show <agent>
apollia-os agent reset <agent>
```

## Run an agent on a file

Open the sheet of the agent and choose "Process a batch". Pick a workbook, a
CSV file or a folder, check the sheet and the header row Apollia proposes, and
launch. You can pause, stop and retry the batch, and export its results.

## Run a workflow

On the Agents screen, launch the workflow and fill in the inputs it asks for.
From the command line:

```bash
apollia-os workflow run <package>/<workflow> --input key=value
```

## Automate an agent

Open the sheet of the agent and choose "Automate", or open the Automations
screen, and choose when it should run: on a schedule, at an interval, when a
file changes, or when a webhook is called.

## See what happened

The sheet of an agent lists what the agent did, with the result of each run,
and what changed on it. The Journal screen shows every decision and every run
in one signed journal, which you can verify and export.
