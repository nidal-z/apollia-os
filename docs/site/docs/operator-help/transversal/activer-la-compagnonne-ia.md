# Enable Apollia Help

> For operators who want a floating assistant one click away, able to answer a quick question without leaving the current screen.

## Prerequisites

- At least one AI provider is configured and ready. The state is visible **in the top left corner**: a **coloured dot to the left of the word *Apollia*** in the top bar shows the combined runtime + LLM state.
  - 🟢 green - healthy runtime + at least one LLM ready, Apollia Help can start.
  - 🟡 amber - no LLM connected, the button is greyed out.
  - 🔴 red - runtime disconnected, nothing will work; quit and relaunch the application.

  To configure an LLM, go to **Settings → LLM models**.

## Steps

1. Press **Cmd+/** (macOS) or **Ctrl+/** (Windows and Linux) from anywhere in the application. There is no button: the shortcut and the command palette are the two ways in. In the palette, opened with **Cmd+K**, the action is **Toggle Apollia Help**.

   Nothing opens as long as no AI provider is ready.

   ![The dashboard, with the Apollia Help panel about to open](/img/operator-help/en/transversal-activer-la-compagnonne-ia-1.png)

2. A **floating panel** opens, docked to the right of the screen by default. A dedicated chat session starts, and a short spinner shows while it is created (1 to 2 seconds).

3. Ask a quick question. Apollia Help answers without interrupting your work on the main page.

   ![Apollia Help panel open, with its welcome message and the input area](/img/operator-help/en/transversal-activer-la-compagnonne-ia-2.png)

4. **Move the panel**: grab the **handle at the top of the panel** (grip-handle icon) and drag it wherever you want. It snaps to the screen edges so it stays reachable.

5. **Resize**: drag the **bottom right corner** to adjust width and height. With that corner focused, the arrow keys resize by 20 pixels a press.

   Your preferred position and size are **remembered** from one session to the next.

6. Apollia Help **knows which page you are looking at**. If you are on *My Assistants* and you ask *"why did this agent fail?"*, it will know what you are referring to.

7. **Collapse into a bubble**: click the Minus icon (−) at the top of the panel. It shrinks into a clickable mini-bubble that you can expand again later. The conversation history is kept.

8. **Close**: click the X icon at the top of the panel, or press **Cmd+/** again from anywhere in the app. The panel disappears but the session stays open, so the next Cmd+/ comes back to the same conversation.

## Keyboard shortcut

- **Cmd+/** (macOS) / **Ctrl+/** (Windows and Linux) - opens and closes the panel. Handy to hide Help quickly during a demo and bring it back afterwards.
- **Cmd+Shift+C** - opens the panel *and* puts the cursor in its input, from anywhere. Use this one when you intend to type straight away.
- **Cmd+Alt** plus an arrow key - moves the panel to another edge. It only works while the panel itself has focus, which is why nothing happens if you try it from the main page.

## Verification

- Clicking the **Apollia Help** button opens the panel in under 2 seconds.
- The panel can be moved and resized without ending up off screen.
- The **Cmd+/** shortcut toggles the display instantly.
- Help answers *"which page am I on?"* by naming the current page correctly.

## If it does not work

- **Help does not answer / no reply arrives**: check the **Apollia state dot** in the top bar. If it is amber, configure an LLM provider in **Settings → LLM models**.
- **The panel opens then shows an error**: the session could not start. Click **Retry** in the panel, or close it and press Cmd+/ again.
- **The panel is invisible even though I already opened it**: it may be minimised into a bubble in a corner of the screen. Look for the Apollia Help bubble; otherwise **Cmd+/** forces it back open.
- **The panel opens in a corner I cannot reach**: give it focus, then use **Cmd+Alt** with an arrow key to bring it back to a visible edge.

> **Concept:** [Apollia explanation](/explanation)
