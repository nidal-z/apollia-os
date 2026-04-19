<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { fly } from "svelte/transition";
  import type { AgentMessage } from "$lib/types";
  import { lastAgentMessage } from "$lib/stores/sse";
  import { ArrowRight, ArrowLeft, MessageSquare } from "lucide-svelte";
  import { Skeleton } from "$lib/components/ui/skeleton";

  interface Props {
    agentName: string;
  }

  let { agentName }: Props = $props();

  let messages = $state<AgentMessage[]>([]);
  let loading = $state(true);
  let expandedIndex = $state<number | null>(null);

  function relativeTime(isoDate: string): string {
    const diffMs = Date.now() - new Date(isoDate).getTime();
    const diffSecs = Math.floor(diffMs / 1000);
    if (diffSecs < 60) return `${diffSecs}s`;
    const diffMins = Math.floor(diffSecs / 60);
    if (diffMins < 60) return `${diffMins}m`;
    const diffHours = Math.floor(diffMins / 60);
    if (diffHours < 24) return `${diffHours}h`;
    const diffDays = Math.floor(diffHours / 24);
    return `${diffDays}d`;
  }

  function truncatePayload(payload: unknown, maxLength: number = 50): string {
    const str = JSON.stringify(payload);
    return str.length > maxLength ? str.slice(0, maxLength) + "..." : str;
  }

  function isOutgoing(msg: AgentMessage): boolean {
    return msg.from_agent === agentName;
  }

  function formatPayload(payload: unknown): string {
    return JSON.stringify(payload, null, 2);
  }

  function toggleExpand(index: number): void {
    expandedIndex = expandedIndex === index ? null : index;
  }

  async function fetchMessages(): Promise<void> {
    loading = true;
    try {
      const result: AgentMessage[] = await invoke("list_agent_messages", {
        agentName,
        limit: 50,
      });
      messages = result;
    } catch {
      messages = [];
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (agentName) {
      void fetchMessages();
    }
  });

  $effect(() => {
    const msg = $lastAgentMessage;
    if (
      msg &&
      (msg.from_agent === agentName || msg.to_agent === agentName)
    ) {
      messages = [msg, ...messages];
    }
  });
</script>

<section data-testid="agent-messages-panel">
  {#if loading}
    <div class="space-y-2">
      {#each Array(3) as _}
        <Skeleton variant="card" class="h-16 w-full" />
      {/each}
    </div>
  {:else if messages.length === 0}
    <div class="flex flex-col items-center justify-center py-8 text-center" data-testid="agent-messages-empty">
      <MessageSquare size={48} class="text-muted-foreground/50 mb-3" />
      <p class="text-sm font-medium text-foreground/70">{$t('agent_detail.messages_empty_title')}</p>
      <p class="mt-1 text-xs text-muted-foreground">{$t('agent_detail.messages_empty_desc')}</p>
    </div>
  {:else}
    <div class="space-y-2 max-h-[400px] overflow-y-auto">
      {#each messages as msg, i (msg.sent_at + msg.from_agent)}
        <button
          class="w-full text-left bg-white/5 backdrop-blur-sm border border-white/10 rounded-lg p-3 cursor-pointer hover:bg-white/[0.08] transition-colors"
          onclick={() => toggleExpand(i)}
          data-testid="agent-message-item"
          transition:fly={{ y: -10, duration: 200 }}
        >
          <div class="flex items-center gap-2">
            {#if isOutgoing(msg)}
              <ArrowRight size={14} class="shrink-0 text-primary" />
              <span class="text-sm font-medium text-foreground truncate">
                {$t('agent_detail.message_sent_to', { values: { agent: msg.to_agent } })}
              </span>
            {:else}
              <ArrowLeft size={14} class="shrink-0 text-secondary" />
              <span class="text-sm font-medium text-foreground truncate">
                {$t('agent_detail.message_received_from', { values: { agent: msg.from_agent } })}
              </span>
            {/if}
            <span class="ml-auto shrink-0 text-xs text-muted-foreground">{relativeTime(msg.sent_at)}</span>
          </div>
          <p class="mt-1 text-xs text-muted-foreground font-mono truncate">{truncatePayload(msg.payload)}</p>

          {#if expandedIndex === i}
            <pre
              class="mt-2 bg-black/20 rounded-lg p-3 text-xs font-mono text-foreground/80 whitespace-pre-wrap overflow-x-auto"
              data-testid="agent-message-payload"
            >{formatPayload(msg.payload)}</pre>
          {/if}
        </button>
      {/each}
    </div>
  {/if}
</section>
