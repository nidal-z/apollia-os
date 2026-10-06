/**
 * What the composer offers under the text field, from what the model of the
 * conversation can do and what the conversation already set.
 *
 * A model that has no reasoning setting, or whose setting is not known (a
 * remote server, a model not indexed yet), gets no reasoning chip: a chip is
 * never shown for a capability the model does not have.
 */
import type { ChatModelCapabilities, ReasoningEffort, SessionSettings } from "$lib/types";

/** The levels of reasoning, in the order the menu lists them. */
export const REASONING_LEVELS: readonly ReasoningEffort[] = ["high", "medium", "low"];

/** The chips of one conversation. */
export interface ComposerChips {
  /**
   * The reasoning switch, for a model that reasons on demand; `null` hides
   * it. `on` is true until the conversation switches reasoning off.
   */
  thinking: { on: boolean } | null;
  /**
   * The level of reasoning, for a model that reads one; `null` hides it.
   * `level` is `null` while the conversation follows the default of the model.
   */
  effort: { level: ReasoningEffort | null } | null;
  /** The model reads images. */
  vision: boolean;
}

/** The chips to show for `capabilities` in a conversation set to `settings`. */
export function composerChips(
  capabilities: ChatModelCapabilities | null,
  settings: SessionSettings | null,
): ComposerChips {
  const profile = capabilities?.reasoning_profile ?? null;
  return {
    thinking: profile === "toggle" ? { on: settings?.thinking !== false } : null,
    effort: profile === "levels" ? { level: settings?.reasoning_effort ?? null } : null,
    vision: capabilities?.vision === true,
  };
}

/** Whether `chips` shows anything. */
export function hasChips(chips: ComposerChips): boolean {
  return chips.thinking !== null || chips.effort !== null || chips.vision;
}

/** `settings` with reasoning switched `on` or off, every other field kept. */
export function withThinking(settings: SessionSettings, on: boolean): SessionSettings {
  return { ...settings, thinking: on };
}

/**
 * `settings` with the level of reasoning set to `level`, every other field
 * kept. `null` removes the field: the conversation follows the default of the
 * model again.
 */
export function withReasoningEffort(
  settings: SessionSettings,
  level: ReasoningEffort | null,
): SessionSettings {
  const { reasoning_effort: _dropped, ...rest } = settings;
  return level === null ? rest : { ...rest, reasoning_effort: level };
}
