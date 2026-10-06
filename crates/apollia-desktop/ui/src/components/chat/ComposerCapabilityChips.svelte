<!--
  The chips under the text field, one per thing the model of the conversation
  can do: reason on demand, reason at a chosen level, read images.

  A chip is absent for a model that lacks the capability. The two reasoning
  chips write the settings of the conversation: the change holds for its next
  turns and for it alone, the agent keeps its own. The vision chip states a
  capability and writes nothing.
-->
<script lang="ts">
  import { locale, t } from "svelte-i18n";
  import { Brain, Check, ChevronDown, Eye, Undo2 } from "lucide-svelte";
  import { ActionMenu } from "$lib/components/ui/action-menu";
  import { addToast } from "$lib/components/ui/toast";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import {
    REASONING_LEVELS,
    composerChips,
    hasChips,
    withReasoningEffort,
    withThinking,
  } from "$lib/chat/composerCapabilities";
  import {
    changeSessionSettings,
    loadSessionSettings,
    sessionSettingsStore,
  } from "$lib/stores/sessionSettings";
  import type { ChatModelCapabilities, ReasoningEffort, SessionSettings } from "$lib/types";

  interface Props {
    /** The conversation the chips set; `null` before one exists. */
    sessionId: string | null;
    /** What its model can do; `null` while it is not known. */
    capabilities: ChatModelCapabilities | null;
    /** True when the conversation takes no input (closed, or no model). */
    disabled: boolean;
  }

  let { sessionId, capabilities, disabled }: Props = $props();

  const settings = $derived(sessionSettingsStore(sessionId ?? ""));
  const chips = $derived(composerChips(capabilities, $settings));
  const reasons = $derived(chips.thinking !== null || chips.effort !== null);

  let writing = $state(false);
  let menuOpen = $state(false);

  // The settings are read only for a model that has a reasoning chip to show.
  // A read that fails leaves the chip on the default of the model.
  $effect(() => {
    const id = sessionId;
    if (!id || !reasons) return;
    loadSessionSettings(id).catch(() => {});
  });

  async function apply(change: (current: SessionSettings) => SessionSettings): Promise<void> {
    if (!sessionId || writing) return;
    writing = true;
    try {
      await changeSessionSettings(sessionId, change);
    } catch {
      addToast($t("chat.chips.write_error"), "error");
    } finally {
      writing = false;
    }
  }

  const LEVEL_KEYS: Record<ReasoningEffort, string> = {
    high: "chat.chips.levels.high",
    medium: "chat.chips.levels.medium",
    low: "chat.chips.levels.low",
  };

  function levelLabel(level: ReasoningEffort): string {
    return $t(LEVEL_KEYS[level]);
  }

  const effortValue = $derived(
    chips.effort?.level
      ? levelLabel(chips.effort.level).toLocaleLowerCase($locale ?? undefined)
      : $t("chat.chips.effort_default"),
  );

  const chip =
    "inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md border px-2 text-caption font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50";
  const chipOn = "border-primary/40 bg-primary/12 text-primary";
  const chipOff = "border-border text-muted-foreground hover:bg-muted/50 hover:text-foreground";
  const menuRow =
    "flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-body-xs text-foreground transition-colors hover:bg-muted disabled:cursor-not-allowed disabled:opacity-50";
</script>

{#if hasChips(chips)}
  <div class="ml-1 flex items-center gap-1" data-testid="composer-capability-chips">
    {#if chips.thinking}
      {@const on = chips.thinking.on}
      <Tooltip content={$t(on ? "chat.chips.thinking_on_hint" : "chat.chips.thinking_off_hint")}>
        {#snippet trigger(triggerProps)}
          <button
            {...triggerProps}
            type="button"
            class="{chip} {on ? chipOn : chipOff}"
            aria-pressed={on}
            disabled={disabled || writing}
            onclick={() => void apply((current) => withThinking(current, !on))}
            data-testid="composer-chip-thinking"
          >
            <Brain size={12} class="shrink-0" aria-hidden="true" />
            {$t("chat.chips.thinking")}
          </button>
        {/snippet}
      </Tooltip>
    {/if}

    {#if chips.effort}
      {@const level = chips.effort.level}
      <ActionMenu bind:open={menuOpen} side="top" align="start">
        {#snippet triggerSlot(triggerProps)}
          <button
            {...triggerProps}
            type="button"
            class="{chip} {level ? chipOn : chipOff}"
            aria-label={$t("chat.chips.effort_menu")}
            disabled={disabled || writing}
            data-testid="composer-chip-effort"
            data-level={level ?? "default"}
          >
            <Brain size={12} class="shrink-0" aria-hidden="true" />
            {$t("chat.chips.effort", { values: { value: effortValue } })}
            <ChevronDown size={12} class="shrink-0" aria-hidden="true" />
          </button>
        {/snippet}
        {#snippet body({ close })}
          <ul class="flex flex-col gap-0.5" role="menu" aria-label={$t("chat.chips.effort_menu")}>
            {#each REASONING_LEVELS as option (option)}
              <li role="none">
                <button
                  type="button"
                  role="menuitemradio"
                  aria-checked={level === option}
                  class={menuRow}
                  onclick={() => {
                    close();
                    void apply((current) => withReasoningEffort(current, option));
                  }}
                  data-testid="composer-effort-{option}"
                >
                  <span class="flex-1">{levelLabel(option)}</span>
                  {#if level === option}
                    <Check size={14} aria-hidden="true" />
                  {/if}
                </button>
              </li>
            {/each}
            <li role="none" class="mt-0.5 border-t border-border/60 pt-0.5">
              <button
                type="button"
                role="menuitem"
                class={menuRow}
                disabled={level === null}
                onclick={() => {
                  close();
                  void apply((current) => withReasoningEffort(current, null));
                }}
                data-testid="composer-effort-default"
              >
                <Undo2 size={14} aria-hidden="true" />
                <span class="flex-1">{$t("chat.chips.effort_reset")}</span>
              </button>
            </li>
          </ul>
        {/snippet}
      </ActionMenu>
    {/if}

    {#if chips.vision}
      <Tooltip content={$t("chat.chips.vision_hint")}>
        <span class="{chip} border-border text-muted-foreground" data-testid="composer-chip-vision">
          <Eye size={12} class="shrink-0" aria-hidden="true" />
          {$t("chat.chips.vision")}
        </span>
      </Tooltip>
    {/if}
  </div>
{/if}
