/**
 * Unit tests for the LLM backend dialog validator.
 *
 * Covers: required fields, URL format, provider-specific rules, JSON
 * validation, and the config_json shaping used by the Save handler.
 */
import { describe, it, expect } from "vitest";
import {
  validateForm,
  buildConfigJson,
  isRemoteProvider,
  type BackendFormState,
} from "./LlmBackendDialog.svelte";

function baseForm(overrides: Partial<BackendFormState> = {}): BackendFormState {
  return {
    name: "local-code",
    provider: "llama-cpp",
    endpoint: "",
    apiKey: "",
    model: "qwen3-0.6b-q8_0",
    timeoutSec: 60,
    enabled: true,
    isDefault: false,
    extraJson: "",
    // Generation params : `null` = use model defaults (sinon buildConfigJson
    // les sérialise dans config_json même si undefined → overwrite extraJson).
    temperature: null,
    topK: null,
    topP: null,
    repeatPenalty: null,
    contextSize: null,
    ...overrides,
  };
}

describe("isRemoteProvider", () => {
  it("marks remote providers correctly", () => {
    expect(isRemoteProvider("openai")).toBe(true);
    expect(isRemoteProvider("anthropic")).toBe(true);
    expect(isRemoteProvider("mistral")).toBe(true);
    expect(isRemoteProvider("ollama")).toBe(true);
    expect(isRemoteProvider("llama-cpp")).toBe(false);
  });
});

describe("validateForm", () => {
  it("passes for a valid llama-cpp backend", () => {
    // GIVEN a valid local backend
    const errors = validateForm(baseForm(), false);
    // THEN no errors
    expect(errors).toEqual({});
  });

  it("reports missing name", () => {
    const errors = validateForm(baseForm({ name: "" }), false);
    expect(errors.name).toBe("required");
  });

  it("rejects names with uppercase or spaces", () => {
    const errors = validateForm(baseForm({ name: "My Backend" }), false);
    expect(errors.name).toBe("invalid_name");
  });

  it("requires a model", () => {
    const errors = validateForm(baseForm({ model: "" }), false);
    expect(errors.model).toBe("required");
  });

  it("requires endpoint + api_key for openai when creating", () => {
    // GIVEN an openai backend with no endpoint/key
    const errors = validateForm(
      baseForm({ provider: "openai", endpoint: "", apiKey: "", model: "gpt-4" }),
      false,
    );
    // THEN both endpoint and apiKey are flagged
    expect(errors.endpoint).toBe("required");
    expect(errors.apiKey).toBe("required");
  });

  it("rejects malformed endpoint URL", () => {
    const errors = validateForm(
      baseForm({ provider: "openai", endpoint: "not-a-url", apiKey: "sk-x", model: "gpt-4" }),
      false,
    );
    expect(errors.endpoint).toBe("invalid_url");
  });

  it("allows missing api_key for ollama", () => {
    const errors = validateForm(
      baseForm({
        provider: "ollama",
        endpoint: "http://localhost:11434",
        apiKey: "",
        model: "llama3",
      }),
      false,
    );
    expect(errors.apiKey).toBeUndefined();
    expect(errors.endpoint).toBeUndefined();
  });

  it("allows empty api_key when editing", () => {
    // GIVEN an edit flow where the user leaves the api_key blank (keep existing)
    const errors = validateForm(
      baseForm({
        provider: "openai",
        endpoint: "https://api.openai.com/v1",
        apiKey: "",
        model: "gpt-4",
      }),
      true,
    );
    // THEN api_key is not flagged as required
    expect(errors.apiKey).toBeUndefined();
  });

  it("rejects non-positive timeouts", () => {
    expect(validateForm(baseForm({ timeoutSec: 0 }), false).timeoutSec).toBe("invalid_timeout");
    expect(validateForm(baseForm({ timeoutSec: -5 }), false).timeoutSec).toBe("invalid_timeout");
  });

  it("rejects invalid extra JSON", () => {
    expect(validateForm(baseForm({ extraJson: "{not json" }), false).extraJson).toBe(
      "invalid_json",
    );
  });

  it("rejects non-object JSON", () => {
    expect(validateForm(baseForm({ extraJson: "[1,2,3]" }), false).extraJson).toBe(
      "invalid_json_object",
    );
  });
});

describe("buildConfigJson", () => {
  it("writes the URL under canonical base_url + api_key for remote providers", () => {
    const cfg = buildConfigJson(
      baseForm({
        provider: "openai",
        endpoint: "https://api.openai.com/v1",
        apiKey: "sk-x",
        model: "gpt-4",
        timeoutSec: 30,
      }),
    );
    expect(cfg).toEqual({
      base_url: "https://api.openai.com/v1",
      api_key: "sk-x",
      timeout_sec: 30,
    });
  });

  it("omits base_url/api_key for local providers", () => {
    const cfg = buildConfigJson(baseForm());
    expect(cfg.base_url).toBeUndefined();
    expect(cfg.api_key).toBeUndefined();
    expect(cfg.timeout_sec).toBe(60);
  });

  it("merges extra JSON object with remote fields", () => {
    const cfg = buildConfigJson(
      baseForm({
        provider: "openai",
        endpoint: "https://api.openai.com/v1",
        apiKey: "sk-x",
        model: "gpt-4",
        extraJson: JSON.stringify({ temperature: 0.2, device: "cuda" }),
      }),
    );
    expect(cfg.temperature).toBe(0.2);
    expect(cfg.device).toBe("cuda");
    expect(cfg.base_url).toBe("https://api.openai.com/v1");
  });

  it("converges legacy endpoint/api_url aliases onto base_url", () => {
    const cfg = buildConfigJson(
      baseForm({
        provider: "openai",
        endpoint: "https://real.example",
        apiKey: "sk-real",
        model: "gpt-4",
        extraJson: JSON.stringify({
          endpoint: "https://stale.example",
          api_url: "https://also-stale.example",
        }),
      }),
    );
    expect(cfg.base_url).toBe("https://real.example");
    expect(cfg.endpoint).toBeUndefined();
    expect(cfg.api_url).toBeUndefined();
  });
});
