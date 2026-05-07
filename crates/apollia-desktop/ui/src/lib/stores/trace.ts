/**
 * Store des traces d'exécution event-sourced (ADR-088, Lot 3).
 *
 * Deux sources de données fusionnées par task_id :
 * 1. `loadTrace(taskId)` — fetch initial paginé via la commande Tauri
 *    `get_task_trace` (replay depuis SQLite).
 * 2. `subscribeTraceLive(taskId)` — abonnement au channel Tauri
 *    `"trace-event"` (live SSE — Lot 4 connectera la voie SSE end-to-end ;
 *    Lot 3 supporte déjà la fusion live/replay côté store).
 *
 * Les événements sont indexés par `eventId` (UUIDv7 lex-ordonné), donc
 * insertion idempotente — si le live arrive avant le replay, le merge
 * détecte les doublons par eventId et conserve l'ordre.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { writable, derived, type Readable } from "svelte/store";

import type { GetTraceParams, RuntimeEventDto, TraceResponse } from "$lib/trace";

/** Page size lors du fetch paginé. Cohérent avec le `MAX_LIMIT` côté Rust. */
const FETCH_PAGE_SIZE = 500;

/** État de chargement par task_id. */
export interface TraceState {
  events: RuntimeEventDto[];
  /** Cursor pour la prochaine page (null = fin atteinte). */
  nextCursor: string | null;
  /** `true` pendant un fetch initial / pagination. */
  loading: boolean;
  /** Dernière erreur de fetch — affichée dans le footer du composant. */
  error: string | null;
  /** `true` si l'abonnement SSE live est actif. */
  live: boolean;
}

const EMPTY_STATE: TraceState = {
  events: [],
  nextCursor: null,
  loading: false,
  error: null,
  live: false,
};

/** Map task_id → état complet. Source de vérité du store. */
const _traceMap = writable<Map<string, TraceState>>(new Map());

/** Dérivé : helper pour piocher l'état d'une task spécifique. */
export function traceFor(taskId: string): Readable<TraceState> {
  return derived(_traceMap, ($m) => $m.get(taskId) ?? EMPTY_STATE);
}

/** Tableau d'unsubscribe handles indexé par taskId — pour cleanup. */
const _liveUnsubs: Map<string, UnlistenFn> = new Map();

/** Mute updater minimal. */
function _patch(taskId: string, patch: Partial<TraceState>): void {
  _traceMap.update((m) => {
    const prev = m.get(taskId) ?? EMPTY_STATE;
    m.set(taskId, { ...prev, ...patch });
    return m;
  });
}

/**
 * Insère / fusionne une liste d'événements dans l'état d'une task.
 *
 * Idempotent : un eventId déjà présent n'est pas dupliqué. L'ordre final
 * est garanti par le tri sur `eventId` (UUIDv7 lex == ordre causal).
 */
function _mergeEvents(taskId: string, incoming: RuntimeEventDto[]): void {
  if (incoming.length === 0) return;
  _traceMap.update((m) => {
    const prev = m.get(taskId) ?? EMPTY_STATE;
    const seen = new Set(prev.events.map((e) => e.eventId));
    const fresh = incoming.filter((e) => !seen.has(e.eventId));
    if (fresh.length === 0) return m;
    const merged = [...prev.events, ...fresh].sort((a, b) =>
      a.eventId < b.eventId ? -1 : a.eventId > b.eventId ? 1 : 0,
    );
    m.set(taskId, { ...prev, events: merged });
    return m;
  });
}

/**
 * Fetch initial / pagination d'une trace depuis le runtime.
 *
 * Sans `since`, repart du début (clear préalable de l'état).
 * Avec `since`, ajoute la prochaine page sans toucher aux événements
 * déjà chargés.
 */
export async function loadTrace(
  taskId: string,
  opts: { since?: string | null; reset?: boolean } = {},
): Promise<void> {
  if (opts.reset) {
    _traceMap.update((m) => {
      m.set(taskId, EMPTY_STATE);
      return m;
    });
  }

  _patch(taskId, { loading: true, error: null });

  try {
    const params: GetTraceParams = {
      taskId,
      since: opts.since ?? null,
      limit: FETCH_PAGE_SIZE,
    };
    const resp = await invoke<TraceResponse>("get_task_trace", { params });
    _mergeEvents(taskId, resp.events);
    _patch(taskId, { nextCursor: resp.nextCursor, loading: false });
  } catch (e: unknown) {
    _patch(taskId, {
      loading: false,
      error: e instanceof Error ? e.message : String(e),
    });
  }
}

/**
 * Charge l'intégralité de la trace en suivant les curseurs jusqu'au bout.
 *
 * Borné implicitement par `MAX_LIMIT` côté Rust (5000 events / page) ;
 * pour des traces très longues, l'UI doit virtualiser le rendu (cf.
 * ExecutionTrace.svelte).
 */
export async function loadFullTrace(taskId: string): Promise<void> {
  await loadTrace(taskId, { reset: true });
  let cursor: string | null = null;
  // Bound conservateur — empêche une boucle infinie si le serveur renvoie
  // toujours nextCursor (ne devrait pas arriver, mais on ne fait jamais
  // confiance aveuglément à un loop).
  for (let i = 0; i < 50; i++) {
    const _state = await new Promise<TraceState>((resolve) => {
      const unsub = traceFor(taskId).subscribe((s) => {
        resolve(s);
        // unsubscribe immédiat — on capture juste le snapshot.
        Promise.resolve().then(() => unsub());
      });
    });
    cursor = _state.nextCursor;
    if (cursor === null) return;
    await loadTrace(taskId, { since: cursor });
  }
}

/**
 * S'abonne aux événements live d'une task via le bus Tauri `"trace-event"`.
 *
 * Filtre côté front sur le `taskId` cible — le bus transporte les
 * événements de toutes les tasks.
 *
 * Idempotent : appeler deux fois pour la même task n'ouvre qu'un seul
 * abonnement. Toujours appeler `unsubscribeTraceLive(taskId)` au démontage
 * du composant.
 */
export async function subscribeTraceLive(taskId: string): Promise<void> {
  if (_liveUnsubs.has(taskId)) return;
  const unlisten = await listen<RuntimeEventDto>("trace-event", (event) => {
    const payload = event.payload;
    if (payload.taskId !== taskId) return;
    _mergeEvents(taskId, [payload]);
  });
  _liveUnsubs.set(taskId, unlisten);
  _patch(taskId, { live: true });
}

/** Coupe l'abonnement live pour une task. À appeler dans `onDestroy`. */
export function unsubscribeTraceLive(taskId: string): void {
  const fn = _liveUnsubs.get(taskId);
  if (fn) {
    fn();
    _liveUnsubs.delete(taskId);
    _patch(taskId, { live: false });
  }
}

/** Vide entièrement la trace d'une task (ex: navigation, cleanup mémoire). */
export function clearTrace(taskId: string): void {
  unsubscribeTraceLive(taskId);
  _traceMap.update((m) => {
    m.delete(taskId);
    return m;
  });
}
