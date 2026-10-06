/**
 * The channel dialog as data: the form a user fills to create or edit a
 * notification channel, how it is checked, and the request it becomes.
 *
 * Kept out of the dialog component so the rules can be tested under the node
 * Vitest environment, where no component mounts.
 */
import type {
  CreateChannelRequest,
  NotificationChannel,
  NotificationChannelView,
  UpdateChannelRequest,
} from "$lib/types";
import { slugify } from "$lib/utils/slugify";
import { MAX_MIN_INTERVAL_SECONDS, isPreset } from "$lib/notifications/throttle-options";

export type ChannelType = "desktop" | "webhook";

/**
 * The value a channel stores to follow the notified events of the
 * application rather than its own list.
 */
export const ALL_EVENTS = "*";

/** Backend `channel_id` pattern. */
export const CHANNEL_ID_PATTERN = /^[a-z0-9][a-z0-9-]*[a-z0-9]$|^[a-z0-9]$/;
export const LABEL_MAX_LEN = 80;

export interface ChannelForm {
  label: string;
  /** Technical identifier, derived from the label until edited by hand. */
  id: string;
  type: ChannelType;
  enabled: boolean;
  url: string;
  headers: string;
  /** `true`: the channel receives every notified event. */
  allEvents: boolean;
  /** The events kept when `allEvents` is off. */
  events: string[];
  /** Preset value in seconds as a string, or `"custom"`. */
  throttlePreset: string;
  throttleCustom: string;
}

/** `true` when a stored event list means "every notified event". */
export function isAllEvents(events: readonly string[] | null | undefined): boolean {
  return !events || events.length === 0 || events.includes(ALL_EVENTS);
}

/** The form of a new channel: this computer, every event, no limit. */
export function emptyForm(): ChannelForm {
  return {
    label: "",
    id: "",
    type: "desktop",
    enabled: true,
    url: "",
    headers: "",
    allEvents: true,
    events: [],
    throttlePreset: "0",
    throttleCustom: "0",
  };
}

/**
 * The form of an existing channel. The channel list carries no configuration,
 * so the address and headers of a web call are only known from a full view.
 */
export function formFromChannel(view: NotificationChannelView): ChannelForm {
  const url = typeof view.config.url === "string" ? view.config.url : "";
  const headers = view.config.headers ? JSON.stringify(view.config.headers, null, 2) : "";
  const throttle = view.min_interval_seconds ?? 0;
  return {
    label: view.label ?? "",
    id: view.id,
    type: view.channel_type,
    enabled: view.enabled,
    url,
    headers,
    allEvents: isAllEvents(view.events),
    events: isAllEvents(view.events) ? [] : [...(view.events ?? [])],
    throttlePreset: isPreset(throttle) ? String(throttle) : "custom",
    throttleCustom: isPreset(throttle) ? "0" : String(throttle),
  };
}

/** The full view of a listed channel, as far as the list knows it. */
export function viewFromListed(channel: NotificationChannel): NotificationChannelView {
  return {
    id: channel.channel_id,
    label: channel.label ?? null,
    channel_type: channel.type,
    enabled: channel.enabled,
    config: {},
    events: channel.events.length > 0 ? channel.events : null,
    min_interval_seconds: channel.min_interval_seconds ?? 0,
    created_at: "",
    updated_at: "",
  };
}

/** The label typed, with the identifier following it until edited by hand. */
export function withLabel(form: ChannelForm, label: string, idEdited: boolean): ChannelForm {
  return { ...form, label, id: idEdited ? form.id : slugify(label) };
}

/** Check or uncheck "every event". Checking it again forgets the partial list. */
export function setAllEvents(form: ChannelForm, all: boolean): ChannelForm {
  return { ...form, allEvents: all, events: all ? [] : [...form.events] };
}

/**
 * Check or uncheck one event. Unchecking one while "every event" is on keeps
 * all the others: the list starts from the notified events.
 */
export function toggleEvent(
  form: ChannelForm,
  event: string,
  notified: readonly string[],
): ChannelForm {
  const current = form.allEvents ? [...notified] : form.events;
  const next = current.includes(event)
    ? current.filter((e) => e !== event)
    : [...current, event];
  return { ...form, allEvents: false, events: next };
}

/** Whether one event box reads as checked. */
export function eventChecked(form: ChannelForm, event: string, notified: readonly string[]): boolean {
  return form.allEvents ? notified.includes(event) : form.events.includes(event);
}

/** The limit in seconds the form asks for. */
export function throttleSeconds(form: ChannelForm): number {
  if (form.throttlePreset === "custom") {
    return Math.max(0, Math.floor(Number.parseFloat(form.throttleCustom) || 0));
  }
  return Number.parseInt(form.throttlePreset, 10) || 0;
}

export type ChannelFormField = "label" | "id" | "url" | "headers" | "throttle";

/** The i18n key of each field in error, empty when the form can be sent. */
export function formErrors(
  form: ChannelForm,
  mode: "create" | "edit",
): Partial<Record<ChannelFormField, string>> {
  const errors: Partial<Record<ChannelFormField, string>> = {};
  if (form.label.length > LABEL_MAX_LEN) errors.label = "notifications.field_label_too_long";
  if (mode === "create") {
    if (!form.id.trim()) errors.id = "notifications.field_id_required";
    else if (!CHANNEL_ID_PATTERN.test(form.id)) errors.id = "notifications.field_id_invalid";
  }
  if (form.type === "webhook") {
    if (!form.url.trim()) errors.url = "notifications.field_url_required";
    if (form.headers.trim()) {
      try {
        JSON.parse(form.headers);
      } catch {
        errors.headers = "notifications.field_headers_invalid";
      }
    }
  }
  if (throttleSeconds(form) > MAX_MIN_INTERVAL_SECONDS) {
    errors.throttle = "notifications.field_throttle_too_large";
  }
  return errors;
}

function configOf(form: ChannelForm): Record<string, unknown> {
  if (form.type !== "webhook") return {};
  const config: Record<string, unknown> = { url: form.url.trim() };
  if (form.headers.trim()) config.headers = JSON.parse(form.headers) as unknown;
  return config;
}

/** The request creating the channel. "Every event" sends no list. */
export function createRequest(form: ChannelForm): CreateChannelRequest {
  const request: CreateChannelRequest = {
    id: form.id.trim(),
    channel_type: form.type,
    enabled: form.enabled,
    config: configOf(form),
  };
  const label = form.label.trim();
  if (label) request.label = label;
  if (!form.allEvents) request.events = [...form.events];
  const throttle = throttleSeconds(form);
  if (throttle > 0) request.min_interval_seconds = throttle;
  return request;
}

/**
 * The request updating a channel from its stored view. Only what changed among
 * the label, the events and the limit is sent; "every event" is sent as the
 * wildcard, since an absent list keeps the stored one.
 */
export function updateRequest(
  original: NotificationChannelView,
  form: ChannelForm,
): UpdateChannelRequest {
  const request: UpdateChannelRequest = {
    channel_type: form.type,
    enabled: form.enabled,
    config: configOf(form),
  };
  const label = form.label.trim();
  if (label !== (original.label ?? "").trim()) request.label = label || null;
  const wasAll = isAllEvents(original.events);
  if (form.allEvents && !wasAll) request.events = [ALL_EVENTS];
  if (!form.allEvents) request.events = [...form.events];
  const throttle = throttleSeconds(form);
  if (throttle !== (original.min_interval_seconds ?? 0)) request.min_interval_seconds = throttle;
  return request;
}
