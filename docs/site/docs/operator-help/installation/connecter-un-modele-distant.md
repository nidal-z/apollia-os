---
title: Connect a remote model
slug: /operator-help/installation/connect-a-remote-model
sidebar_position: 6
---

# Connect a remote model

> For anyone who wants to plug into Apollia the AI server of their organisation, or a service they already use: Ollama, vLLM, LM Studio, a LiteLLM gateway, or any server with an OpenAI-compatible interface.

## Prerequisites

- Apollia running.
- The address of the server, as it gives it to its clients, with its port.
- A key, only if the server asks for one.
- A network path from your machine to the server.

## What this choice implies

Two points to know before plugging in a server.

**Your prompts leave the machine.** A server receives what Apollia sends it,
which can include file contents, memory entries and workspace data. This is the
expected behaviour of a remote model, and it is precisely what a local model
avoids: with the embedded `llama-server` engine, nothing goes out. The choice is
yours, it just has to be a conscious one.

**Over `http://`, the key travels in cleartext.** If the address does not start
with `https://` and points to another machine, the key transits unencrypted over
the network. On a trusted local network or inside a tunnel, that is acceptable.
Otherwise, prefer `https://`. A server that asks for no key is only concerned by
the first point.

## For which use case

Apollia distinguishes three cases. Choose according to what you want to do:

- **Plug in a server**: this is that page. An address is enough, nothing to download.
- **Run a local model**, served by the embedded engine and managed automatically by Apollia: see [Download local models](telecharger-des-modeles-locaux.md). That is the choice that keeps everything on the machine.
- **See and manage the servers already saved**: open the **AI** screen, under **Servers**.

## Steps

1. In the sidebar, open **AI**.
2. Under **Servers**, click **Add a server**. The form opens in the screen.
3. Type the **Address** of the server, with `http://` or `https://` and its port, for example `http://localhost:11434`. It often ends with `/v1`.
4. Check the **Name**, which Apollia proposes from the address: lowercase letters, digits and hyphens. It is the name the server appears under in Apollia, and it does not change once the server is saved.
5. Fill in the **Key** only if the server asks for one. Apollia keeps it with its secrets and never shows it again.
6. Click **Test**. Apollia asks the server for the list of its models and saves nothing: a message confirms the connection and the number of models found.
7. Click **Save**. The server appears under **Servers**, with its state and its models. The test is not required: a server saved while it is off shows its models as soon as it answers.
8. (Optional) In the list of the models of the server, open the menu of a model and choose **Use by default** so that conversations and agents answer with it when no other model is chosen.

## What a saved server shows

- **Its state**: **Active** when it answers and its models are offered, **Disabled** when you turned it off with the **Active** switch, **Unreachable** when nothing answers at its address, **Failing** when something answers but not with a list of models.
- **Test** asks the server for its models again, and **Edit** reopens the form to change the address or the key.
- **Its models**, each with the **Context window** the server serves it with. A model marked **Short window** is served with a window shorter than the one an agent needs: a long conversation or a task of several steps may be cut short, and only the person who runs the server can enlarge it.
- The menu of the server copies its address or deletes it. Deleting a server forgets its key.

Apollia does not change the loading settings of the models of a server: they belong to the person who runs it.

## Addresses of common servers

- **Ollama**: `http://<host>:11434`, or `http://localhost:11434` when Ollama runs on your machine. The Ollama service must be started on that host.
- **vLLM, LM Studio, LiteLLM and similar**: the address their documentation gives for the OpenAI-compatible interface, which usually ends with `/v1`.
- **An online service with an OpenAI-compatible interface**: the address its documentation gives, with the key it issued to you.

For a model managed directly by Apollia through its embedded engine, without a server, see [Download local models](telecharger-des-modeles-locaux.md).

## Hybrid routing: escalate to a frontier model

Hybrid routing lets Apollia use a local model by default and switch automatically to a more capable remote model for the steps that exceed local capabilities, within a cost ceiling. It is set in the configuration file, not in the application: see [Configure hybrid local + frontier routing](configure-hybrid-routing.md).

## Verification

- The server appears under **Servers** with the **Active** state, and its models are listed.
- Open a chat, choose one of its models in the selector of agents and models, under **Models**, and send a short message. The answer streams in.

## If it does not work

- **The server does not answer**: check that the server is started, that the address and the port are right, and that this computer can reach it on the network. For Ollama, `curl http://<host>:11434/api/tags` from your machine tells whether it is reachable.
- **The server refuses the key**: the key is missing, mistyped or no longer valid. Copy it again without stray whitespace.
- **This address does not answer like an AI server**: something answers, but with no list of models. Use the address of the interface of the server, not the one of its home page.
- **The server offers no model**: it answers, but its list is empty. Install or load a model on the server, then test again.
- **The policy of your organisation refuses this server**: use a server your organisation allows, or ask the person who manages Apollia for you.
- **No answer in the chat although the server is Active**: see [The AI does not answer](../troubleshooting/the-ai-provider-does-not-answer.md).

> **Technical reference:** [Configuration](/reference/configuration) for the `[llm]` section, its backends and its routing keys, and [LLM sampling defaults](/reference/sampling-defaults) for the one sampling parameter a request actually carries to a model.
