---
title: Enable Apollia Help
slug: /operator-help/transversal/enable-apollia-help
sidebar_position: 4
---

# Enable Apollia Help

> For operators who want help on the screen they are looking at, one click away, without leaving it.

## Prerequisites

- At least one AI provider is configured and ready. The state is visible **in the top left corner**: a **coloured dot to the left of the word *Apollia*** in the top bar shows the combined runtime + LLM state.
  - 🟢 green - healthy runtime + at least one LLM ready, the guide can answer.
  - 🟡 amber - no LLM connected, the guide cannot answer yet.
  - 🔴 red - runtime disconnected, nothing will work; quit and relaunch the application.

  To install a model or connect a server, open **AI** in the sidebar.

## Steps

1. Click the **help icon** (a question mark in a circle) in the top bar. It is on every screen, always at the same place. **Cmd+/** (macOS) or **Ctrl+/** (Windows and Linux) does the same from anywhere, even while typing, and so does the **Open or close the guide** action of the command palette (**Cmd+K**).

2. The **detail sidebar** opens on the right of the screen, on its **Guide** tab. At the top, the guide says which screen you are on and what it is for, and offers actions; each one opens the screen it names. Below, a dedicated chat session starts, and a short spinner shows while it is created (1 to 2 seconds).

3. Ask a quick question. The guide answers in the column while the screen stays usable on its left.

4. The guide **knows which screen you are on**, and which record is open in it. On the Agents screen, with an agent open, asking *"what does this agent do on its own?"* is enough: it knows which agent you mean.

5. **Collapse**: click the collapse icon at the top right of the column, the help icon again, or press **Cmd+/** again. The session stays open, so the next opening comes back to the same conversation.

The help icon also works during the first launch: the top bar and the detail sidebar stay above the welcome window, and the guide is then told you are in the first launch.

## Keyboard shortcut

- **Cmd+/** (macOS) / **Ctrl+/** (Windows and Linux) - opens and collapses the Guide tab.
- **Cmd+Shift+C** - opens the Guide tab *and* puts the cursor in its input, from anywhere. Use this one when you intend to type straight away.

## Verification

- Clicking the help icon opens the Guide tab in under 2 seconds.
- The **Cmd+/** shortcut opens and collapses it instantly.
- The guide answers *"which screen am I on?"* by naming the current screen correctly.

## If it does not work

- **The guide does not answer / no reply arrives**: check the **Apollia state dot** in the top bar. If it is amber, install a model or connect a server in **AI**.
- **The tab shows a documentation page instead of the conversation**: no model can answer yet. The page is the one of the current screen, read without a network, and **Open in the browser** leaves for the same page of the public site. Install a model or connect a server in **AI**, and the conversation comes back.
- **The Guide tab opens then shows an error**: the session could not start. Click **Retry** in the tab, or collapse it and open it again.

> **Concept:** [Apollia explanation](/explanation)
