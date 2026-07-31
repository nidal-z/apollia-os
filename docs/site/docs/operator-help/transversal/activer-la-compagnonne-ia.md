# Enable Apollia Help

> For operators who want a floating assistant one click away, able to answer a quick question without leaving the current screen.

## Prerequisites

- At least one AI provider is configured and ready. The state is visible **in the top left corner**: a **coloured dot to the left of the word *Apollia*** in the top bar shows the combined runtime + LLM state.
  - 🟢 green - healthy runtime + at least one LLM ready, Apollia Help can start.
  - 🟡 amber - no LLM connected, the button is greyed out.
  - 🔴 red - runtime disconnected, nothing will work; quit and relaunch the application.

  To configure an LLM, go to **Settings → LLM models**.

## Steps

1. In the sidebar, find the **Apollia Help** button (Apollia logo, at the bottom of the sidebar).

   The button is **greyed out and not clickable** as long as no AI provider is ready. On hover, a tooltip spells it out: *"Configure an LLM model to enable contextual help"*.

   ![The dashboard, with the sidebar and the entry point to Apollia Help](/img/operator-help/transversal-activer-la-compagnonne-ia-1.png)

2. Click the button. A **floating panel** opens, docked to the right of the screen by default. A dedicated chat session starts, and a short spinner shows while it is created (1 to 2 seconds).

3. Ask a quick question. Apollia Help answers without interrupting your work on the main page.

   ![Apollia Help panel open, with its welcome message and the input area](/img/operator-help/transversal-activer-la-compagnonne-ia-2.png)

4. **Move the panel**: grab the **handle at the top of the panel** (grip-handle icon) and drag it wherever you want. It snaps to the screen edges so it stays reachable.

5. **Resize**: drag the **bottom left corner** to adjust width and height.

   Your preferred position and size are **remembered** from one session to the next.

6. Apollia Help **knows which page you are looking at**. If you are on *My Assistants* and you ask *"why did this agent fail?"*, it will know what you are referring to.

7. **Collapse into a bubble**: click the Minus icon (−) at the top of the panel. It shrinks into a clickable mini-bubble that you can expand again later. The conversation history is kept.

8. **Close**: click the X icon at the top of the panel (or use the **Cmd+/** / **Ctrl+/** shortcut from anywhere in the app). The panel disappears but Help stays *enabled*, and another Cmd+/ reopens it instantly on the same session.

9. **Disable entirely**: click the **Apollia Help** button in the sidebar again. This time it disables the feature: the panel closes and the history is closed with it. The preference is persisted, so the next time you open it a fresh session starts.

## Keyboard shortcut

- **Cmd+/** (macOS) / **Ctrl+/** (Windows and Linux) - toggles the panel **without touching the enabled state**. Handy to hide Help quickly during a demo and bring it back afterwards.

## Verification

- Clicking the **Apollia Help** button opens the panel in under 2 seconds.
- The panel can be moved and resized without ending up off screen.
- The **Cmd+/** shortcut toggles the display instantly.
- Help answers *"which page am I on?"* by naming the current page correctly.

## If it does not work

- **Help does not answer / no reply arrives**: check the **Apollia state dot** in the top bar. If it is amber, configure an LLM provider in **Settings → LLM models**.
- **The sidebar button is greyed out**: no LLM is ready. The tooltip on hover explains it. See the previous point.
- **The panel opens then shows an error**: the session could not start. Click **Retry** in the panel, or close and reopen it from the sidebar.
- **The panel is invisible even though I already opened it**: it may be minimised into a bubble in a corner of the screen. Look for the Apollia Help bubble; otherwise **Cmd+/** forces it back open.
- **The panel opens in a corner I cannot reach**: press and hold the **Apollia Help** button in the sidebar (or disable then re-enable it) to reset the default position (right side).

> **Concept:** [Apollia explanation](/explanation)
