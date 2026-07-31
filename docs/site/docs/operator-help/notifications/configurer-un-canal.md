# Set up a notification channel

> For operators who want to receive Apollia alerts where they work: on their desktop, in Slack, in Discord, or on a homemade endpoint.

## Prerequisites

- You know where you want to receive the notifications (desktop, Slack, Discord, custom endpoint).
- For a Webhook channel, you have the delivery URL ready (a Slack incoming webhook URL, for example).

> **Note:** the **Notifications** page is reachable directly from the sidebar in **Operator** mode as well as in **Builder** mode. No need to switch to Builder to set up your channels any more.

## The two-level model

Notification control happens on two levels, and confusing them is the most
frequent cause of a silent channel.

1. **Global events**, the section at the top of the **Notifications** page: decides which events the system tracks and routes. An event unchecked here will go out **on no channel at all**, whatever was set elsewhere.
2. **Per-channel events**, in the Create or Edit dialog of a channel: among the globally enabled events, filters the ones that go to **this channel**. Empty list = this channel receives every global event.

In order: first enable globally what interests you, then narrow down channel by channel.

## The 7 available events

| Identifier | Label | Description |
|---|---|---|
| `task.completed` | **Task completed successfully** | An agent has just completed its mission. |
| `task.failed` | **Task failed** | An agent task aborted on failure. |
| `task.input_required` | **Approval required** | An agent is waiting for your decision (HITL). |
| `agent.degraded` | **Agent degraded** | The agent is running but something optional did not come up: either an optional declared tool that was not resolved, or a failed install of its Python environment. |
| `trigger.error` | **Trigger error** | A scheduled automation failed to trigger. |
| `llm.backend_down` | **LLM provider unavailable** | The configured AI provider is no longer responding. |
| `chat.user_input_required` | **Agent question** | An agent is waiting for your answer to a question (`ask_user`). |

## Enable or disable an event globally

1. In the sidebar, click **Notifications**.
2. Spot the **Global events** section at the top of the page: a grid of checkboxes, one per type, with a label, a short description and the technical identifier.

   ![Global events section, grid of 7 checkboxes with label, description and technical identifier](/img/operator-help/notifications-choisir-les-evenements-notifies-1.png)

3. Check or uncheck according to what you want surfaced.
4. Click **Save**. A *"Global events saved"* toast confirms. **Without that click, nothing is applied**: the checkmarks stay local to the screen.

## At first launch

Apollia automatically creates a default **Desktop** channel on the very first start: it is named *"Bureau de {your first name}"* (or *"Bureau"* if your profile does not hold a first name yet). You find it in the list, enabled, with no filtered events (so it receives every global event). If you delete it, it is not recreated.

## Steps - create a new channel

1. In the sidebar, click **Notifications**. The list shows your existing channels, plus a *"Global events"* section at the top.

2. Click **+ New channel** at the top right. The **Create channel** dialog opens.
   ![Notifications page - Global events section, channel list, "New channel" button at the top right](/img/operator-help/notifications-configurer-un-canal-1.png)

3. **Name** (first field, auto-focused) - type a clear, free-form name (spaces, accents and emojis accepted, 80 characters max). Examples: *Slack team alerts*, *Supervision webhook*, *Personal desktop*. This name will appear in the list, in the delivery history and in the toasts.

4. **Technical identifier** (collapsible section, optional) - as you type the name, Apollia automatically generates an identifier in *kebab-case* (`slack-team-alerts`). Open the *"Technical identifier"* block to see it and customise it if needed; it stays validated against the *lowercase + hyphens* regex. Once set, it never changes.

5. **Type** - choose:
   - **Desktop** - system notification of your computer (toast / native notification centre).
   - **Webhook** - HTTP JSON POST to an external URL. This is the way for **Slack**, **Discord**, **Teams** or any homemade endpoint; just paste their incoming webhook URL.

6. For **Webhook** only:
   - **Webhook URL** - paste the delivery URL.
   - **Headers (JSON)** *(optional)* - a monospace textarea that accepts a JSON object, for example `{"Authorization": "Bearer xyz"}`. The format is validated as you type; an error message shows if the JSON is malformed. Leave it empty if the endpoint does not ask for extra authentication.

7. **Events** *(visible as soon as global events are active)* - check the event types that must go out on **this channel**. Each row shows:
   - A **human label** (*Task failed*, *Approval required*…).
   - A **short description** under the label.
   - The **technical identifier** in smaller monospace (useful if you parse the payload on the Slack/Discord side).

   Leave everything unchecked to receive **every global event** on this channel. The detail of the 7 types and the two-level logic are at the top of this page.

8. **Throttle notifications** *(anti-spam, per channel and per event type)* - a dropdown selector:
   - **No limit** (default).
   - **1 per minute** (60 s).
   - **1 every 5 min** (300 s).
   - **1 per hour** (3600 s).
   - **Custom…** - shows a numeric field to enter an interval between 1 s and 86 400 s (24 h).

   The limit is computed **per (channel, event type) pair**. Example: setting 60 s lets the first *Task completed* through, ignores the following ones for 60 s, and sends at the end of the window a **summary** of the form *"12 ‘task.completed’ events over the last 60 seconds"*. Meanwhile, *Approval required* keeps going through without throttling (different pair).

9. **Enabled** *(checked by default)* - uncheck to create the channel paused.

10. Click **Create channel**. A toast confirms the creation and the channel appears in the list.

## Anatomy of a channel card

Once created, each channel is rendered as a card with:

- A **thin horizontal accent bar at the top** - *info* blue for a Desktop, *primary* blue for a Webhook; greyed out if the channel is disabled.
- To the left of the title, a **coloured thumbnail** with the type icon (screen for Desktop, webhook plug for Webhook).
- The channel **name** as the title, its **technical identifier** in monospace as subtext (if you customised the name), then a **Desktop** or **Webhook** badge.
- To the right of the title, the **on/off toggle** to pause / resume.
- Below, the filtered **event pills** (or the wording *"All events"* if the list is empty).
- At the far right of the events row, a small **⏱ … s** indicator appears if a throttle is configured.
- In the card footer, separated by a thin rule, **three action icons** side by side: paper plane (Test), pencil (Edit), red bin (Delete). Hover each icon to see its tooltip.

![A notification channel card, with its accent bar, channel icon, name and identifier, and its badges](/img/operator-help/notifications-configurer-un-canal-2.png)

## Test the channel

The test is not in the creation dialog; it lives on the **channel card** once created.

1. Spot the channel card in the list.
2. Click the **paper plane icon** in the card footer (tooltip *"Test"*). Apollia sends a generic test notification.
3. A green **OK · xxx ms** badge shows in the card footer for about 5 s, plus a *"Test notification sent"* toast. On failure, a red badge carries the endpoint error message.

> **Note:** the **Test** icon is disabled as long as the channel is paused (toggle off).

## Pause a channel

The **on/off toggle** sits directly in the card header. One click is enough to flip it; the change is persisted immediately (toast *"Channel {name} enabled"* / *"disabled"*). The accent bar at the top of the card turns from its type colour to grey.

## Edit or delete a channel

- **Pencil icon** *(tooltip "Edit")* - opens the same dialog as the creation, pre-filled. The name stays editable, the technical identifier does not. Any change must be confirmed with **Save**.
- Red **bin icon** *(tooltip "Delete")* - opens a confirmation modal (*"Delete the channel {id}? This action cannot be undone"*). No undo is offered.

## What a throttle actually does

The limit acts **per (channel, event type) pair**. When several notifications of
the same type land inside the window:

- the **first** goes out normally;
- the **following ones** are absorbed silently;
- at the end of the window, Apollia sends a **summary**, *"12 ‘task.completed’ events over the last 60 seconds"*.

The other types keep going out unconstrained: an aggressive throttle on `task.completed` will never hold back a `task.input_required`. As soon as a limit is set, the channel card shows a **⏱ … s** indicator to the right of the events row.

## Check what actually went out

The delivery history is **not** on the Notifications page. That page now carries
only a line pointing elsewhere. Open the **Inbox** and go to the **Notifications
sent** tab.

The list gives you, per entry, when it went out, which channel took it, which
event triggered it, and whether the send succeeded or failed.

The failure reason is not shown there. To get it, run **Test** from the channel
card on the Notifications page.

## Verification

- After creation, the channel card appears in the list with its **accent bar at the top** coloured by type and its **toggle enabled** to the right of the title.
- Clicking the **Test** icon sends a message that lands on the target channel within a few seconds.
- The channel appears in the **Inbox**, tab **Notifications sent**, from the first real delivery.

## If it does not work

- **Desktop notification invisible**: check that the application's notifications are allowed in the system settings (macOS: *Settings → Notifications → Apollia*; Windows 11: *Settings → System → Notifications*).
- **Webhook returning 401 / 403**: the URL or the authentication header is wrong. Regenerate the URL on the Slack/Discord/Teams side and paste it again.
- **Webhook returning 404 or timing out**: the URL was copied wrong or the endpoint is offline.
- **Webhook blocked (SSRF)**: if the error message contains *"SSRF blocked"*, the URL points to a private address (`10.x.x.x`, `192.168.x.x`, `127.0.0.1`, or a cloud metadata endpoint). Apollia OS refuses these deliveries for safety; use public URLs only.
- **Name already taken**: the name is free-form but the technical identifier must be unique. If the creation fails with a conflict, open the *"Technical identifier"* section and change it manually.
- **Test notification received but not the real ones**: check the *Global events* section at the top of the **Notifications** page - an event unchecked globally will go out on no channel, even if that channel checks it locally.

> **Technical reference:** [Apollia reference](/reference)
