<script lang="ts">
  import { t } from "svelte-i18n";
  import { Send } from "lucide-svelte";

  interface Props {
    disabled: boolean;
    onsend: (content: string) => void;
  }

  let { disabled, onsend }: Props = $props();

  let value = $state("");
  let textareaEl = $state<HTMLTextAreaElement | undefined>(undefined);

  function autoResize() {
    if (!textareaEl) return;
    textareaEl.style.height = "auto";
    textareaEl.style.height = Math.min(textareaEl.scrollHeight, 160) + "px";
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      send();
    }
  }

  function send() {
    const trimmed = value.trim();
    if (!trimmed || disabled) return;
    onsend(trimmed);
    value = "";
    if (textareaEl) textareaEl.style.height = "auto";
  }

  $effect(() => {
    if (!disabled && textareaEl) textareaEl.focus();
  });
</script>

<div class="border-t border-border/30 px-4 pb-3 pt-2" data-testid="chat-input">
  <div class="flex items-end gap-2">
    <textarea
      bind:this={textareaEl}
      bind:value
      oninput={autoResize}
      onkeydown={handleKeydown}
      {disabled}
      rows="1"
      placeholder={$t("chat.input_placeholder")}
      class="flex-1 resize-none rounded-lg border border-border/40 bg-card/50 px-3 py-2 text-sm text-foreground
        outline-none transition-shadow placeholder:text-muted-foreground/40
        focus:ring-1 focus:ring-primary/20 focus:border-primary/30
        disabled:cursor-not-allowed disabled:opacity-50"
    ></textarea>
    <button
      onclick={send}
      disabled={disabled || !value.trim()}
      class="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-lg
        bg-primary text-primary-foreground
        transition-all hover:bg-primary/90 active:scale-[0.96]
        disabled:pointer-events-none disabled:opacity-30"
      data-testid="chat-send-button"
    >
      <Send size={15} />
    </button>
  </div>
</div>
