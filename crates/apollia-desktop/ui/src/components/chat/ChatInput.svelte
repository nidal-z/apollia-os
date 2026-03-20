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

<div class="flex items-end gap-2 border-t bg-background px-4 py-3" data-testid="chat-input">
  <textarea
    bind:this={textareaEl}
    bind:value
    oninput={autoResize}
    onkeydown={handleKeydown}
    {disabled}
    rows="1"
    placeholder={$t("chat.input_placeholder")}
    class="flex-1 resize-none rounded-md border bg-background px-3 py-2 text-sm outline-none focus:ring-1 focus:ring-primary/40 disabled:cursor-not-allowed disabled:opacity-50"
  ></textarea>
  <button
    onclick={send}
    disabled={disabled || !value.trim()}
    class="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-primary text-white transition-colors hover:bg-primary/90 disabled:pointer-events-none disabled:opacity-50"
    data-testid="chat-send-button"
  >
    <Send class="h-4 w-4" />
  </button>
</div>
