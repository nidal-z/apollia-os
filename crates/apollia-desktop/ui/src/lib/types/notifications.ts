// ─── Notification channels and history ───

/** Configured notification channel. */
export interface NotificationChannel {
  channel_id: string;
  /** Free display name. `null` or absent falls back to `channel_id`. */
  label?: string | null;
  type: "desktop" | "webhook";
  enabled: boolean;
  events: string[];
  /** Minimum throttling interval, in seconds (`0` = none). */
  min_interval_seconds?: number;
}

/** Full channel definition returned by the CRUD operations. */
export interface NotificationChannelView {
  id: string;
  /** Free display name. `null` = no label. */
  label: string | null;
  channel_type: "desktop" | "webhook";
  enabled: boolean;
  config: Record<string, unknown>;
  events: string[] | null;
  /** Minimum throttling interval, in seconds (`0` = none). */
  min_interval_seconds: number;
  created_at: string;
  updated_at: string;
}

/** Request body creating a notification channel. */
export interface CreateChannelRequest {
  id: string;
  /** Display name (free, 80 chars max). Omit it when there is no label. */
  label?: string;
  channel_type: "desktop" | "webhook";
  enabled: boolean;
  config: Record<string, unknown>;
  events?: string[];
  /** Minimum throttling interval, in seconds. Omitted = `0` (none). */
  min_interval_seconds?: number;
}

/**
 * Request body updating a notification channel.
 *
 * Semantics of the `label` field:
 * - key absent -> keep the existing label;
 * - `label: null` -> clear the label;
 * - `label: "texte"` → remplacer.
 */
export interface UpdateChannelRequest {
  label?: string | null;
  channel_type?: "desktop" | "webhook";
  enabled?: boolean;
  config?: Record<string, unknown>;
  events?: string[];
  /** New throttling interval. Omitted = keep the existing one. */
  min_interval_seconds?: number;
}

/** Result of testing a notification channel. */
export interface ChannelTestResult {
  channel_id: string;
  status: "ok" | "error" | "disabled";
  error: string | null;
  latency_ms: number | null;
}

/** Entry of the notification history. */
export interface NotificationLogEntry {
  id: string;
  event_name: string;
  task_id: string | null;
  sent_at: string;
  channels: Record<string, string>;
  error: string | null;
}
