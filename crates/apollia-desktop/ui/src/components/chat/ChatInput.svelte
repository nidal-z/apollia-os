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
    if (textareaEl) {
      textareaEl.style.height = "auto";
    }
  }

  $effect(() => {
    if (!disabled && textareaEl) {
      textareaEl.focus();
    }
  });
</script>

<div class="px-4 pb-4 pt-2" data-testid="chat-input">
  <div class="flex items-end gap-2.5 rounded-2xl glass-card glass-border border px-3 py-2.5 shadow-lg">
    <textarea
      bind:this={textareaEl}
      bind:value
      oninput={autoResize}
      onkeydown={handleKeydown}
      {disabled}
      rows="1"
      placeholder={$t("chat.input_placeholder")}
      class="flex-1 resize-none bg-transparent px-1.5 py-1 text-sm text-foreground
        outline-none placeholder:text-muted-foreground/50
        disabled:cursor-not-allowed disabled:opacity-50"
    ></textarea>
    <button
      onclick={send}
      disabled={disabled || !value.trim()}
      class="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-xl
        bg-gradient-to-br from-primary to-secondary text-white shadow-md
        transition-all hover:shadow-lg hover:scale-105 active:scale-95
        disabled:pointer-events-none disabled:opacity-40 disabled:shadow-none"
      data-testid="chat-send-button"
    >
      <Send class="h-4 w-4" />
    </button>
  </div>
</div>
