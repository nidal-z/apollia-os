/**
 * The system notifications of the batches: the end of a batch, with how many
 * items succeeded and failed, and an item that waits for a decision.
 *
 * The runtime event store hands every event of the batch engine here; what a
 * notification says is decided by two pure functions, and sent through the
 * notification plugin the approvals already use.
 */
import { get } from "svelte/store";
import { t } from "svelte-i18n";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";

/** A notification to send: the catalogue keys of its two lines and their values. */
export interface BatchNotice {
  titleKey: string;
  bodyKey: string;
  values: Record<string, string | number>;
}

/** A runtime event as the bridge emits it. */
export interface BatchRuntimeEvent {
  event_type: string;
  payload: Record<string, unknown>;
}

function inner(event: BatchRuntimeEvent): Record<string, unknown> {
  const tagged = event.payload[event.event_type];
  return tagged !== null && typeof tagged === "object" ? (tagged as Record<string, unknown>) : event.payload;
}

function count(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function name(value: unknown): string {
  return typeof value === "string" ? value : "";
}

/** How a batch ended, as its notification titles it. */
const ENDED_TITLE: Record<string, string> = {
  completed: "notifications.native.batch_completed_title",
  interrupted: "notifications.native.batch_interrupted_title",
  stopped: "notifications.native.batch_stopped_title",
};

/**
 * What the notification of an event of the batch engine says, `null` for any
 * other event. The end of a batch carries its successes and its failures as
 * the engine counted them.
 */
export function batchNotice(event: BatchRuntimeEvent): BatchNotice | null {
  const p = inner(event);
  if (event.event_type === "BatchEnded") {
    const waiting = count(p.awaiting);
    return {
      titleKey: ENDED_TITLE[name(p.state)] ?? ENDED_TITLE.completed,
      bodyKey: waiting > 0 ? "notifications.native.batch_ended_body_waiting" : "notifications.native.batch_ended_body",
      values: {
        agent: name(p.agent_id),
        completed: count(p.completed),
        failed: count(p.failed),
        awaiting: waiting,
      },
    };
  }
  if (event.event_type === "BatchItemAwaitingApproval") {
    return {
      titleKey: "notifications.native.batch_awaiting_title",
      bodyKey: "notifications.native.batch_awaiting_body",
      values: { agent: name(p.agent_id), number: count(p.rank) + 1, access: name(p.access) },
    };
  }
  return null;
}

/** Sends the system notification of an event of the batch engine, if it has one. */
export async function notifyBatchEvent(event: BatchRuntimeEvent): Promise<void> {
  const notice = batchNotice(event);
  if (notice === null) return;
  try {
    let granted = await isPermissionGranted();
    if (!granted) granted = (await requestPermission()) === "granted";
    if (!granted) return;
    sendNotification({
      title: get(t)(notice.titleKey),
      body: get(t)(notice.bodyKey, { values: notice.values }),
    });
  } catch {
    // Notification API unavailable: the view of the batch still shows its end.
  }
}
