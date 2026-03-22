<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { slide } from "svelte/transition";
  import { ShieldAlert } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    sessionId: string;
    messageId: string;
    toolName: string;
    inputPreview: string;
  }

  let { sessionId, messageId, toolName, inputPreview }: Props = $props();

  let isProcessing = $state(false);
  let error = $state<string | null>(null);

  async function handleDecision(decision: "accept" | "refuse" | "always_accept"): Promise<void> {
    isProcessing = true;
    error = null;
    try {
      await invoke("authorize_chat_tool", {
        sessionId,
        messageId,
        toolName,
        decision,
      });
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      isProcessing = false;
    }
  }
</script>

<div
  class="my-1.5 glass-card glass-border rounded-lg border-l-2 border-l-warning px-3 py-2.5 text-xs"
  data-testid="approval-card-{toolName}"
  transition:slide={{ duration: 200 }}
>
  <div class="flex items-center gap-2 font-medium text-foreground">
    <div class="flex h-6 w-6 items-center justify-center rounded-lg bg-warning/10">
      <ShieldAlert class="h-3.5 w-3.5 text-warning" />
    </div>
    <span>{$t("chat.authorize_tool")} <strong>{toolName}</strong> ?</span>
  </div>

  <pre class="mt-2 whitespace-pre-wrap break-all rounded-lg glass-surface p-2 font-mono text-[10px] text-muted-foreground">{inputPreview}</pre>

  {#if error}
    <p class="mt-1.5 text-[10px] text-destructive">{error}</p>
  {/if}

  <div class="mt-2.5 flex gap-2">
    <Button
      variant="outline"
      size="sm"
      class="text-[11px] h-7 px-3"
      disabled={isProcessing}
      onclick={() => handleDecision("accept")}
      data-testid="approval-accept-{toolName}"
    >
      {$t("chat.approve_accept")}
    </Button>
    <Button
      variant="ghost"
      size="sm"
      class="text-[11px] h-7 px-3 text-destructive hover:text-destructive hover:bg-destructive/10"
      disabled={isProcessing}
      onclick={() => handleDecision("refuse")}
      data-testid="approval-refuse-{toolName}"
    >
      {$t("chat.approve_refuse")}
    </Button>
    <Button
      variant="ghost"
      size="sm"
      class="text-[11px] h-7 px-3 text-primary"
      disabled={isProcessing}
      onclick={() => handleDecision("always_accept")}
      data-testid="approval-always-{toolName}"
    >
      {$t("chat.approve_always")}
    </Button>
  </div>
</div>
