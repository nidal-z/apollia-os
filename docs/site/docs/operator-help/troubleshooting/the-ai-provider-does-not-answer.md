---
title: The AI provider does not answer
slug: /operator-help/troubleshooting/the-ai-provider-does-not-answer
sidebar_position: 1
---

# The AI provider does not answer

> For operators whose chat stays frozen, or whose **status dot to the left of the word *Apollia*** in the top bar turns amber or red: get a working AI back in under five minutes.

## Understanding the Apollia status dot

Apollia shows **a single global indicator** for the state of its engine and of the AI, at the top left, next to the word *Apollia*:

- 🟢 **green** - *Engine connected · LLM provider available*: the engine runs and at least one model is ready to answer.
- 🟡 **amber** - *Engine connected · no LLM provider configured*: the engine runs but no model can answer.
- 🔴 **blinking red** - *Engine offline - reconnecting*: the application lost its engine and is trying to reach it again.

Hover the dot to read the exact state. This dot is the central reference point of this page.

## Quick checks (in order of likelihood)

### 1. Your network connection dropped

A server is reached over the network. A Wi-Fi or VPN outage cuts the chat off from it, while a model on this computer keeps answering.

**Solution:**
1. Open a browser tab to confirm that you are online, and that the VPN your organisation requires is up.
2. Once the connection is back, wait a few seconds: the status dot returns to green and you can send your message again.

### 2. The key of the server is invalid or expired

A revoked key, an expired one, or one copied with an extra space makes every request fail.

**Solution:**
1. In the sidebar, open **AI**, then choose the server under **Servers**.
2. Read its state at the top: **Active**, **Disabled**, **Unreachable** or **Failing**. Hover the state to read what it means.
3. Click **Test** to ask the server for its models again. The message says what to check: the address, the key, or whether the server is started.
4. If the server refuses the key, click **Edit**, type a valid key, click **Test** again, then **Save**.

### 3. The model is no longer offered by the server

The person who runs a server can rename or retire its models. A model that is no longer served fails on every call.

**Solution:**
1. On the **AI** screen, choose the server: the models it offers today are listed under its state.
2. If the model you used is no longer in the list, open the menu of another model and choose **Use by default**.
3. In a conversation, choose that model in the selector of agents and models.

### 4. The server is down

A server that is stopped, or that an incident took down, shows the **Unreachable** state. If the test fails while the address and the key are right, the cause is on the side of the server.

**Solution:**
1. Ask the person who runs the server, or check the status page of the service concerned.
2. Meanwhile, add another server with **Add a server**, or download a model with **Discover models**, then choose **Use by default** in the menu of the model. An agent whose own model is no longer available says so on its sheet, under **Model**.

### 5. The model on this computer does not answer

A model on this computer is run by the engine built into Apollia; Ollama is a separate service, reached like any other server.

**Solution:**
1. For **Ollama**, check that the Ollama service is started on your workstation.
2. For a **model on this computer**, open **AI** and confirm that the model is listed under **On this computer** and carries the **Default** badge. An empty **Loaded** section is normal: a model loads the first time it is asked a question.
3. For a server, click **Test** on it to check it again.

## If nothing works

1. **Quit Apollia completely and restart it.** The status dot returns to green if all is well.
2. If the dot stays blinking red, the problem is on the side of the engine of Apollia, not of the model. Read the log files in `~/.apollia/logs/`. **Clear Logs**, in the **Danger Zone** block of **Settings → General**, empties that folder to start from a clean state.
3. To be told when the AI stops answering, turn on the **AI unavailable** event in **Settings → Notifications**.
4. As a last resort, contact support and include the state and the message shown on the server, with its name.

> **Technical reference:** [Apollia reference](/reference).
