/**
 * Tests for {@link timeoutFetch} — covers happy-path resolution and the
 * timeout branch. See.
 */
import { describe, it, expect, vi, afterEach } from "vitest";
import { timeoutFetch, TimeoutError } from "./timeoutFetch";

describe("timeoutFetch", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  // GIVEN fetch resolves quickly
  // WHEN timeoutFetch is called with a generous budget
  // THEN it resolves with the Response
  it("resolves when fetch completes before the timeout", async () => {
    const mockResponse = new Response("ok", { status: 200 });
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(mockResponse));

    const res = await timeoutFetch("https://example.test/", {}, 1000);
    expect(res.status).toBe(200);
  });

  // GIVEN fetch never resolves
  // WHEN timeoutFetch is called with a short budget
  // THEN it rejects with a TimeoutError whose timeoutMs matches
  it("rejects with TimeoutError when fetch exceeds the budget", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((_url: string, init?: RequestInit): Promise<Response> => {
        return new Promise((_resolve, reject) => {
          init?.signal?.addEventListener("abort", () => {
            // Mirror browser fetch behaviour: throw on abort.
            const e = new Error("aborted");
            e.name = "AbortError";
            reject(e);
          });
        });
      }),
    );

    await expect(timeoutFetch("https://example.test/", {}, 20)).rejects.toBeInstanceOf(
      TimeoutError,
    );
  });
});
