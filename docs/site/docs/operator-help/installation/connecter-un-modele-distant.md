---
title: Connect a remote model
slug: /operator-help/installation/connect-a-remote-model
sidebar_position: 6
---

# Connect a remote model

> For any operator who wants to plug Anthropic, OpenAI (or a compatible LM Studio, vLLM), Mistral, or a remote Ollama server into Apollia.

## Prerequisites

- Apollia running.
- For a cloud model, a valid API key with the provider (link to the console further down, per provider).
- For remote Ollama, the URL of an instance reachable from your machine (and `ollama serve` running on the server side).
- Active internet connection, unless Ollama is on your local network.

## What this choice implies

Two points to know before plugging in a remote backend. Apollia also
reminds you of them in the form, but they deserve to be stated here.

**Your prompts leave the machine.** A remote backend receives what Apollia
sends it, which can include file contents, memory entries and workspace
data. This is the expected behaviour of a remote model, and it is
precisely what a local model avoids: with the embedded `llama-server` engine,
nothing goes out. The choice is yours, it just has to be a conscious one.

**Over `http://`, the API key travels in cleartext.** If the endpoint is not
`https://` and points to another machine, the key transits unencrypted over
the network. On a trusted local network or inside a tunnel, that is acceptable.
Otherwise, prefer `https://`. Remote Ollama uses no key, so only the first
point applies to it.

## For which use case

Apollia distinguishes three cases. Choose according to what you want to do:

- **Plug in a cloud provider or an Ollama server**: this is that page. An API key or a URL is enough, nothing to download.
- **Run a local model** in `.gguf` format, served by the embedded `llama-server` engine and managed automatically by Apollia: see [Download local models](telecharger-des-modeles-locaux.md). That is the choice that keeps everything on the machine.
- **See and manage the backends already configured**: open **Settings**, then **LLM Backends**.

## Common steps

1. In the sidebar, open **Settings**, then the **LLM Backends** section.
2. Click **+ Add LLM backend** at the top. A configuration window opens.

   ![Add LLM backend dialog, empty, with the Name and Provider fields](/img/operator-help/installation-connecter-un-modele-distant-1.png)

3. Give it a unique **Name** (lowercase letters, digits and hyphens, for example `claude-anthropic`).
4. Choose the **Provider** from the dropdown.
5. Fill in the provider-specific fields (see the sections below).
6. Click **Test** to validate the connection. A green *"OK · XXX ms"* badge confirms that the provider answers.

   On screen: the configuration dialog with the provider selected, the Endpoint and API Key fields filled in, and a green "OK · 312 ms" badge shown under the Test button.

7. If the test passes, click **Save**. The backend appears in the list.
8. (Optional) Tick **Default backend** so that it is selected automatically when a new chat opens.

## Anthropic

- **Default endpoint**: `https://api.anthropic.com` (leave as is unless you use a custom gateway).
- **Where to get the key**: https://console.anthropic.com, **API Keys** section.
- **Model**: a free-text field. Apollia ships no model list for any provider and does not validate the value, so it is passed to the API as you type it. Take the exact identifier from the provider's own documentation, which is the only current source. Anthropic identifiers are dated and change with each release, so an identifier copied from an older page will be rejected by the API rather than silently downgraded.

  Model identifiers change over time: always take the exact identifier of the current model from the [Anthropic model list](https://docs.anthropic.com/en/docs/about-claude/models).

Apollia applies prompt caching on the Anthropic side automatically.

## OpenAI (or compatible)

- **Default endpoint**: `https://api.openai.com/v1`.
- **Custom endpoint**: use the matching `/v1` URL for LM Studio, vLLM, OpenRouter, Azure OpenAI or any other compatible service.
- **Where to get the key**: https://platform.openai.com, **API keys** section.
- **Model**: free text, as above. Take the identifier from the provider's documentation.

## Mistral

- **Default endpoint**: `https://api.mistral.ai/v1`.
- **Where to get the key**: https://console.mistral.ai, **API keys** section.
- **Model**: free text, as above. Take the identifier from the provider's documentation.

## Remote Ollama

- **Endpoint**: `http://<host>:11434/v1` for a remote server, or `http://localhost:11434/v1` if Ollama runs on your machine.
- **API Key**: optional (useful if you have a reverse proxy with authentication).
- **Service prerequisite**: `ollama serve` must be running on the target host.
- **Models**: see `ollama list` on the host. Examples: `llama3.1:8b`, `qwen2.5:14b`.

For a GGUF model managed directly by Apollia through its embedded engine (without an Ollama daemon), see [Download local models](telecharger-des-modeles-locaux.md).

## Hybrid routing: escalate to a frontier model

Hybrid routing lets Apollia use a local model by default and switch automatically to a frontier (cloud) model for steps that exceed local capabilities, within a cost ceiling.

Configure it in your Apollia configuration file. `[llm.routing.hybrid]` is a
subsection of `[llm.routing]`, whose two keys are required as soon as the table
exists, so the whole block goes in together:

```toml
[llm.routing]
precise = "local-qwen3-8b"      # backend for deep reasoning
fast    = "local-qwen3-4b"      # backend for lightweight extraction

[llm.routing.hybrid]
frontier = "claude-anthropic"   # name of the remote backend to use on escalation
cost_ceiling_usd = 2.00         # ceiling in dollars per routing session
```

With this setting, the runtime evaluates each step: if it needs the frontier model and the ceiling has not been reached yet, escalation happens automatically. Past the ceiling, the runtime falls back to local.

The backend named in `frontier` must be configured and active in **Settings - LLM Backends**.

## Verification

- The backend appears in the list with a green dot.
- Open a chat, select that backend in the picker at the top, send a short message. The answer streams in.
- The Apollia top banner shows the name of the active backend.

## If it does not work

- **401 or 403 error on the test**: your API key is invalid, expired or revoked. Copy the key again from the provider console without stray whitespace.
- **"Model not found" error**: check the exact spelling of the name (case sensitive, for example `claude-3-5-sonnet-20241022` and not `Claude-3.5-Sonnet`).
- **Timeout on cloud**: check your internet connection or the provider status.
- **Ollama unreachable**: check that `ollama serve` is running on the target host and that port 11434 is open. For remote Ollama, test with `curl http://<host>:11434/api/tags` from your machine.
- **No answer in the chat despite a green dot**: see [The AI provider is not responding](../troubleshooting/the-ai-provider-does-not-answer.md).
- **Ceiling reached and the agent does not finish**: the ceiling counts the cumulative cost of a routing session, not of a single run. When it is reached, the runtime degrades to local automatically for the rest of the session, unless `ceiling_action = "hard_stop"` is set, in which case the run ends with an error instead. If the task absolutely needs the frontier model all the way, raise the ceiling or disable hybrid routing by removing the `[llm.routing.hybrid]` section.

> **Technical reference:** [Configuration](/reference/configuration) for the `[llm]` section, its backends and its routing keys, and [LLM sampling defaults](/reference/sampling-defaults) for the one sampling parameter a request actually carries to a model.
