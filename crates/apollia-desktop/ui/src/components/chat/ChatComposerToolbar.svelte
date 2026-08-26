<!--
  The gutter under the composer textarea.

  Secondary actions on the left (plan toggle, attach, dictate, rewrite, the two
  menu triggers), send or stop anchored right. It owns no state: every control
  reports to the composer, which holds the text buffer and the controllers.
-->
<script lang="ts">
  import { t } from "svelte-i18n";
  import {
    Send,
    Square,
    Paperclip,
    Mic,
    MicOff,
    Slash,
    AtSign,
    ListChecks,
    Wand2,
    Loader2,
    Undo2,
  } from "lucide-svelte";
  import type { ComposerDictation } from "./useComposerDictation.svelte";

  interface Props {
    disabled: boolean;
    /** True while a turn is generating - swaps Send for a Stop control. */
    busy: boolean;
    /** Whether the ⌘↵ hint is worth showing. */
    isDesktop: boolean;
    canSend: boolean;
    /** False when the textarea is empty: rewriting nothing rewrites nothing. */
    hasText: boolean;
    planMode: boolean;
    planDisabled: boolean;
    onplantoggle?: () => void;
    dictation: ComposerDictation;
    isRewriting: boolean;
    canRestore: boolean;
    onattach: () => void;
    onrewrite: () => void;
    onrestore: () => void;
    ontrigger: (char: "/" | "@") => void;
    onstop?: () => void;
    onsend: () => void;
  }

  let {
    disabled,
    busy,
    isDesktop,
    canSend,
    hasText,
    planMode,
    planDisabled,
    onplantoggle,
    dictation,
    isRewriting,
    canRestore,
    onattach,
    onrewrite,
    onrestore,
    ontrigger,
    onstop,
    onsend,
  }: Props = $props();

  // Shared styling for the composer toolbar icon buttons (single source so the
  // gutter no longer mixes h-7 / h-8 and Button-vs-button treatments).
  const toolBtn =
    "inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-muted-foreground/70 transition-colors hover:bg-muted hover:text-foreground disabled:opacity-40 disabled:cursor-not-allowed";
</script>

<div class="flex items-center gap-0.5 border-t border-border/40 px-2 py-1.5">
  {#if onplantoggle}
    <button
      type="button"
      onclick={onplantoggle}
      disabled={disabled || planDisabled}
      aria-pressed={planMode}
      class="mr-1 inline-flex items-center gap-1.5 rounded-md border px-2 py-1 text-caption font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50 {planMode
        ? 'border-primary/40 bg-primary/12 text-primary'
        : 'border-border text-muted-foreground hover:bg-muted/50 hover:text-foreground'}"
      aria-label={$t("chat.planMode.chipLabel")}
      title={$t("chat.planMode.chipLabel")}
      data-testid="composer-plan-toggle"
    >
      <ListChecks size={12} class="shrink-0" />
      {$t("chat.planMode.chipLabel")}
    </button>
  {/if}
  <button
    type="button"
    onclick={onattach}
    {disabled}
    class={toolBtn}
    aria-label={$t("chat.attachments.add")}
    title={$t("chat.attachments.add")}
    data-testid="chat-attach-button"
  >
    <Paperclip size={16} />
  </button>
  <button
    type="button"
    onclick={() => dictation.toggle()}
    disabled={disabled || (dictation.busy && !dictation.recording)}
    class="{toolBtn} {dictation.recording ? 'bg-destructive text-destructive-foreground hover:bg-destructive hover:text-destructive-foreground mic-pulse' : ''}"
    aria-label={dictation.recording ? $t("chat.dictate_stop") : $t("chat.dictate_start")}
    title={dictation.recording ? $t("chat.dictate_stop") : $t("chat.dictate_start")}
    data-testid="chat-mic-button"
  >
    {#if dictation.recording}
      <MicOff size={16} />
    {:else}
      <Mic size={16} />
    {/if}
  </button>
  <button
    type="button"
    onclick={onrewrite}
    disabled={disabled || isRewriting || !hasText}
    class={toolBtn}
    aria-label={$t("chat.rewrite.button_tooltip")}
    title={$t("chat.rewrite.button_tooltip")}
    data-testid="chat-input-rewrite-button"
  >
    {#if isRewriting}
      <Loader2 size={16} class="animate-spin" />
    {:else}
      <Wand2 size={16} />
    {/if}
  </button>
  {#if canRestore}
    <button
      type="button"
      onclick={onrestore}
      disabled={disabled || isRewriting}
      class={toolBtn}
      aria-label={$t("chat.rewrite.restore_tooltip")}
      title={$t("chat.rewrite.restore_tooltip")}
      data-testid="chat-input-rewrite-restore-button"
    >
      <Undo2 size={16} />
    </button>
  {/if}
  <button
    type="button"
    onclick={() => ontrigger("/")}
    {disabled}
    class={toolBtn}
    aria-label={$t("chat.slash_commands")}
    title={$t("chat.slash_commands")}
    data-testid="chat-slash-button"
  >
    <Slash size={16} />
  </button>
  <button
    type="button"
    onclick={() => ontrigger("@")}
    {disabled}
    class={toolBtn}
    aria-label={$t("chat.mention_resources")}
    title={$t("chat.mention_resources")}
    data-testid="chat-mention-button"
  >
    <AtSign size={16} />
  </button>

  <div class="flex-1"></div>

  {#if isDesktop}
    <span class="mr-1.5 select-none text-caption text-muted-foreground/50" aria-hidden="true">
      ⌘↵
    </span>
  {/if}
  {#if busy && onstop}
    <button
      type="button"
      onclick={() => onstop?.()}
      class="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md
        bg-destructive text-destructive-foreground shadow-warm-glow
        transition-all active:scale-[0.96]"
      aria-label={$t("chat.stop")}
      title={$t("chat.stop")}
      data-testid="chat-stop-button"
    >
      <Square size={14} class="fill-current" />
    </button>
  {:else}
    <button
      onclick={onsend}
      disabled={!canSend}
      class="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md
        transition-all active:scale-[0.96]
        disabled:pointer-events-none disabled:opacity-40 disabled:cursor-not-allowed"
      class:bg-primary-solid={canSend}
      class:text-primary-foreground={canSend}
      class:shadow-warm-glow={canSend}
      class:bg-muted={!canSend}
      class:text-muted-foreground={!canSend}
      aria-label={$t("chat.send")}
      data-testid="chat-send-button"
    >
      <Send size={16} />
    </button>
  {/if}
</div>

<style>
  /* Subtle pulse around the mic button while recording. */
  :global(.mic-pulse) {
    box-shadow: 0 0 0 0 hsl(var(--destructive) / 0.5);
    animation: mic-pulse 1.4s ease-out infinite;
  }
  @keyframes mic-pulse {
    0% {
      box-shadow: 0 0 0 0 hsl(var(--destructive) / 0.55);
    }
    70% {
      box-shadow: 0 0 0 6px hsl(var(--destructive) / 0);
    }
    100% {
      box-shadow: 0 0 0 0 hsl(var(--destructive) / 0);
    }
  }
  :global(.shadow-warm-glow) {
    box-shadow: var(--shadow-warm-focus);
  }
</style>
