# Apollia, tutorials

## Talk with an agent

Open the Chat screen, pick the agent, and write your request in your own
words. When the agent wants to do something sensitive, such as sending an
email or changing a file, Apollia asks you first; the question also appears in
the Inbox.

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

Open its card on the Agents screen. You can change its mission, its examples,
the accesses it is granted and its autonomy. Going back to the package
defaults removes your adjustments. From the command line:

```bash
apollia-os agent show <agent>
apollia-os agent reset <agent>
```

## Choose the model of an agent

Open the Models screen to download or connect a model and to set the default
one. An agent without a model of its own uses the default.

## Run a workflow

A workflow runs with the inputs it declares. From the command line:

```bash
apollia-os workflow run <package>/<workflow> --input key=value
```

## Automate an agent

Open the Automations screen and choose when the agent should run: on a
schedule, at an interval, when a file changes, or when a webhook is called.

## See what happened

The Tasks screen lists what agents did, with their results. The Observability
screen shows the runs, their costs, and the audit journal, which you can
verify and export.
