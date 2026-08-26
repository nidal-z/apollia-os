<!--
  Keyboard hints line shown under the chat input.

  Slim, low-contrast row: Enter to send · Shift+Enter for newline · / for
  commands. Visually recedes until the input is focused.
-->
<script lang="ts">
  import { t } from "svelte-i18n";

  interface Props {
    /** When provided, shown as a compact right-aligned status (e.g. rate-limit). */
    status?: string | null;
    /** Tone for the status chip. */
    statusTone?: "neutral" | "warn";
  }

  let { status = null, statusTone = "neutral" }: Props = $props();
</script>

<div
  class="mt-1 flex items-center justify-between gap-2 px-1 text-micro text-muted-foreground/50"
  data-testid="chat-input-hints"
>
  <div class="flex items-center gap-2 truncate">
    <span><kbd class="rounded bg-muted/50 px-1 py-px text-micro-xs">{$t("chat.hints.key_enter")}</kbd> {$t("chat.hints.send")}</span>
    <span aria-hidden="true">·</span>
    <span><kbd class="rounded bg-muted/50 px-1 py-px text-micro-xs">⇧</kbd> + <kbd class="rounded bg-muted/50 px-1 py-px text-micro-xs">{$t("chat.hints.key_enter")}</kbd> {$t("chat.hints.newline")}</span>
    <span aria-hidden="true">·</span>
    <span><kbd class="rounded bg-muted/50 px-1 py-px text-micro-xs">/</kbd> {$t("chat.hints.commands")}</span>
  </div>
  {#if status}
    <span
      class="shrink-0 rounded px-1.5 py-px text-micro"
      class:text-warning={statusTone === "warn"}
      class:bg-warning-subtle={statusTone === "warn"}
      data-testid="chat-input-hint-status"
    >
      {status}
    </span>
  {/if}
</div>
