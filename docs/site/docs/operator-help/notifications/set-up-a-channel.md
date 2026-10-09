---
title: Set up a notification channel
slug: /operator-help/notifications/set-up-a-channel
sidebar_position: 1
---

# Set up a notification channel

> For operators who want to receive Apollia alerts where they work: on their desktop, in Slack, in Discord, or on a homemade endpoint.

## Prerequisites

- You know where you want to receive the notifications (desktop, Slack, Discord, custom endpoint).
- For a web call, you have the delivery URL ready (a Slack incoming webhook URL, for example).

> **Note:** notifications are a section of the settings. In the sidebar, click **Settings**, then **Notifications**. Every change made there is saved at once: the section has no **Save** button.

## The two-level model

Notification control happens on two levels, and confusing them is the most
frequent cause of a silent channel.

1. **Notified events**, a block of the **Notifications** section: decides which events can trigger a notification. An event switched off here goes out **on no channel at all**, whatever was set elsewhere.
2. **Channel events**, in the window that adds or edits a channel: among the notified events, keeps the ones that go to **this channel**. By default a channel receives every notified event.

In order: first switch on what interests you, then narrow down channel by channel.

## The 7 available events

| Identifier | Label | Description |
|---|---|---|
| `task.completed` | **Task completed** | An agent has just completed its mission. |
| `task.failed` | **Task failed** | An agent task aborted on failure. |
| `task.input_required` | **Decision awaited** | An agent is waiting for your decision. |
| `agent.degraded` | **Agent with reduced capability** | An optional tool is unavailable; the agent continues with reduced capability. |
| `trigger.error` | **Automation not triggered** | A scheduled automation failed to trigger. |
| `llm.backend_down` | **AI unavailable** | The configured AI provider is no longer responding. |
| `chat.user_input_required` | **Question from an agent** | An agent is waiting for your answer to a question. |

The identifier is not shown in the section. It is the name of the event in what a web call receives.

## Switch an event on or off

1. In the sidebar, click **Settings**, then **Notifications**.
2. Spot the **Notified events** block: one line per event, with its label, a short description and a switch.
3. Flip the switch of the event. The change is saved at once.

## At first launch

Apollia creates a default channel on the very first start, a notification on this computer. It is named after the first name of your profile, in the form *"Ada's desktop"*, or *"Desktop"* if your profile does not hold a name yet. You find it in the list, switched on, receiving every notified event. If you delete it, it is not recreated.

## Steps - add a channel

1. In the sidebar, click **Settings**, then **Notifications**. The **Channels** block lists your existing channels.

2. Click **Add a channel**, in the header of the **Channels** block. The window opens.

3. **Name**: type a clear, free-form name (80 characters at most). Examples: *Slack team alerts*, *Supervision webhook*, *Personal desktop*. This name appears in the list and in the recent sends.

4. **Technical identifier (advanced)** (collapsible, optional): as you type the name, Apollia generates an identifier in lowercase with hyphens (`slack-team-alerts`). Open the block to see it and change it if needed. It is offered when adding a channel only, and cannot be changed afterwards.

5. **Type**: choose:
   - **Notification on this computer**: a system notification of your computer.
   - **Web call**: an HTTP call to an external URL. This is the way for **Slack**, **Discord**, **Teams** or any homemade endpoint; paste their incoming webhook URL.

6. For a **Web call** only:
   - **Web call address**: paste the delivery URL.
   - **Headers (JSON)** *(collapsible, optional)*: a JSON object, for example `{"Authorization": "Bearer xyz"}`. An error message shows if the JSON is malformed. Leave it empty if the endpoint does not ask for extra authentication.

7. **Channel events**: **All events** is checked for a new channel, so it receives every notified event. Uncheck it to pick the events that go out on **this channel**. An event switched off in the notified events is greyed and says *Off in the notified events*.

8. **Maximum frequency** *(per channel and per event)*: a dropdown selector:
   - **No limit** (default).
   - **1 per minute (60 s)**.
   - **1 every 5 min (300 s)**.
   - **1 per hour (3600 s)**.
   - **Custom…**: shows the field **Minimum interval (seconds)**, up to 86 400 s (24 h).

   Beyond the limit, Apollia gathers the notifications into a summary. The section on throttling, lower on this page, says how.

9. **Enabled** *(checked by default)*: uncheck to add the channel switched off.

10. Click **Add a channel**. A message confirms it and the channel appears in the list.

## Anatomy of a channel row

Each channel is one row of the **Channels** block, with:

- The **icon** of its type.
- The channel **name**.
- A **detail line**: its type, the events it receives (**All events** or their number) and its maximum frequency.
- The **Test** button.
- A **menu** holding **Edit** and **Delete**.
- The **switch** that turns the channel on or off.

## Test the channel

The test is not in the window that adds a channel; it lives on the **row** of the channel.

1. Spot the row of the channel in the list.
2. Click **Test**. Apollia sends a test notification.
3. The row reads *Sending…*, then *Test passed*, with the time it took, or *Failed*, followed by the reason.

> **Note:** a channel that is switched off cannot be tested.

## Pause a channel

The **switch** sits at the end of the row. One click is enough to flip it; the change is saved at once. A channel switched off receives nothing.

## Edit or delete a channel

- **Edit**, in the menu of the row, opens the same window as the addition, pre-filled. The name stays editable, the technical identifier is not shown. Confirm with **Save**.
- **Delete**, in the same menu, opens a confirmation: the channel will receive nothing more, and the other channels and the notified events do not change.

## What a throttle actually does

The limit acts **per (channel, event type) pair**. When several notifications of
the same type land inside the window:

- the **first** goes out normally;
- the **following ones** are absorbed silently;
- at the end of the window, Apollia sends a **summary**, *"12 'task.completed' events over the last 60 seconds"*.

The other types keep going out unconstrained: an aggressive throttle on `task.completed` will never hold back a `task.input_required`. The detail line of the row shows the limit in force.

## Check what actually went out

At the bottom of the **Notifications** section, the **Recent sends** block lists
the last five notifications Apollia sent, the most recent first. Without any
send yet, a sentence says so.

Each line gives the event, the channels it went through, when it went out, and
whether it arrived: **Sent** or **Failed**.

To get the reason of a failure, run **Test** from the row of the channel: the
row then shows the reason.

## Verification

- After the addition, the channel appears in the **Channels** block with its switch on.
- Clicking **Test** sends a message that lands on the target within a few seconds, and the row reads *Test passed*.
- The first real delivery shows in **Recent sends**.

## If it does not work

- **Desktop notification invisible**: check that the application's notifications are allowed in the system settings (macOS: *Settings → Notifications → Apollia*; Windows 11: *Settings → System → Notifications*).
- **Web call returning 401 / 403**: the URL or the authentication header is wrong. Regenerate the URL on the Slack/Discord/Teams side and paste it again.
- **Web call returning 404 or timing out**: the URL was copied wrong or the endpoint is offline.
- **Web call blocked (SSRF)**: if the error message contains *"SSRF blocked"*, the URL points to a private address (`10.x.x.x`, `192.168.x.x`, `127.0.0.1`, or a cloud metadata endpoint). Apollia refuses these deliveries for safety; use public URLs only.
- **Identifier already taken**: the name is free-form but the technical identifier must be unique. If the addition fails with a conflict, open the **Technical identifier (advanced)** block and change it.
- **Test notification received but not the real ones**: check the **Notified events** block. An event switched off there goes out on no channel, even if that channel checks it.

> **Technical reference:** [Apollia reference](/reference)
