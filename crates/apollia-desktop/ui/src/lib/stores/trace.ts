/**
 * Store des traces d'exécution event-sourced.
 *
 * Deux sources de données fusionnées par task_id :
 * 1. `loadTrace(taskId)` - fetch initial paginé via la commande Tauri
 *    `get_task_trace` (replay depuis SQLite).
 * 2. `subscribeTraceLive(taskId)` - abonnement au channel Tauri
 *    `"trace-event"` (live SSE - Lot 4 connectera la voie SSE end-to-end ;
 *    Lot 3 supporte déjà la fusion live/replay côté store).
 *
 * Les événements sont indexés par `eventId` (UUIDv7 lex-ordonné), donc
 * insertion idempotente - si le live arrive avant le replay, le merge
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
  /** Dernière erreur de fetch - affichée dans le footer du composant. */
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

/** Tableau d'unsubscribe handles indexé par taskId - pour cleanup. */
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
 * Idempotent : un eventId déjà présent n'est pas dupliqué.
 *
 * Ordre final : tri par `ts` (clé primaire), `eventId` en tiebreaker.
 *
 * Pourquoi pas `eventId` seul ? Certains variants comme `tool_call_started`
 * pré-génèrent leur UUIDv7 *côté producteur* (`ToolProxy::call`) pour servir
 * de `parent_event_id` au companion `tool_call_completed`. D'autres comme
 * `thought` ne reçoivent leur UUIDv7 qu'au moment où le persistor les
 * consomme du bus. Conséquence : un `tool_call_started` émis APRÈS un
 * `thought` peut hériter d'un eventId lex-antérieur - l'ordre causal est
 * inversé. En revanche `ts` est posé par le persistor pour TOUS les
 * variants, et le bus broadcast est FIFO single-consumer ; donc `ts`
 * respecte l'ordre causal. EventId reste tiebreaker pour les égalités à
 * la milliseconde.
 */
function _mergeEvents(taskId: string, incoming: RuntimeEventDto[]): void {
  if (incoming.length === 0) return;
  _traceMap.update((m) => {
    const prev = m.get(taskId) ?? EMPTY_STATE;
    const seen = new Set(prev.events.map((e) => e.eventId));
    const fresh = incoming.filter((e) => !seen.has(e.eventId));
    if (fresh.length === 0) return m;
    const merged = [...prev.events, ...fresh].sort((a, b) => {
      if (a.ts !== b.ts) return a.ts < b.ts ? -1 : 1;
      if (a.eventId < b.eventId) return -1;
      if (a.eventId > b.eventId) return 1;
      return 0;
    });
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
  // Bound conservateur - empêche une boucle infinie si le serveur renvoie
  // toujours nextCursor (ne devrait pas arriver, mais on ne fait jamais
  // confiance aveuglément à un loop).
  for (let i = 0; i < 50; i++) {
    const _state = await new Promise<TraceState>((resolve) => {
      const unsub = traceFor(taskId).subscribe((s) => {
        resolve(s);
        // unsubscribe immédiat - on capture juste le snapshot.
        Promise.resolve().then(() => unsub());
      });
    });
    cursor = _state.nextCursor;
    if (cursor === null) return;
    await loadTrace(taskId, { since: cursor });
  }
}

/**
 * Enveloppe Tauri du bridge `EventBus → "runtime-event"`.
 *
 * Voir `apollia-desktop/src/events.rs::TauriRuntimeEvent`. Le bridge
 * route TOUS les `RuntimeEvent` du bus vers ce seul canal Tauri ; le
 * `category` permet aux stores de dispatcher sans parser chaque variant.
 */
interface TauriRuntimeEvent {
  category: string;
  event_type: string;
  payload: Record<string, unknown>;
}

/**
 * Convertit une enveloppe `runtime-event` (variant `RuntimeEvent` Rust,
 * sérialisé externally-tagged en `{"VariantName": {...fields}}`) en un
 * `RuntimeEventDto` consommable par les composants UI.
 *
 * Génère un `eventId` synthétique côté front (UUIDv4 random) - distinct
 * des UUIDv7 produits par l'`EventPersistor` côté Rust. Au reload du
 * panneau, `loadTrace` recharge depuis la DB avec les vrais event_ids et
 * remplace l'état (`reset: true`) - pas de doublons à terme.
 *
 * Retourne `null` quand l'enveloppe ne porte pas un kind exposable côté
 * trace (ex : variants legacy d'un autre store).
 */
function envelopeToDto(env: TauriRuntimeEvent): RuntimeEventDto | null {
  if (env.category !== "trace-event") return null;

  // Le payload est externally-tagged : `{"AgentLog": {task_id: "T", ...}}`.
  const variantName = env.event_type;
  const variantPayload =
    (env.payload[variantName] as Record<string, unknown> | undefined) ?? null;
  if (variantPayload === null) return null;

  // Mapping VariantName Rust → kind canonique (snake_case) côté trace.
  const kindMap: Record<string, string> = {
    AgentLog: "agent_log",
    Thought: "thought",
    LlmCallStarted: "llm_call_started",
    LlmCallFailed: "llm_call_failed",
    ToolCallStarted: "tool_call_started",
    ToolCallCompleted: "tool_call_completed",
    ToolCallDenied: "tool_call_denied",
    A2AInvokeStarted: "a2a_invoke_started",
    A2AInvokeCompleted: "a2a_invoke_completed",
    Retry: "retry",
    ActionParseError: "action_parse_error",
  };
  const kind = kindMap[variantName] ?? variantName.toLowerCase();

  // Helper: only stringify primitive-ish values; objects/arrays fall back to "".
  const asString = (v: unknown): string =>
    typeof v === "string" ? v : "";

  // Extraction des champs communs depuis la variante. Tous sont optionnels
  // selon le variant - on prend ce qui est présent.
  const taskId = asString(variantPayload.task_id);
  const agentId =
    asString(variantPayload.agent_id) ||
    asString(variantPayload.caller_agent_id);
  const parentEventId =
    typeof variantPayload.parent_event_id === "string"
      ? variantPayload.parent_event_id
      : null;
  const correlationId =
    typeof variantPayload.correlation_id === "string"
      ? variantPayload.correlation_id
      : null;
  const stepNum =
    typeof variantPayload.step_num === "number"
      ? variantPayload.step_num
      : null;
  // Pour les started/completed qui portent leur event_id explicite, le
  // réutiliser pour permettre au pairing client de fonctionner avant
  // même que le DB ne soit interrogé.
  const eventId =
    typeof variantPayload.event_id === "string"
      ? variantPayload.event_id
      : `live-${crypto.randomUUID()}`;

  return {
    eventId,
    taskId,
    agentId,
    parentEventId,
    correlationId,
    stepNum,
    kind,
    payload: variantPayload,
    ts: new Date().toISOString(),
  } as RuntimeEventDto;
}

/**
 * S'abonne aux événements live d'une task via le bus Tauri `"runtime-event"`.
 *
 * Filtre côté front sur la `category === "trace-event"` ET sur le
 * `taskId` cible - le bus transporte tous les events de toutes les
 * tasks et toutes les catégories.
 *
 * Idempotent : appeler deux fois pour la même task n'ouvre qu'un seul
 * abonnement. Toujours appeler `unsubscribeTraceLive(taskId)` au démontage
 * du composant.
 */
export async function subscribeTraceLive(taskId: string): Promise<void> {
  if (_liveUnsubs.has(taskId)) return;
  const unlisten = await listen<TauriRuntimeEvent>("runtime-event", (event) => {
    const dto = envelopeToDto(event.payload);
    if (dto === null) return;
    if (dto.taskId !== taskId) return;
    _mergeEvents(taskId, [dto]);
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
