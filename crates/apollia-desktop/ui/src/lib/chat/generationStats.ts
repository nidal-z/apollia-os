/**
 * The generation figures of a turn, as the chat shows them.
 *
 * The answer of a turn carries its total under the `generation` key of its
 * metadata, so a reloaded conversation reads it back from the message itself.
 * The detail, one row per model call, is read on demand. A value the engine
 * did not measure is absent, and is shown as "not measured", never as zero.
 *
 * This module decides what is shown and under which catalogue key; the
 * components only lay it out.
 */
import { formatSecondsLocalized } from "$lib/chat/duration";

/** Why a turn ended, as the runtime writes it under the answer. */
export type TurnStopReason = "completed" | "length" | "user_stop" | "error";

/** The total of a turn. */
export interface TurnGeneration {
  /** Number of model calls the turn made. */
  calls: number;
  /** Tokens generated across the calls. */
  output_tokens: number;
  /** Generated tokens per second of generation time; absent when unknown. */
  tokens_per_second?: number | null;
  /** Time to first token of the first call; absent when not measured. */
  ttft_ms?: number | null;
  /** From the user's message to the end of the turn. */
  duration_ms?: number | null;
  stop_reason: TurnStopReason;
}

/** One model call of a turn, as the runtime lists it. */
export interface TurnLlmCall {
  backend: string;
  model: string;
  prompt_tokens: number | null;
  completion_tokens: number | null;
  ttft_ms: number | null;
  latency_ms: number | null;
  generation_ms: number | null;
  /** `stop`, `tool_calls`, `length`, `error` or `cancelled`. */
  finish_reason: string | null;
}

/** The five figures of the line under an answer, in the order shown. */
export type GenerationInfoId = "speed" | "tokens" | "ttft" | "duration" | "stop";

/** One figure of the line: its text and the sentence that explains it. */
export interface GenerationInfoItem {
  id: GenerationInfoId;
  /** Catalogue key of the text. */
  labelKey: string;
  /** Values of the text: figures formatted for the locale, or a count. */
  values: Record<string, string | number>;
  /** Catalogue key of the one-sentence explanation shown on hover. */
  helpKey: string;
  /** False when the figure reads "not measured". */
  measured: boolean;
}

const STOP_KEYS: Record<TurnStopReason, string> = {
  completed: "chat.generation.stop.completed",
  length: "chat.generation.stop.length",
  user_stop: "chat.generation.stop.user_stop",
  error: "chat.generation.stop.error",
};

const HELP_KEYS: Record<GenerationInfoId, string> = {
  speed: "chat.generation.help.speed",
  tokens: "chat.generation.help.tokens",
  ttft: "chat.generation.help.ttft",
  duration: "chat.generation.help.duration",
  stop: "chat.generation.help.stop",
};

type FigureId = Exclude<GenerationInfoId, "stop">;

const FIGURE_KEYS: Record<FigureId, string> = {
  speed: "chat.generation.speed",
  tokens: "chat.generation.tokens",
  ttft: "chat.generation.ttft",
  duration: "chat.generation.duration",
};

const UNMEASURED_FIGURE_KEYS: Record<Exclude<FigureId, "tokens">, string> = {
  speed: "chat.generation.speed_unmeasured",
  ttft: "chat.generation.ttft_unmeasured",
  duration: "chat.generation.duration_unmeasured",
};

const CALL_END_KEYS: Record<string, string> = {
  stop: "chat.generation.call_end.stop",
  tool_calls: "chat.generation.call_end.tool_calls",
  length: "chat.generation.call_end.length",
  error: "chat.generation.call_end.error",
  cancelled: "chat.generation.call_end.cancelled",
};

const CALL_FIELD_KEYS: Record<CallFieldId, string> = {
  model: "chat.generation.call.model",
  input_tokens: "chat.generation.call.input_tokens",
  output_tokens: "chat.generation.call.output_tokens",
  ttft: "chat.generation.call.ttft",
  duration: "chat.generation.call.duration",
  end: "chat.generation.call.end",
};

function isStopReason(value: unknown): value is TurnStopReason {
  return typeof value === "string" && Object.hasOwn(STOP_KEYS, value);
}

function knownNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : null;
}

/**
 * The total of a turn read from the metadata of its answer; `null` when the
 * message carries none (a user message, an answer written before the totals
 * existed, a turn that made no call).
 */
export function readGeneration(
  metadata: Record<string, unknown> | null | undefined,
): TurnGeneration | null {
  const raw = metadata?.generation;
  if (raw === null || typeof raw !== "object") return null;
  const g = raw as Record<string, unknown>;
  const outputTokens = knownNumber(g.output_tokens);
  const stop = g.stop_reason;
  if (outputTokens === null) return null;
  if (!isStopReason(stop)) return null;
  return {
    calls: knownNumber(g.calls) ?? 0,
    output_tokens: outputTokens,
    tokens_per_second: knownNumber(g.tokens_per_second),
    ttft_ms: knownNumber(g.ttft_ms),
    duration_ms: knownNumber(g.duration_ms),
    stop_reason: stop,
  };
}

function formatNumber(value: number, loc: string, fractionDigits: number): string {
  try {
    return new Intl.NumberFormat(loc, {
      minimumFractionDigits: fractionDigits,
      maximumFractionDigits: fractionDigits,
    }).format(value);
  } catch {
    return value.toFixed(fractionDigits);
  }
}

/** Seconds with two fraction digits: a first token is often under a second. */
/**
 * The answer the "last message only" setting shows figures under: the last
 * message of the thread that carries figures, so they stay in place while the
 * next question is typed and sent.
 */
export function lastAnswerId(
  messages: readonly {
    id: string;
    role: string;
    metadata?: Record<string, unknown> | null;
  }[],
): string | null {
  for (let i = messages.length - 1; i >= 0; i -= 1) {
    const m = messages[i];
    if (m.role !== "user" && readGeneration(m.metadata) !== null) return m.id;
  }
  return null;
}

/** Whether an answer shows its generation figures under the Appearance setting. */
export function showsGenerationInfo(input: {
  scope: "last" | "all";
  messageId: string;
  lastAnswerId: string | null;
}): boolean {
  return input.scope === "all" || input.messageId === input.lastAnswerId;
}

export function formatFirstTokenSeconds(ms: number, loc: string): string {
  return formatNumber(ms / 1000, loc, 2);
}

/** A count of tokens, grouped for the locale. */
export function formatTokenCount(count: number, loc: string): string {
  return formatNumber(count, loc, 0);
}

/**
 * The line under an answer: speed, tokens generated, time to first token,
 * duration and why the turn ended. Always five items; an unknown figure keeps
 * its place and reads "not measured".
 */
export function generationInfoItems(gen: TurnGeneration, loc: string): GenerationInfoItem[] {
  const speed = knownNumber(gen.tokens_per_second);
  const ttft = knownNumber(gen.ttft_ms);
  const duration = knownNumber(gen.duration_ms);
  return [
    speed === null
      ? unmeasured("speed")
      : measured("speed", { value: formatNumber(speed, loc, 1) }),
    measured("tokens", { count: gen.output_tokens }),
    ttft === null
      ? unmeasured("ttft")
      : measured("ttft", { value: formatFirstTokenSeconds(ttft, loc) }),
    duration === null
      ? unmeasured("duration")
      : measured("duration", { value: formatSecondsLocalized(duration, loc) }),
    {
      id: "stop",
      labelKey: STOP_KEYS[gen.stop_reason],
      values: {},
      helpKey: HELP_KEYS.stop,
      measured: true,
    },
  ];
}

function measured(id: FigureId, values: Record<string, string | number>): GenerationInfoItem {
  return { id, labelKey: FIGURE_KEYS[id], values, helpKey: HELP_KEYS[id], measured: true };
}

function unmeasured(id: Exclude<FigureId, "tokens">): GenerationInfoItem {
  return {
    id,
    labelKey: UNMEASURED_FIGURE_KEYS[id],
    values: {},
    helpKey: HELP_KEYS[id],
    measured: false,
  };
}

/** Catalogue key of why one call ended; an unknown reason reads "not measured". */
export function callEndKey(finishReason: string | null | undefined): string {
  if (finishReason != null && Object.hasOwn(CALL_END_KEYS, finishReason)) {
    return CALL_END_KEYS[finishReason];
  }
  return UNMEASURED_KEY;
}

/** The fields of one call, in the order its card lists them. */
export type CallFieldId = "model" | "input_tokens" | "output_tokens" | "ttft" | "duration" | "end";

/** One field of a call: either a ready text, or a catalogue key to translate. */
export interface CallField {
  id: CallFieldId;
  /** Catalogue key of the field name. */
  nameKey: string;
  /** Text shown as is, when the value is a figure or a name. */
  text?: string;
  /** Catalogue key of the value, when it is a phrase. */
  valueKey?: string;
  /** Values of `valueKey`. */
  values?: Record<string, string>;
}

const UNMEASURED_KEY = "chat.generation.unmeasured";
const SECONDS_KEY = "chat.generation.seconds";

/**
 * The card of one model call: model, tokens read and generated, time to first
 * token, duration, why it ended. An unknown figure reads "not measured".
 */
export function callFields(call: TurnLlmCall, loc: string): CallField[] {
  const input = knownNumber(call.prompt_tokens);
  const output = knownNumber(call.completion_tokens);
  const ttft = knownNumber(call.ttft_ms);
  const latency = knownNumber(call.latency_ms);
  const name = (id: CallFieldId): string => CALL_FIELD_KEYS[id];
  return [
    { id: "model", nameKey: name("model"), text: call.model },
    input === null
      ? { id: "input_tokens", nameKey: name("input_tokens"), valueKey: UNMEASURED_KEY }
      : { id: "input_tokens", nameKey: name("input_tokens"), text: formatTokenCount(input, loc) },
    output === null
      ? { id: "output_tokens", nameKey: name("output_tokens"), valueKey: UNMEASURED_KEY }
      : { id: "output_tokens", nameKey: name("output_tokens"), text: formatTokenCount(output, loc) },
    ttft === null
      ? { id: "ttft", nameKey: name("ttft"), valueKey: UNMEASURED_KEY }
      : {
          id: "ttft",
          nameKey: name("ttft"),
          valueKey: SECONDS_KEY,
          values: { value: formatFirstTokenSeconds(ttft, loc) },
        },
    latency === null
      ? { id: "duration", nameKey: name("duration"), valueKey: UNMEASURED_KEY }
      : {
          id: "duration",
          nameKey: name("duration"),
          valueKey: SECONDS_KEY,
          values: { value: formatSecondsLocalized(latency, loc) },
        },
    { id: "end", nameKey: name("end"), valueKey: callEndKey(call.finish_reason) },
  ];
}

/** A translator shaped like the `$t` of the catalogue. */
export type Translate = (
  key: string,
  options?: { values?: Record<string, string | number> },
) => string;

/** The value of a field as text. */
export function callFieldText(field: CallField, translate: Translate): string {
  if (field.text !== undefined) return field.text;
  return translate(field.valueKey ?? UNMEASURED_KEY, { values: field.values ?? {} });
}

/** A model call as the steps panel lists it. */
export interface LlmJournalEntry {
  type: "llm";
  heading: string;
  body: string;
  time: string;
}

/**
 * The model calls of a turn as events of the steps panel, one per call, in
 * order. The default wording names each call and says what it read, wrote and
 * why it ended; the developer wording lists the raw fields of the call.
 */
export function llmJournalEvents(
  calls: readonly TurnLlmCall[],
  options: { developer: boolean; loc: string; time: string; translate: Translate },
): LlmJournalEntry[] {
  const { developer, loc, time, translate } = options;
  return calls.map((call, index) => {
    if (developer) {
      const raw = (value: number | string | null): string => (value === null ? "null" : String(value));
      return {
        type: "llm",
        heading: call.model,
        body: [
          `prompt_tokens=${raw(call.prompt_tokens)}`,
          `completion_tokens=${raw(call.completion_tokens)}`,
          `ttft_ms=${raw(call.ttft_ms)}`,
          `latency_ms=${raw(call.latency_ms)}`,
          `finish_reason=${raw(call.finish_reason)}`,
        ].join(" · "),
        time,
      };
    }
    const body = callFields(call, loc)
      .map(
        (field) =>
          `${translate(field.nameKey)} ${callFieldText(field, translate)}`,
      )
      .join(" · ");
    return {
      type: "llm",
      heading: translate("chat.generation.call_title", { values: { n: String(index + 1) } }),
      body,
      time,
    };
  });
}
