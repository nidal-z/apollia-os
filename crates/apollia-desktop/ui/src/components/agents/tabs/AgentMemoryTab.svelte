<script lang="ts">
  /**
   * AgentMemoryTab - the agent's persistent memory namespace.
   *
   * The namespace comes from the Python manifest, not from `agent.name`:
   * several agents in a package often share one namespace, so we never fall
   * back to the name when fetching entries. Loads on demand when the tab is
   * shown and surfaces load failures with a humanized message.
   */
  import { t } from "svelte-i18n";
  import { Clock, ExternalLink, Sparkles, Zap } from "lucide-svelte";
  import { Card } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { listMemoryEntries } from "$lib/ipc/agents";
  import { reportError } from "$lib/errors/reportError";
  import type { AgentListItem, MemoryEntry } from "$lib/types";

  interface Props {
    agent: AgentListItem;
    onNavigateMemory: () => void;
  }

  let { agent, onNavigateMemory }: Props = $props();

  let entries = $state<MemoryEntry[]>([]);
  let loading = $state(false);
  let errorMessage = $state<string | null>(null);

  const namespace = $derived(agent.memory_namespace);

  $effect(() => {
    const ns = namespace;
    if (!ns) {
      entries = [];
      loading = false;
      errorMessage = null;
      return;
    }
    void load(ns);
  });

  async function load(ns: string): Promise<void> {
    loading = true;
    errorMessage = null;
    try {
      entries = await listMemoryEntries(ns);
    } catch (err) {
      errorMessage = reportError(err, { surface: "inline" }).friendly_message;
      entries = [];
    } finally {
      loading = false;
    }
  }

  const shown = $derived(entries.slice(0, 12));

  function relative(iso: string): string {
    const diffMs = Date.now() - new Date(iso).getTime();
    if (diffMs < 0) return $t("agents.memory_relative_future");
    const s = Math.floor(diffMs / 1000);
    if (s < 60) return `${s}s`;
    const m = Math.floor(s / 60);
    if (m < 60) return `${m}min`;
    const h = Math.floor(m / 60);
    if (h < 24) return `${h}h`;
    const d = Math.floor(h / 24);
    if (d < 7) return `${d}j`;
    return `${Math.floor(d / 7)}sem`;
  }
</script>

<div class="max-w-3xl space-y-4">
  {#if !namespace}
    <Card class="p-3.5">
      <p class="text-body-xs italic text-muted-foreground">
        {$t("agents.memory_none_declared")}
      </p>
    </Card>
  {:else}
    <div class="flex items-center gap-3 text-caption text-muted-foreground">
      <div class="inline-flex items-center gap-1.5">
        <span class="font-mono text-foreground">{namespace}</span>
        <span class="text-muted-foreground/60">·</span>
        <span class="tabular-nums">
          {entries.length > 1
            ? $t("agents.memory_entries_plural", { values: { count: entries.length } })
            : $t("agents.memory_entries_singular", { values: { count: entries.length } })}
        </span>
      </div>
      {#if agent.shared_memory_namespaces.length > 0}
        <span class="text-muted-foreground/60">·</span>
        <span class="text-caption">
          {agent.shared_memory_namespaces.length > 1
            ? $t("agents.memory_shared_namespaces_plural", {
                values: { count: agent.shared_memory_namespaces.length },
              })
            : $t("agents.memory_shared_namespaces_singular", {
                values: { count: agent.shared_memory_namespaces.length },
              })}
        </span>
      {/if}
    </div>

    <Card class="overflow-hidden">
      {#if loading}
        <div class="px-4 py-6 text-center text-body-xs text-muted-foreground">
          {$t("agents.memory_loading")}
        </div>
      {:else if errorMessage}
        <div class="px-4 py-3 text-body-xs text-destructive" data-testid="agent-memory-error">
          {errorMessage}
        </div>
      {:else if entries.length === 0}
        <div class="px-4 py-8 text-center">
          <p class="mb-1 text-body-xs text-muted-foreground">
            {$t("agents.memory_empty_namespace")}
          </p>
          <p class="text-caption text-muted-foreground/70">
            {$t("agents.memory_empty_hint")}
          </p>
        </div>
      {:else}
        {#each shown as entry, i (entry.id)}
          <div
            class="flex items-start gap-3 px-4 py-2.5 {i === shown.length - 1
              ? ''
              : 'border-b border-border/40'}"
            data-testid="agent-memory-row"
          >
            <span
              class="mt-0.5 inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground"
            >
              {#if entry.entry_type === "episodic"}
                <Clock size={11} />
              {:else if entry.entry_type === "procedural"}
                <Zap size={11} />
              {:else}
                <Sparkles size={11} />
              {/if}
            </span>
            <div class="min-w-0 flex-1">
              <div class="flex min-w-0 items-center gap-2">
                <code class="truncate text-body-xs text-foreground" title={entry.key}>
                  {entry.key}
                </code>
                <Badge size="sm" variant="neutral">
                  {entry.entry_type.slice(0, 4).toLowerCase()}
                </Badge>
              </div>
              {#if entry.value}
                <div class="mt-1 truncate text-caption text-muted-foreground" title={entry.value}>
                  {entry.value.length > 140 ? entry.value.slice(0, 140) + "…" : entry.value}
                </div>
              {/if}
              <div class="mt-0.5 text-caption tabular-nums text-muted-foreground/70">
                {$t("agents.memory_time_ago", { values: { time: relative(entry.created_at) } })}
              </div>
            </div>
          </div>
        {/each}
        {#if entries.length > 12}
          <div class="border-t border-border/40 px-4 py-2 text-center text-caption text-muted-foreground/70">
            {entries.length - 12 > 1
              ? $t("agents.memory_more_entries_plural", { values: { count: entries.length - 12 } })
              : $t("agents.memory_more_entries_singular", { values: { count: entries.length - 12 } })}
            -
            <Button
              variant="ghost"
              size="auto"
              onclick={onNavigateMemory}
              class="inline p-0 text-caption text-primary hover:underline"
            >
              {$t("agents.memory_view_all")}
            </Button>
          </div>
        {/if}
      {/if}
    </Card>

    <Button variant="outline" size="sm" onclick={onNavigateMemory} class="gap-2">
      {#snippet icon()}<ExternalLink size={13} />{/snippet}
      {$t("agent_detail.memory_link", { values: { namespace } })}
    </Button>
  {/if}
</div>
