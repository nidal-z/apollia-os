<script lang="ts">
  import type { ToolCallView } from "$lib/types";
  import { resolveToolDisplay } from "$lib/tools/tool-display";
  import { t } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { Shield } from "lucide-svelte";
  import { slide } from "svelte/transition";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    sessionId: string;
    messageId: string;
    toolCall: ToolCallView;
  }

  let { sessionId, messageId, toolCall }: Props = $props();

  const display = $derived(resolveToolDisplay(toolCall));
  const ToolIcon = $derived(display.icon);

  let isProcessing = $state(false);
  let error = $state<string | null>(null);

  async function handleDecision(
    decision: "accept" | "refuse" | "always_accept",
  ): Promise<void> {
    isProcessing = true;
    error = null;
    try {
      await invoke("authorize_chat_tool", {
        sessionId,
        messageId,
        toolName: toolCall.tool_name,
        decision,
      });
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      isProcessing = false;
    }
  }
</script>

<div
  class="my-1.5 rounded-lg glass-surface glass-border-subtle border-l-2 border-l-warning px-3 py-2.5 text-xs"
  data-testid="operator-approval-{toolCall.tool_name}"
  transition:slide={{ duration: 200 }}
>
  <!-- Title row: shield icon + generic action label -->
  <div class="flex items-center gap-2 font-medium text-foreground">
    <div class="flex h-6 w-6 items-center justify-center rounded-lg bg-warning/10">
      <Shield class="h-3.5 w-3.5 text-warning" />
    </div>
    <span class="text-sm">{$t("chat.authorize_action")}</span>
  </div>

  <!-- Human-readable action description with tool icon -->
  <div class="mt-3 flex items-center gap-2">
    <ToolIcon class="h-5 w-5 flex-shrink-0 text-muted-foreground" />
    <span class="text-[12px] text-foreground">
      {$t(display.descriptionKey, { values: display.templateParams })}
    </span>
  </div>

  {#if error}
    <p class="mt-1.5 text-[10px] text-destructive">{error}</p>
  {/if}

  <!-- Decision buttons -->
  <div class="mt-4 flex gap-2">
    <Button
      variant="outline"
      size="sm"
      class="h-7 px-3 text-[11px]"
      disabled={isProcessing}
      onclick={() => handleDecision("accept")}
      data-testid="operator-approval-accept-{toolCall.tool_name}"
    >
      {$t("chat.approve_accept")}
    </Button>
    <Button
      variant="ghost"
      size="sm"
      class="h-7 px-3 text-[11px] text-destructive hover:bg-destructive/10 hover:text-destructive"
      disabled={isProcessing}
      onclick={() => handleDecision("refuse")}
      data-testid="operator-approval-refuse-{toolCall.tool_name}"
    >
      {$t("chat.approve_refuse")}
    </Button>
    <Button
      variant="ghost"
      size="sm"
      class="h-7 px-3 text-[11px] text-primary"
      disabled={isProcessing}
      onclick={() => handleDecision("always_accept")}
      data-testid="operator-approval-always-{toolCall.tool_name}"
    >
      {$t("chat.approve_always")}
    </Button>
  </div>
</div>
