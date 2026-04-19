<script lang="ts">
  import { Brain } from "lucide-svelte";
  import { chatConversationStats, memoryEntryCount } from "$lib/stores/chat";
  import SummarizedMessagesBanner from "./SummarizedMessagesBanner.svelte";

  interface Props {
    sessionId: string;
  }

  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  let { sessionId: _sessionId }: Props = $props();

  // Stats come from the session header via chatConversationStats.
  // Memory entries are injected on demand by the agent (Principle 6 — no
  // automatic injection), so this panel reflects what the agent pulled in for
  // the current exchange. Full recall history lives in the memory tab of the
  // workspace; this drawer only shows session-local injection activity.

  const stats = $derived($chatConversationStats);
  const injectedCount = $derived($memoryEntryCount);
</script>

<div class="flex flex-col gap-3 px-4 py-4" data-testid="memory-injected-panel">
  <section class="glass-card glass-border rounded-lg p-3">
    <header class="mb-2 flex items-center gap-2 text-[11px] font-medium text-primary">
      <Brain size={12} />
      <span>Mémoire injectée par l'agent</span>
    </header>

    {#if injectedCount === 0}
      <p class="text-[11px] italic text-muted-foreground/60">
        L'agent n'a encore injecté aucune mémoire pour cette conversation.
      </p>
      <p class="mt-2 text-[10px] text-muted-foreground/50">
        Principe&nbsp;6 — la mémoire n'est jamais injectée automatiquement.
        L'agent appelle <code class="rounded bg-muted/40 px-1">recall_entry()</code>
        ou <code class="rounded bg-muted/40 px-1">recall_all()</code> quand
        c'est utile.
      </p>
    {:else}
      <div class="flex items-baseline gap-2">
        <span class="text-2xl font-semibold tabular-nums text-primary">
          {injectedCount}
        </span>
        <span class="text-[11px] text-muted-foreground">
          entrée{injectedCount > 1 ? "s" : ""} de mémoire
        </span>
      </div>
      <p class="mt-1 text-[10px] text-muted-foreground/60">
        Injectées manuellement par l'agent sur ce tour.
      </p>
    {/if}
  </section>

  {#if stats && stats.summarized_count > 0}
    <SummarizedMessagesBanner
      summarizedCount={stats.summarized_count}
      summaryText="Les anciens messages de cette conversation ont été résumés pour économiser le contexte."
    />
  {/if}

  {#if stats && stats.cross_sessions_referenced > 0}
    <section class="glass-card glass-border rounded-lg p-3">
      <header class="mb-1 text-[11px] font-medium text-primary">
        Cross-session
      </header>
      <p class="text-[11px] text-muted-foreground">
        {stats.cross_sessions_referenced} conversation{stats.cross_sessions_referenced > 1 ? "s" : ""}
        passée{stats.cross_sessions_referenced > 1 ? "s" : ""} référencée{stats.cross_sessions_referenced > 1 ? "s" : ""}.
      </p>
    </section>
  {/if}
</div>
