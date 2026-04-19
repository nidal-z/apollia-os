<script lang="ts">
  import { t } from "svelte-i18n";
  import { MessageSquare, Archive, Pin, XCircle, ArrowUpDown } from "lucide-svelte";
  import {
    sessionFilter,
    sessionSort,
    type SessionFilter,
    type SessionSort,
  } from "$lib/stores/chatSessions";

  const FILTERS: Array<{ id: SessionFilter; labelKey: string; icon: typeof MessageSquare }> = [
    { id: "active", labelKey: "chat.filter_active", icon: MessageSquare },
    { id: "pinned", labelKey: "chat.filter_pinned", icon: Pin },
    { id: "closed", labelKey: "chat.filter_closed", icon: XCircle },
    { id: "archived", labelKey: "chat.filter_archived", icon: Archive },
  ];

  const SORTS: Array<{ id: SessionSort; labelKey: string }> = [
    { id: "recent", labelKey: "chat.sort_recent" },
    { id: "oldest", labelKey: "chat.sort_oldest" },
    { id: "alphabetical", labelKey: "chat.sort_alphabetical" },
    { id: "most-tokens", labelKey: "chat.sort_most_tokens" },
  ];
</script>

<div class="flex flex-col gap-1.5" data-testid="session-filters">
  <!-- Segmented filter -->
  <div
    role="tablist"
    class="inline-flex items-center gap-0.5 rounded-md bg-muted/30 p-0.5"
    data-testid="session-filter-tabs"
  >
    {#each FILTERS as f (f.id)}
      {@const active = $sessionFilter === f.id}
      {@const Icon = f.icon}
      <button
        role="tab"
        aria-selected={active}
        class="flex-1 inline-flex items-center justify-center gap-1 rounded px-1.5 py-1
          text-[10px] font-medium transition-all
          {active
            ? 'bg-background text-foreground shadow-sm ring-1 ring-border/40'
            : 'text-muted-foreground/60 hover:text-foreground hover:bg-muted/40'}"
        onclick={() => sessionFilter.set(f.id)}
        data-testid="session-filter-{f.id}"
      >
        <Icon size={10} />
        <span class="truncate">{$t(f.labelKey)}</span>
      </button>
    {/each}
  </div>

  <!-- Sort dropdown -->
  <label class="flex items-center gap-1 text-[10px] text-muted-foreground/60">
    <ArrowUpDown size={10} class="shrink-0" />
    <select
      class="h-5 flex-1 min-w-0 rounded border border-border/30 bg-transparent px-1 text-[10px]
        focus:outline-none focus:ring-1 focus:ring-primary/40"
      value={$sessionSort}
      onchange={(e) => sessionSort.set(e.currentTarget.value as SessionSort)}
      data-testid="session-sort-select"
    >
      {#each SORTS as s (s.id)}
        <option value={s.id}>{$t(s.labelKey)}</option>
      {/each}
    </select>
  </label>
</div>
