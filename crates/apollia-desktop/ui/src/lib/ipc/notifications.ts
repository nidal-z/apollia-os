// Typed IPC wrappers for the notifications surface (channels + global scope).
//
// Centralizing the Tauri `invoke` calls here keeps the `.svelte` components free
// of raw `invoke("...")` string commands: the command names and argument shapes
// live in one place, so a backend rename is a single-file edit and every call
// site stays strongly typed.

import { invoke } from "@tauri-apps/api/core";
import type {
  ChannelTestResult,
  CreateChannelRequest,
  NotificationChannel,
  NotificationChannelView,
  UpdateChannelRequest,
} from "$lib/types";

/** Lists every configured notification channel. */
export async function listNotificationChannels(): Promise<NotificationChannel[]> {
  return invoke<NotificationChannel[]>("list_notification_channels");
}

/** Reads the globally-enabled event types (the notification scope). */
export async function getNotificationEvents(): Promise<string[]> {
  return invoke<string[]>("get_notification_events");
}

/** Persists the globally-enabled event types. */
export async function setNotificationEvents(events: string[]): Promise<void> {
  await invoke("set_notification_events", { events });
}

/** Creates a channel and returns its full stored view. */
export async function createNotificationChannel(
  channel: CreateChannelRequest,
): Promise<NotificationChannelView> {
  return invoke<NotificationChannelView>("create_notification_channel", { channel });
}

/** Updates a channel by id and returns its refreshed view. */
export async function updateNotificationChannel(
  id: string,
  channel: UpdateChannelRequest,
): Promise<NotificationChannelView> {
  return invoke<NotificationChannelView>("update_notification_channel", { id, channel });
}

/** Deletes a channel by id. */
export async function deleteNotificationChannel(id: string): Promise<void> {
  await invoke("delete_notification_channel", { id });
}

/** Sends a test notification through a channel and reports the outcome. */
export async function testNotificationChannel(
  channelId: string,
): Promise<ChannelTestResult> {
  return invoke<ChannelTestResult>("test_notification_channel", { channelId });
}
