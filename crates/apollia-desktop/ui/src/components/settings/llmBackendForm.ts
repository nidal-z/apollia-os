// The form behind `LlmBackendDialog.svelte`: its shape, its defaults, its
// validation and the configuration JSON it produces.
//
// Nothing here reads component state, so the rules stay node-testable and the
// dialog keeps only the interaction. `llmBackendIpc.ts` is the sibling module
// that carries the calls.

import type { LlmBackendConfig } from "$lib/types";

export type LlmProvider = LlmBackendConfig["provider"];

export interface BackendFormState {
  name: string;
  provider: LlmProvider;
  endpoint: string;
  apiKey: string;
  model: string;
  timeoutSec: number;
  enabled: boolean;
  isDefault: boolean;
  extraJson: string;
  // Generation parameters (null = not set / use model defaults)
  temperature: number | null;
  topK: number | null;
  topP: number | null;
  repeatPenalty: number | null;
  contextSize: number | null;
}

export interface ValidationErrors {
  name?: string;
  provider?: string;
  endpoint?: string;
  apiKey?: string;
  model?: string;
  timeoutSec?: string;
  extraJson?: string;
}

const URL_RE = /^https?:\/\/[^\s]+$/i;
const NAME_RE = /^[a-z0-9_-]+$/;

/** Returns `true` when this provider needs a remote endpoint + api key. */
export function isRemoteProvider(p: LlmProvider): boolean {
  return p === "openai" || p === "mistral" || p === "anthropic" || p === "ollama";
}

/**
 * Endpoint each provider expects, so the user never has to guess it.
 *
 * The `/v1` suffix is not cosmetic and it is not uniform. The
 * OpenAI-compatible client appends `/chat/completions` to the base, so those
 * providers need the base to already end in `/v1`. The Anthropic client
 * appends `/v1/messages` itself, so adding `/v1` here would produce
 * `/v1/v1/messages` and a 404. Both mistakes are easy to make by hand, which
 * is exactly why this table exists.
 */
export const PROVIDER_DEFAULT_ENDPOINT: Partial<Record<LlmProvider, string>> = {
  openai: "https://api.openai.com/v1",
  mistral: "https://api.mistral.ai/v1",
  anthropic: "https://api.anthropic.com",
  ollama: "http://localhost:11434/v1",
};

/** Every value the prefill is allowed to overwrite: empty, or another provider's default. */
const DEFAULT_ENDPOINTS = Object.values(PROVIDER_DEFAULT_ENDPOINT);

/**
 * Endpoint to show after a provider change. Returns `current` untouched as
 * soon as the user typed something of their own, so a self-hosted gateway is
 * never clobbered by switching provider back and forth.
 */
export function endpointForProvider(p: LlmProvider, current: string): string {
  const trimmed = current.trim();
  if (trimmed !== "" && !DEFAULT_ENDPOINTS.includes(trimmed)) return current;
  return PROVIDER_DEFAULT_ENDPOINT[p] ?? "";
}

/** Hosts where cleartext HTTP never leaves the machine. */
function isLoopbackHost(host: string): boolean {
  const h = host.toLowerCase().replace(/^\[|\]$/g, "");
  return h === "localhost" || h === "127.0.0.1" || h === "::1" || h.endsWith(".localhost");
}

/**
 * Flags an endpoint that would put credentials on the wire in cleartext.
 *
 * Returns a warning, never a validation error: reaching a backend over plain
 * HTTP on a trusted LAN or through a WireGuard-style tunnel is a legitimate
 * setup, and refusing it would block real deployments. But an API key sent
 * over `http://` to another host travels in the clear, and nothing else in the
 * product says so.
 *
 * Loopback is exempt: the bytes never reach a network interface.
 */
export function credentialExposureWarning(state: BackendFormState): boolean {
  if (!isRemoteProvider(state.provider)) return false;
  if (!state.apiKey.trim()) return false;
  const raw = state.endpoint.trim();
  if (!raw.toLowerCase().startsWith("http://")) return false;
  try {
    return !isLoopbackHost(new URL(raw).hostname);
  } catch {
    // Unparseable: validateForm already reports invalid_url, stay quiet here.
    return false;
  }
}

/** Pure validator - unit-testable. */
export function validateForm(state: BackendFormState, editing: boolean): ValidationErrors {
  const errors: ValidationErrors = {};
  if (!state.name.trim()) {
    errors.name = "required";
  } else if (!NAME_RE.test(state.name)) {
    errors.name = "invalid_name";
  }
  if (!state.model.trim()) {
    errors.model = "required";
  }
  if (isRemoteProvider(state.provider)) {
    if (!state.endpoint.trim()) {
      errors.endpoint = "required";
    } else if (!URL_RE.test(state.endpoint.trim())) {
      errors.endpoint = "invalid_url";
    }
    // api_key optional for ollama; required for others
    if (state.provider !== "ollama" && !state.apiKey.trim() && !editing) {
      errors.apiKey = "required";
    }
  }
  if (!Number.isFinite(state.timeoutSec) || state.timeoutSec <= 0) {
    errors.timeoutSec = "invalid_timeout";
  }
  if (state.extraJson.trim().length > 0) {
    try {
      const parsed = JSON.parse(state.extraJson);
      if (typeof parsed !== "object" || Array.isArray(parsed) || parsed === null) {
        errors.extraJson = "invalid_json_object";
      }
    } catch {
      errors.extraJson = "invalid_json";
    }
  }
  return errors;
}

export function buildConfigJson(state: BackendFormState): Record<string, unknown> {
  let base: Record<string, unknown> = {};
  if (state.extraJson.trim()) {
    try {
      base = JSON.parse(state.extraJson) as Record<string, unknown>;
    } catch {
      base = {};
    }
  }
  // For llama-cpp, model_path in config_json must always match form.model.
  // This overwrites any stale model_path that may have survived in extraJson.
  if (state.provider === "llama-cpp" && state.model.trim()) {
    base.model_path = state.model.trim();
    delete base.model_paths;
  }
  if (isRemoteProvider(state.provider)) {
    // Persist the URL under the canonical `base_url` key (the one the router
    // reads). Drop the legacy `endpoint`/`api_url` aliases so an edited row
    // converges on a single key instead of keeping a stale duplicate.
    if (state.endpoint.trim()) base.base_url = state.endpoint.trim();
    delete base.endpoint;
    delete base.api_url;
    if (state.apiKey.trim()) base.api_key = state.apiKey.trim();
  }
  if (Number.isFinite(state.timeoutSec) && state.timeoutSec > 0) {
    base.timeout_sec = state.timeoutSec;
  }
  if (state.temperature !== null) base.temperature = state.temperature;
  if (state.topK !== null) base.top_k = state.topK;
  if (state.topP !== null) base.top_p = state.topP;
  if (state.repeatPenalty !== null) base.repeat_penalty = state.repeatPenalty;
  if (state.contextSize !== null) base.context_size = state.contextSize;
  return base;
}

function numOrNull(v: unknown): number | null {
  return typeof v === "number" && Number.isFinite(v) ? v : null;
}

export function seedFrom(b: LlmBackendConfig | null): BackendFormState {
  if (!b) {
    return {
      name: "",
      provider: "llama-cpp",
      endpoint: "",
      apiKey: "",
      model: "",
      timeoutSec: 60,
      enabled: true,
      isDefault: false,
      extraJson: "",
      temperature: null,
      topK: null,
      topP: null,
      repeatPenalty: null,
      contextSize: null,
    };
  }
  const cfg = (b.config_json ?? {}) as Record<string, unknown>;
  // The URL reads from the canonical `base_url`, falling back to the legacy
  // `endpoint`/`api_url` aliases so backends persisted before the key
  // convergence still populate the field.
  const urlValue = [cfg.base_url, cfg.endpoint, cfg.api_url].find(
    (v): v is string => typeof v === "string" && v.length > 0,
  );
  const endpoint = urlValue ?? "";
  const apiKey = typeof cfg.api_key === "string" ? cfg.api_key : "";
  const timeout = typeof cfg.timeout_sec === "number" ? cfg.timeout_sec : 60;
  const rest = { ...cfg };
  delete rest.base_url;
  delete rest.endpoint;
  delete rest.api_url;
  delete rest.api_key;
  delete rest.timeout_sec;
  delete rest.temperature;
  delete rest.top_k;
  delete rest.top_p;
  delete rest.repeat_penalty;
  delete rest.context_size;
  // model_path / model_paths are managed via form.model - never round-trip via extraJson
  delete rest.model_path;
  delete rest.model_paths;
  return {
    name: b.name,
    provider: b.provider,
    endpoint,
    apiKey,
    model: b.model,
    timeoutSec: timeout,
    enabled: b.enabled,
    isDefault: b.is_default,
    extraJson: Object.keys(rest).length ? JSON.stringify(rest, null, 2) : "",
    temperature: numOrNull(cfg.temperature),
    topK: numOrNull(cfg.top_k),
    topP: numOrNull(cfg.top_p),
    repeatPenalty: numOrNull(cfg.repeat_penalty),
    contextSize: numOrNull(cfg.context_size),
  };
}

/**
 * Returns multi-part info if the filename matches the llama.cpp split pattern
 * `<name>-<NNNNN>-of-<TOTAL>.gguf`, otherwise null.
 */
export function detectMultipart(filePath: string): { part: number; total: number; firstPath: string } | null {
  const m = filePath.match(/^(.*?)-(\d{5})-of-(\d{5})\.gguf$/i);
  if (!m) return null;
  const part = parseInt(m[2], 10);
  const total = parseInt(m[3], 10);
  const firstPath = `${m[1]}-00001-of-${m[3]}.gguf`;
  return { part, total, firstPath };
}
