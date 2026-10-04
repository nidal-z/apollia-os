/**
 * What the Notifications section of the settings does, one function per
 * action, each through the existing notification IPC. The section component
 * only renders their results.
 */
import type {
  ChannelTestResult,
  NotificationChannel,
  NotificationChannelView,
  NotificationLogEntry,
} from "$lib/types";
import {
  createNotificationChannel,
  deleteNotificationChannel,
  getNotificationEvents,
  listNotificationChannels,
  setNotificationEvents,
  testNotificationChannel,
  updateNotificationChannel,
} from "$lib/ipc/notifications";
import { getNotificationLogs } from "$lib/ipc/inbox";
import {
  createRequest,
  isAllEvents,
  updateRequest,
  type ChannelForm,
} from "$lib/notifications/channelForm";
import { NOTIFICATION_EVENT_TYPES } from "$lib/notifications/event-labels";

/** How many sends the "Recent sends" block shows. */
export const RECENT_SENDS_LIMIT = 5;

/** The channels, in the order the runtime lists them. */
export function loadChannels(): Promise<NotificationChannel[]> {
  return listNotificationChannels();
}

/** The notified events of the application, in the order of the section. */
export async function loadNotifiedEvents(): Promise<string[]> {
  const events = await getNotificationEvents();
  return NOTIFICATION_EVENT_TYPES.filter((e) => events.includes(e));
}

/** Create a channel from the dialog. */
export function addChannel(form: ChannelForm): Promise<NotificationChannelView> {
  return createNotificationChannel(createRequest(form));
}

/** Save the dialog over an existing channel. */
export function saveChannel(
  original: NotificationChannelView,
  form: ChannelForm,
): Promise<NotificationChannelView> {
  return updateNotificationChannel(original.id, updateRequest(original, form));
}

/** Switch a channel on or off, from the switch of its row. */
export function setChannelEnabled(id: string, enabled: boolean): Promise<NotificationChannelView> {
  return updateNotificationChannel(id, { enabled });
}

/** Delete a channel, after its confirmation. */
export function removeChannel(id: string): Promise<void> {
  return deleteNotificationChannel(id);
}

/**
 * Send a test notification through a channel. A rejected call reads as a
 * failed test, so the row always has an outcome to show.
 */
export async function runChannelTest(id: string): Promise<ChannelTestResult> {
  try {
    return await testNotificationChannel(id);
  } catch (err: unknown) {
    return {
      channel_id: id,
      status: "error",
      error: err instanceof Error ? err.message : String(err),
      latency_ms: null,
    };
  }
}

/** Switch one notified event, saved at once. Returns the list now stored. */
export async function toggleNotifiedEvent(
  current: readonly string[],
  event: string,
): Promise<string[]> {
  const next = current.includes(event)
    ? current.filter((e) => e !== event)
    : [...current, event];
  await setNotificationEvents(next);
  return next;
}

/** The last sends, newest first. */
export function loadRecentSends(): Promise<NotificationLogEntry[]> {
  return getNotificationLogs(RECENT_SENDS_LIMIT);
}

/** The name a channel is shown under. */
export function channelName(channel: Pick<NotificationChannel, "channel_id" | "label">): string {
  return channel.label?.trim() ? channel.label : channel.channel_id;
}

/** What a channel row says under its name: where, which events, how often. */
export function channelDetailKeys(channel: NotificationChannel): {
  typeKey: string;
  eventsKey: string | null;
  eventCount: number;
  throttleKey: string;
  throttleSeconds: number;
} {
  const throttle = channel.min_interval_seconds ?? 0;
  const presets: Record<number, string> = {
    0: "settings.notifications.row.no_limit",
    60: "notifications.throttle_options.minute",
    300: "notifications.throttle_options.five_min",
    3600: "notifications.throttle_options.hour",
  };
  return {
    typeKey:
      channel.type === "webhook"
        ? "settings.notifications.type_webhook"
        : "settings.notifications.type_desktop",
    eventsKey: isAllEvents(channel.events) ? "notifications.all_events" : null,
    eventCount: isAllEvents(channel.events) ? 0 : channel.events.length,
    throttleKey: presets[throttle] ?? "settings.notifications.row.custom_limit",
    throttleSeconds: throttle,
  };
}

/** Whether a send reached every channel it was addressed to. */
export function sendSucceeded(entry: NotificationLogEntry): boolean {
  if (entry.error) return false;
  return Object.values(entry.channels).every((status) => status !== "error");
}
