---
title: The AI provider does not answer
sidebar_position: 1
---

# The AI provider does not answer

> For operators whose chat stays frozen, or whose **status dot to the left of the word *Apollia*** in the top bar turns amber or red: get a working AI back in under five minutes.

## Understanding the Apollia status dot

Apollia shows **a single global indicator** for runtime + LLM state, at the top left, next to the word *Apollia* in the breadcrumb:

- 🟢 **green** - healthy runtime and at least one LLM backend ready.
- 🟡 **amber** - healthy runtime but no LLM backend connected.
- 🔴 **blinking red** - runtime disconnected or reconnection in progress.

Hover the dot to see the exact state (native tooltip). This dot is the central reference point of this page.

## Quick checks (in order of likelihood)

### 1. Your internet connection dropped

Cloud providers (Anthropic, OpenAI, Vertex…) are online services. A Wi-Fi or VPN outage cuts the chat off.

**Solution:**
1. Open a browser tab to confirm that you are online.
2. Once the connection is back, wait a few seconds: the status dot returns to green and you can send your message again.

### 2. The backend API key is invalid or expired

A revoked key, an expired one, or one copied with an extra space makes every request fail.

**Solution:**
1. In the sidebar, open **Settings**, then the **LLM backends** section.
2. Find the backend marked `✗ error` in the list. **Hover the status label**: a native tooltip shows the exact error reason (for example *"401 Unauthorized"*, *"connection refused"*).
   ![LLM backends page: a backend card in error, with its red icon and the Error label](/img/operator-help/troubleshooting-le-fournisseur-d-ia-ne-repond-pas-1.png)
3. Click the **Plug icon** (first in the card actions) to test the connection again. A green **OK · *Nms*** badge appears on success, a red **Error** one otherwise. The badge fades after 5 seconds.
4. If the failure persists, click the **pencil icon** to open the edit dialog, paste a valid key from the provider console, then click **Test connection** again at the bottom of the dialog.

### 3. The model name is wrong or unavailable

Providers regularly rename or retire their models. An obsolete identifier causes an error on every call.

**Solution:**
1. Click the **pencil icon** on the failing card.
2. Check the **Model** field: it must match exactly a valid identifier at the provider. Check the provider's up-to-date documentation, identifiers change over the months.
3. Fix it, click **Test connection** in the dialog, then click **Save**.

### 4. The provider service is down

Anthropic, OpenAI and the other cloud providers publish incidents on their public status page. If the test fails while everything looks right on the Apollia side, the cause is on theirs.

**Solution:**
1. Check the status page of the provider concerned.
2. If an incident is ongoing, add a fallback backend (another provider or a local model) in **Settings → LLM backends** so you are not stuck. Routing will automatically pick a backend that is ready.

### 5. The local service is no longer running

If you use a local Apollia model (llama.cpp) or Ollama, the engine must be running on your machine.

**Solution:**
1. For **Ollama**, check that the Ollama service is started on your workstation.
2. For a **local Apollia model**, open **Settings → Model Hub** and confirm that the model is loaded.
3. Click the **Plug icon** on the matching backend card to test again.

## If nothing works

1. **Quit Apollia completely and restart it**: the connection to the provider is tested again automatically at startup. The status dot returns to green if all is well.
2. If the dot stays blinking red (runtime down): the problem is on the Apollia runtime side, not the LLM. Check `~/.apollia/logs/` (or `Settings → Danger Zone → Clear Logs` to start from a clean state).
3. To track **exactly when** the loss happened, open **Inbox → Activity tab**: `llm.backend_down` events appear there with their timestamp if you enabled the matching notification.
4. As a last resort, contact support and include the error message visible when hovering the status + the backend identifier.

> **Technical reference:** [Apollia reference](/reference) - understand how Apollia isolates API keys and how the periodic ping works.
