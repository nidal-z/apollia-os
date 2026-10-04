/**
 * Settings > Notifications, one test per action of the section: list, create,
 * edit, delete and test a channel, switch it, choose the notified events, read
 * the last sends. Each action goes through the existing notification IPC; the
 * test reads the Tauri command and its arguments.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const ipc = vi.hoisted(() => vi.fn(async (_cmd: string, _args?: unknown): Promise<unknown> => null));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc }));

import type { NotificationChannel, NotificationChannelView } from "$lib/types";
import {
  addChannel,
  channelDetailKeys,
  loadChannels,
  loadNotifiedEvents,
  loadRecentSends,
  removeChannel,
  runChannelTest,
  saveChannel,
  sendSucceeded,
  setChannelEnabled,
  toggleNotifiedEvent,
  RECENT_SENDS_LIMIT,
} from "./notificationsSection";
import {
  ALL_EVENTS,
  emptyForm,
  formErrors,
  formFromChannel,
  setAllEvents,
  toggleEvent,
  viewFromListed,
  withLabel,
} from "./channelForm";

function view(over: Partial<NotificationChannelView> = {}): NotificationChannelView {
  return {
    id: "team-alerts",
    label: "Team alerts",
    channel_type: "webhook",
    enabled: true,
    config: { url: "https://hooks.example.invalid/a" },
    events: ["task.failed"],
    min_interval_seconds: 0,
    created_at: "",
    updated_at: "",
    ...over,
  };
}

function listed(over: Partial<NotificationChannel> = {}): NotificationChannel {
  return {
    channel_id: "this-computer",
    label: "This computer",
    type: "desktop",
    enabled: true,
    events: [],
    min_interval_seconds: 0,
    ...over,
  };
}

beforeEach(() => {
  ipc.mockReset();
  ipc.mockResolvedValue(null);
});

describe("list", () => {
  it("lists the channels through list_notification_channels", async () => {
    // GIVEN the runtime holding one channel
    ipc.mockResolvedValueOnce([listed()]);

    // WHEN the section loads its channels
    const channels = await loadChannels();

    // THEN the one command is called and its rows come back unchanged
    expect(ipc).toHaveBeenCalledWith("list_notification_channels");
    expect(channels.map((c) => c.channel_id)).toEqual(["this-computer"]);
  });

  it("describes a row by where, which events and how often", () => {
    // GIVEN a web call keeping two events, limited to one per minute
    const channel = listed({ type: "webhook", events: ["task.failed", "llm.backend_down"], min_interval_seconds: 60 });

    // WHEN its detail line is built
    const detail = channelDetailKeys(channel);

    // THEN it names the type, counts the events and names the preset
    expect(detail).toEqual({
      typeKey: "settings.notifications.type_webhook",
      eventsKey: null,
      eventCount: 2,
      throttleKey: "notifications.throttle_options.minute",
      throttleSeconds: 60,
    });
    // and a channel following every event says so
    expect(channelDetailKeys(listed({ events: [ALL_EVENTS] })).eventsKey).toBe("notifications.all_events");
  });
});

describe("create", () => {
  it("creates a channel on this computer for every event, without a list", async () => {
    // GIVEN the add dialog filled with a name only
    const form = withLabel(emptyForm(), "This computer", false);

    // WHEN it is sent
    await addChannel(form);

    // THEN create_notification_channel receives the slug, the type, and no event list
    expect(ipc).toHaveBeenCalledWith("create_notification_channel", {
      channel: { id: "this-computer", label: "This computer", channel_type: "desktop", enabled: true, config: {} },
    });
  });

  it("creates a web call with its address, headers, events and limit", async () => {
    // GIVEN a web call keeping only the failed tasks, limited to one per hour
    let form = withLabel(emptyForm(), "Team alerts", false);
    form = { ...form, type: "webhook", url: " https://hooks.example.invalid/a ", headers: '{"X-Key":"1"}' };
    form = toggleEvent(setAllEvents(form, false), "task.failed", ["task.completed", "task.failed"]);
    form = { ...form, throttlePreset: "3600" };

    // WHEN it is sent
    await addChannel(form);

    // THEN the request carries all of it
    expect(ipc).toHaveBeenCalledWith("create_notification_channel", {
      channel: {
        id: "team-alerts",
        label: "Team alerts",
        channel_type: "webhook",
        enabled: true,
        config: { url: "https://hooks.example.invalid/a", headers: { "X-Key": "1" } },
        events: ["task.failed"],
        min_interval_seconds: 3600,
      },
    });
  });

  it("refuses a web call without an address and a malformed identifier", () => {
    // GIVEN a web call with no address and an identifier typed by hand
    const form = { ...emptyForm(), type: "webhook" as const, id: "Bad Id" };

    // WHEN the form is checked
    const errors = formErrors(form, "create");

    // THEN both fields are in error
    expect(errors.url).toBe("notifications.field_url_required");
    expect(errors.id).toBe("notifications.field_id_invalid");
  });
});

describe("edit", () => {
  it("fills the dialog from the channel, every event unchecked when it keeps a list", () => {
    // GIVEN a stored web call keeping one event
    // WHEN the edit dialog is filled from it
    const form = formFromChannel(view());

    // THEN its address is there and "every event" is off
    expect(form.url).toBe("https://hooks.example.invalid/a");
    expect(form.allEvents).toBe(false);
    expect(form.events).toEqual(["task.failed"]);
  });

  it("goes back to every event with the wildcard, through update_notification_channel", async () => {
    // GIVEN a channel keeping one event, edited back to every event
    const original = view();
    const form = setAllEvents(formFromChannel(original), true);

    // WHEN it is saved
    await saveChannel(original, form);

    // THEN the update sends the wildcard, an absent list keeping the stored one
    expect(ipc).toHaveBeenCalledWith("update_notification_channel", {
      id: "team-alerts",
      channel: {
        channel_type: "webhook",
        enabled: true,
        config: { url: "https://hooks.example.invalid/a" },
        events: [ALL_EVENTS],
      },
    });
  });

  it("opens a listed channel with what the list knows", () => {
    // GIVEN a listed channel following every event
    // WHEN its view is built for the dialog
    const v = viewFromListed(listed());

    // THEN it has no stored list and no configuration
    expect(v.events).toBeNull();
    expect(v.config).toEqual({});
  });

  it("switches a channel off from its row", async () => {
    // GIVEN a channel switched on
    // WHEN its switch is turned off
    await setChannelEnabled("this-computer", false);

    // THEN only `enabled` is sent
    expect(ipc).toHaveBeenCalledWith("update_notification_channel", {
      id: "this-computer",
      channel: { enabled: false },
    });
  });
});

describe("delete", () => {
  it("deletes a channel through delete_notification_channel", async () => {
    // GIVEN a confirmed deletion
    // WHEN the channel is removed
    await removeChannel("team-alerts");

    // THEN the command receives its id
    expect(ipc).toHaveBeenCalledWith("delete_notification_channel", { id: "team-alerts" });
  });
});

describe("test", () => {
  it("tests a channel through test_notification_channel and returns the outcome", async () => {
    // GIVEN a channel the runtime reaches in 42 ms
    ipc.mockResolvedValueOnce({ channel_id: "this-computer", status: "ok", error: null, latency_ms: 42 });

    // WHEN it is tested
    const result = await runChannelTest("this-computer");

    // THEN the command carries the channel id and the outcome comes back
    expect(ipc).toHaveBeenCalledWith("test_notification_channel", { channelId: "this-computer" });
    expect(result.status).toBe("ok");
    expect(result.latency_ms).toBe(42);
  });

  it("reads a rejected test as a failed one, with its reason", async () => {
    // GIVEN a runtime that rejects the test
    ipc.mockRejectedValueOnce(new Error("connection refused"));

    // WHEN the channel is tested
    const result = await runChannelTest("team-alerts");

    // THEN the row has a failure to show
    expect(result).toEqual({ channel_id: "team-alerts", status: "error", error: "connection refused", latency_ms: null });
  });
});

describe("notified events", () => {
  it("reads the notified events in the order of the section", async () => {
    // GIVEN the runtime returning events out of order, one unknown
    ipc.mockResolvedValueOnce(["llm.backend_down", "pipeline.done", "task.completed"]);

    // WHEN the section loads them
    const events = await loadNotifiedEvents();

    // THEN get_notification_events is read and the list follows the section order
    expect(ipc).toHaveBeenCalledWith("get_notification_events");
    expect(events).toEqual(["task.completed", "llm.backend_down"]);
  });

  it("switches one event off and saves the whole list at once", async () => {
    // GIVEN two events on
    // WHEN one is switched off
    const next = await toggleNotifiedEvent(["task.completed", "task.failed"], "task.failed");

    // THEN set_notification_events receives the remaining one
    expect(next).toEqual(["task.completed"]);
    expect(ipc).toHaveBeenCalledWith("set_notification_events", { events: ["task.completed"] });
  });
});

describe("recent sends", () => {
  it("reads the last sends through get_notification_logs", async () => {
    // GIVEN one failed send
    const entry = {
      id: "1",
      event_name: "task.failed",
      task_id: null,
      sent_at: "2026-10-04T10:00:00Z",
      channels: { "team-alerts": "error" },
      error: null,
    };
    ipc.mockResolvedValueOnce([entry]);

    // WHEN the block loads
    const sends = await loadRecentSends();

    // THEN the log is read with the block's limit and the send reads as failed
    expect(ipc).toHaveBeenCalledWith("get_notification_logs", { limit: RECENT_SENDS_LIMIT });
    expect(sendSucceeded(sends[0])).toBe(false);
    expect(sendSucceeded({ ...entry, channels: { "team-alerts": "sent" } })).toBe(true);
  });
});
