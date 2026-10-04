import { describe, test, expect } from "vitest";
import {
  callEndKey,
  callFields,
  callFieldText,
  generationInfoItems,
  llmJournalEvents,
  readGeneration,
  type Translate,
  type TurnLlmCall,
} from "./generationStats";

const echo: Translate = (key, options) => {
  const values = options?.values ?? {};
  const shown = Object.keys(values)
    .map((k) => `${k}=${values[k]}`)
    .join(",");
  return shown ? `${key}(${shown})` : key;
};

const CALL: TurnLlmCall = {
  backend: "local",
  model: "qwen3-8b",
  prompt_tokens: 3120,
  completion_tokens: 212,
  ttft_ms: 480,
  latency_ms: 5200,
  generation_ms: 4700,
  finish_reason: "tool_calls",
};

describe("the total of a turn read from its answer", () => {
  test("a stored answer gives back its total after a reload", () => {
    // GIVEN the metadata of an answer as the conversation store returns it,
    // serialised and read back as a reload does
    const stored = JSON.stringify({
      thinking_trace: "…",
      generation: {
        calls: 2,
        output_tokens: 412,
        tokens_per_second: 38.2,
        ttft_ms: 480,
        duration_ms: 11600,
        stop_reason: "completed",
      },
    });
    const metadata = JSON.parse(stored) as Record<string, unknown>;

    // WHEN the total is read
    const generation = readGeneration(metadata);

    // THEN every figure is there
    expect(generation).toEqual({
      calls: 2,
      output_tokens: 412,
      tokens_per_second: 38.2,
      ttft_ms: 480,
      duration_ms: 11600,
      stop_reason: "completed",
    });
  });

  test("an absent figure stays absent, never zero", () => {
    // GIVEN a total whose engine reported neither a first token nor a speed
    const metadata = {
      generation: { calls: 1, output_tokens: 90, duration_ms: 2000, stop_reason: "length" },
    };

    // WHEN the total is read
    const generation = readGeneration(metadata);

    // THEN the unknown figures are null
    expect(generation?.ttft_ms).toBeNull();
    expect(generation?.tokens_per_second).toBeNull();
    expect(generation?.duration_ms).toBe(2000);
  });

  test("a message without a total, or with a malformed one, has no line", () => {
    // GIVEN messages that carry no usable total
    // WHEN each is read
    // THEN none gives a total
    expect(readGeneration(null)).toBeNull();
    expect(readGeneration({})).toBeNull();
    expect(readGeneration({ generation: "fast" })).toBeNull();
    expect(readGeneration({ generation: { output_tokens: 3 } })).toBeNull();
    expect(readGeneration({ generation: { output_tokens: 3, stop_reason: "tired" } })).toBeNull();
  });
});

describe("the line under an answer", () => {
  test("it shows the five figures in order, formatted for the locale", () => {
    // GIVEN a complete total
    const generation = readGeneration({
      generation: {
        calls: 2,
        output_tokens: 412,
        tokens_per_second: 38.2,
        ttft_ms: 480,
        duration_ms: 11600,
        stop_reason: "completed",
      },
    });

    // WHEN the line is built in French
    const items = generationInfoItems(generation!, "fr");

    // THEN speed, tokens, first token, duration and stop reason follow each other
    expect(items.map((i) => i.id)).toEqual(["speed", "tokens", "ttft", "duration", "stop"]);
    expect(items[0]).toMatchObject({ labelKey: "chat.generation.speed", values: { value: "38,2" } });
    expect(items[1]).toMatchObject({ labelKey: "chat.generation.tokens", values: { count: 412 } });
    expect(items[2]).toMatchObject({ labelKey: "chat.generation.ttft", values: { value: "0,48" } });
    expect(items[3]).toMatchObject({ labelKey: "chat.generation.duration", values: { value: "11,6" } });
    expect(items[4].labelKey).toBe("chat.generation.stop.completed");
    expect(items.every((i) => i.measured)).toBe(true);
  });

  test("an unknown figure keeps its place and reads not measured", () => {
    // GIVEN a total with no first token, no speed and no duration
    const generation = readGeneration({
      generation: { calls: 1, output_tokens: 0, stop_reason: "user_stop" },
    });

    // WHEN the line is built
    const items = generationInfoItems(generation!, "en");

    // THEN the three unknown figures say so, and zero tokens stays a figure
    expect(items.map((i) => i.labelKey)).toEqual([
      "chat.generation.speed_unmeasured",
      "chat.generation.tokens",
      "chat.generation.ttft_unmeasured",
      "chat.generation.duration_unmeasured",
      "chat.generation.stop.user_stop",
    ]);
    expect(items.map((i) => i.measured)).toEqual([false, true, false, false, true]);
    expect(items[1].values).toEqual({ count: 0 });
  });

  test("every figure has its own explanation", () => {
    // GIVEN a line, complete or not
    const full = generationInfoItems(
      readGeneration({
        generation: { calls: 1, output_tokens: 5, ttft_ms: 10, stop_reason: "error" },
      })!,
      "en",
    );

    // WHEN the explanations are listed
    const helps = full.map((i) => i.helpKey);

    // THEN there are five, one per figure, measured or not
    expect(helps).toEqual([
      "chat.generation.help.speed",
      "chat.generation.help.tokens",
      "chat.generation.help.ttft",
      "chat.generation.help.duration",
      "chat.generation.help.stop",
    ]);
  });
});

describe("the detail of one model call", () => {
  test("it lists model, tokens in and out, first token, duration and end", () => {
    // GIVEN a call that asked for a tool
    // WHEN its card is built in French
    const fields = callFields(CALL, "fr");

    // THEN the six fields are there with their values
    expect(fields.map((f) => f.id)).toEqual([
      "model",
      "input_tokens",
      "output_tokens",
      "ttft",
      "duration",
      "end",
    ]);
    expect(fields.map((f) => callFieldText(f, echo))).toEqual([
      "qwen3-8b",
      "3 120",
      "212",
      "chat.generation.seconds(value=0,48)",
      "chat.generation.seconds(value=5,2)",
      "chat.generation.call_end.tool_calls",
    ]);
  });

  test("an unknown value of a call reads not measured", () => {
    // GIVEN a call whose engine measured no first token and gave no reason
    const call: TurnLlmCall = { ...CALL, ttft_ms: null, prompt_tokens: null, finish_reason: null };

    // WHEN its card is built
    const texts = callFields(call, "en").map((f) => callFieldText(f, echo));

    // THEN those values say so
    expect(texts[1]).toBe("chat.generation.unmeasured");
    expect(texts[3]).toBe("chat.generation.unmeasured");
    expect(texts[5]).toBe("chat.generation.unmeasured");
    expect(callEndKey("stop")).toBe("chat.generation.call_end.stop");
    expect(callEndKey("cancelled")).toBe("chat.generation.call_end.cancelled");
  });
});

describe("the model calls in the steps panel", () => {
  test("each call of a turn becomes one llm event, in order", () => {
    // GIVEN the two calls of a turn
    const calls = [CALL, { ...CALL, ttft_ms: null, finish_reason: "stop" }];

    // WHEN the panel builds its events in the default wording
    const events = llmJournalEvents(calls, {
      developer: false,
      loc: "en",
      time: "10:42",
      translate: echo,
    });

    // THEN there is one llm event per call, numbered, with its end
    expect(events.map((e) => e.type)).toEqual(["llm", "llm"]);
    expect(events[0].heading).toBe("chat.generation.call_title(n=1)");
    expect(events[1].heading).toBe("chat.generation.call_title(n=2)");
    expect(events[0].body).toContain("chat.generation.call.model qwen3-8b");
    expect(events[0].body).toContain("chat.generation.call.end chat.generation.call_end.tool_calls");
    expect(events[1].body).toContain("chat.generation.call.ttft chat.generation.unmeasured");
    expect(events[1].time).toBe("10:42");
  });

  test("the developer wording lists the raw fields of the call", () => {
    // GIVEN a call with no measured first token
    const calls = [{ ...CALL, ttft_ms: null }];

    // WHEN the panel builds its events in the developer wording
    const [event] = llmJournalEvents(calls, {
      developer: true,
      loc: "en",
      time: "",
      translate: echo,
    });

    // THEN the heading is the model and the body its fields, null kept as null
    expect(event.heading).toBe("qwen3-8b");
    expect(event.body).toBe(
      "prompt_tokens=3120 · completion_tokens=212 · ttft_ms=null · latency_ms=5200 · finish_reason=tool_calls",
    );
  });

  test("a turn with no call adds nothing", () => {
    // GIVEN no call
    // WHEN the events are built
    // THEN there is none
    expect(
      llmJournalEvents([], { developer: false, loc: "en", time: "", translate: echo }),
    ).toEqual([]);
  });
});
