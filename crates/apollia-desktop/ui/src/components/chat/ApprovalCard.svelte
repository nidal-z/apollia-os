<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { slide } from "svelte/transition";
  import { ShieldAlert } from "lucide-svelte";

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
  class="my-1.5 rounded-md border border-[hsl(var(--warning))]/50 bg-[hsl(var(--warning))]/5 px-3 py-2.5 text-xs"
  data-testid="approval-card-{toolName}"
  transition:slide={{ duration: 200 }}
>
  <div class="flex items-center gap-1.5 font-medium text-foreground">
    <ShieldAlert class="h-3.5 w-3.5 text-[hsl(var(--warning))]" />
    <span>Autoriser l'exécution de <strong>{toolName}</strong> ?</span>
  </div>

  <pre class="mt-1.5 whitespace-pre-wrap break-all rounded bg-muted/50 p-1.5 font-mono text-[10px] text-muted-foreground">{inputPreview}</pre>

  {#if error}
    <p class="mt-1.5 text-[10px] text-[hsl(var(--destructive))]">{error}</p>
  {/if}

  <div class="mt-2 flex gap-2">
    <button
      class="rounded px-2.5 py-1 text-[11px] font-medium text-white transition-colors
        bg-[hsl(var(--success))] hover:bg-[hsl(var(--success))]/80
        disabled:cursor-not-allowed disabled:opacity-50"
      disabled={isProcessing}
      onclick={() => handleDecision("accept")}
      data-testid="approval-accept-{toolName}"
    >
      Accepter
    </button>
    <button
      class="rounded px-2.5 py-1 text-[11px] font-medium text-white transition-colors
        bg-[hsl(var(--destructive))] hover:bg-[hsl(var(--destructive))]/80
        disabled:cursor-not-allowed disabled:opacity-50"
      disabled={isProcessing}
      onclick={() => handleDecision("refuse")}
      data-testid="approval-refuse-{toolName}"
    >
      Refuser
    </button>
    <button
      class="rounded px-2.5 py-1 text-[11px] font-medium text-white transition-colors
        bg-[hsl(var(--primary))] hover:bg-[hsl(var(--primary))]/80
        disabled:cursor-not-allowed disabled:opacity-50"
      disabled={isProcessing}
      onclick={() => handleDecision("always_accept")}
      data-testid="approval-always-{toolName}"
    >
      Toujours accepter
    </button>
  </div>
</div>
