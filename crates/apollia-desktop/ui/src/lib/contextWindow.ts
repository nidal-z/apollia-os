/**
 * Context window choices, shared by onboarding and the backend settings.
 *
 * The window is stored as `config_json.context_window` on the backend. The
 * embedded engine is launched with it (`-c`), Ollama is asked for it on every
 * call (`num_ctx`), and the router sizes compaction and the context gauge
 * against it, so one figure governs the whole path.
 */

/** Runtime default when a backend sets no window. Mirrors the engine's `-c`. */
export const DEFAULT_CONTEXT_WINDOW = 32_768;

/** Windows offered in a picker, smallest first. */
export const CONTEXT_WINDOW_CHOICES: readonly number[] = [
  8_192, 16_384, 32_768, 65_536, 131_072,
];

/**
 * The choices to list for a stored value. A window set elsewhere (the CLI, an
 * older build) that is not one of the presets is kept in the list, so opening
 * the dialog never silently changes it.
 */
export function contextWindowChoices(current: number | null): number[] {
  const choices = [...CONTEXT_WINDOW_CHOICES];
  if (current !== null && current > 0 && !choices.includes(current)) {
    choices.push(current);
    choices.sort((a, b) => a - b);
  }
  return choices;
}

/** `32768` reads as `32k`; a value that is not a multiple of 1024 stays exact. */
export function formatContextWindow(tokens: number): string {
  return tokens % 1024 === 0 ? `${tokens / 1024}k` : String(tokens);
}
