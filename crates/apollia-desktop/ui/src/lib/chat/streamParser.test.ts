import { describe, it, expect } from "vitest";
import { parseStream, isThinking, answerText, normaliseReasoningMarkers } from "./streamParser";

describe("parseStream", () => {
  it("returns an empty array for empty input", () => {
    expect(parseStream("")).toEqual([]);
  });

  it("returns a single text block for plain content", () => {
    expect(parseStream("hello world")).toEqual([
      { type: "text", content: "hello world", closed: true },
    ]);
  });

  it("segments a closed thinking block between text", () => {
    const blocks = parseStream("before<think>reasoning</think>after");
    expect(blocks).toEqual([
      { type: "text", content: "before", closed: true },
      { type: "thinking", content: "reasoning", closed: true },
      { type: "text", content: "after", closed: true },
    ]);
  });

  it("marks an unclosed thinking block as open (still streaming)", () => {
    const blocks = parseStream("intro<think>still reason");
    expect(blocks).toEqual([
      { type: "text", content: "intro", closed: true },
      { type: "thinking", content: "still reason", closed: false },
    ]);
    expect(isThinking(blocks)).toBe(true);
  });

  it("isThinking is false when the last block is text", () => {
    const blocks = parseStream("<think>done</think>final");
    expect(isThinking(blocks)).toBe(false);
  });

  it("segments tool blocks", () => {
    const blocks = parseStream("<tool>call</tool>result");
    expect(blocks).toEqual([
      { type: "tool", content: "call", closed: true },
      { type: "text", content: "result", closed: true },
    ]);
  });

  it("handles multiple interleaved thinking blocks", () => {
    const blocks = parseStream("a<think>x</think>b<think>y</think>c");
    expect(blocks).toHaveLength(5);
    expect(blocks.map((b) => b.type)).toEqual([
      "text",
      "thinking",
      "text",
      "thinking",
      "text",
    ]);
  });

  it("picks the earliest tag when multiple types are present", () => {
    const blocks = parseStream("<tool>t</tool><think>r</think>");
    expect(blocks.map((b) => b.type)).toEqual(["tool", "thinking"]);
  });

  it("stays linear on a very large buffer (10k tokens of text)", () => {
    const chunk = "lorem ipsum dolor sit amet ";
    const big = chunk.repeat(2000);
    const start = performance.now();
    const blocks = parseStream(big);
    const elapsed = performance.now() - start;
    expect(blocks).toHaveLength(1);
    expect(elapsed).toBeLessThan(100);
  });

  it("handles a large buffer with many thinking segments", () => {
    let src = "";
    for (let i = 0; i < 500; i += 1) {
      src += `text-${i}<think>reason-${i}</think>`;
    }
    const blocks = parseStream(src);
    expect(blocks).toHaveLength(1000);
  });
});

describe("answerText", () => {
  it("drops reasoning and keeps the rest in order", () => {
    // GIVEN a stream mixing answer text with a reasoning fragment
    const blocks = parseStream("before<think>hidden</think>after");

    // WHEN the answer is extracted
    const answer = answerText(blocks);

    // THEN only what the model said out loud remains
    expect(answer).toBe("beforeafter");
  });

  it("keeps a tool block's content without its delimiters", () => {
    // GIVEN a tool block inside the stream
    const blocks = parseStream("a<tool>payload</tool>b");

    // WHEN the answer is extracted
    const answer = answerText(blocks);

    // THEN the delimiters are gone and the content stays
    expect(answer).toBe("apayloadb");
  });

  it("never leaks a stream marker into the answer", () => {
    // GIVEN every shape the parser can produce, including unclosed tags mid
    // stream, which is what a live buffer looks like between two tokens
    const sources = [
      "plain text",
      "a<think>r</think>b",
      "a<think>still thinking",
      "a<tool>t</tool>b",
      "a<tool>unclosed",
      "<think>r</think><tool>t</tool>tail",
      "",
    ];

    for (const src of sources) {
      // WHEN the answer is extracted
      const answer = answerText(parseStream(src));

      // THEN it carries no marker, which is the invariant that lets the
      // renderer treat it as plain markdown instead of re-parsing it
      expect(answer).not.toContain("<think>");
      expect(answer).not.toContain("</think>");
      expect(answer).not.toContain("<tool>");
      expect(answer).not.toContain("</tool>");
    }
  });
});

describe("Gemma 4 reasoning markers", () => {
  it("parses a thought channel as thinking rather than as the answer", () => {
    // GIVEN the reply reported from a Gemma 4 model, markers inline
    const raw =
      "<|channel>thought The user just typed test. <channel|>I'm here. How can I help you today?";

    // WHEN it is parsed
    const blocks = parseStream(raw);

    // THEN the reasoning is a thinking block and the answer carries no marker
    expect(blocks[0].type).toBe("thinking");
    expect(answerText(blocks)).toBe("I'm here. How can I help you today?");
  });

  it("treats text before an orphan closing marker as reasoning", () => {
    // GIVEN the turn after a tool result, whose opening marker the template
    // placed in the prompt
    const raw = "three files listed<channel|>You have three files.";

    // WHEN it is parsed
    const blocks = parseStream(raw);

    // THEN the prefix is reasoning and only the answer is shown
    expect(blocks[0]).toMatchObject({ type: "thinking", content: "three files listed", closed: true });
    expect(answerText(blocks)).toBe("You have three files.");
  });

  it("leaves a plain answer untouched", () => {
    // GIVEN a reply with no reasoning at all
    // WHEN it is normalised
    // THEN nothing changes
    expect(normaliseReasoningMarkers("Hello there.")).toBe("Hello there.");
  });
});
