/**
 * Session metrics store.
 *
 * Agrège les événements `SessionMetricsUpdated` reçus via le pont Tauri
 * dans un index `session_id -> { metrics, alert }`. Les composants
 * `SessionMetricsPanel`, `ContextWindowBar`, `ToolTimingList` et
 * `SummarizedMessagesBanner` s'abonnent à ce store.
 *
 * Toujours-on : le store est alimenté dès qu'au moins un événement arrive,
 * sans configuration préalable côté UI.
 */
import { listen, type Event } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { derived, writable, type Readable } from "svelte/store";
import type {
  BudgetAlertLevel,
  SessionMetrics,
  SessionMetricsUpdatedEvent,
} from "$lib/types";

interface RuntimeEventEnvelope<T> {
  category: string;
  event_type: string;
  payload: T;
}

export interface SessionMetricsSlot {
  metrics: SessionMetrics;
  alert: BudgetAlertLevel;
}

const EMPTY_METRICS: SessionMetrics = {
  tokens_in: 0,
  tokens_out: 0,
  tokens_cached: 0,
  tokens_meta: 0,
  context_window_used: 0,
  context_window_max: 0,
  token_budget: 0,
  tool_timings: [],
  summarization_events: [],
};

const EMPTY_SLOT: SessionMetricsSlot = { metrics: EMPTY_METRICS, alert: "ok" };

const slots = writable<Record<string, SessionMetricsSlot>>({});

/** Read-only map `session_id -> SessionMetricsSlot`. */
export const sessionMetricsSlots: Readable<Record<string, SessionMetricsSlot>> = {
  subscribe: slots.subscribe,
};

/** Store dérivé pour une session donnée — retourne un slot vide si inconnu. */
export function sessionMetricsFor(
  sessionId: string,
): Readable<SessionMetricsSlot> {
  return derived(slots, ($s) => $s[sessionId] ?? EMPTY_SLOT);
}

/** Handler idempotent — exporté pour les tests et pour l'initialisation. */
export function handleSessionMetricsUpdated(
  payload: SessionMetricsUpdatedEvent,
): void {
  slots.update((current) => ({
    ...current,
    [payload.session_id]: { metrics: payload.metrics, alert: payload.alert },
  }));
}

/** Charge le snapshot initial depuis la commande Tauri (fallback hors event). */
export async function hydrateSessionMetrics(sessionId: string): Promise<void> {
  try {
    const metrics = await invoke<SessionMetrics | null>("get_session_metrics", {
      sessionId,
    });
    if (metrics) {
      handleSessionMetricsUpdated({ session_id: sessionId, metrics, alert: "ok" });
    }
  } catch {
    // Commande absente en dev / pas encore de snapshot — ignorer silencieusement.
  }
}

/**
 * Initialise le listener Tauri. Idempotent : appels multiples n'installent
 * qu'un seul listener.
 */
let unlisten: (() => void) | null = null;
let initialized = false;

export function initSessionMetricsListener(): void {
  if (initialized) return;
  initialized = true;
  void listen<RuntimeEventEnvelope<SessionMetricsUpdatedEvent>>(
    "runtime-event",
    (event: Event<RuntimeEventEnvelope<SessionMetricsUpdatedEvent>>) => {
      if (event.payload.event_type !== "SessionMetricsUpdated") return;
      handleSessionMetricsUpdated(event.payload.payload);
    },
  ).then((fn) => {
    unlisten = fn;
  });
}

/** Arrête le listener — utilisé dans les tests. */
export function disposeSessionMetricsListener(): void {
  if (unlisten) {
    unlisten();
    unlisten = null;
  }
  initialized = false;
  slots.set({});
}
