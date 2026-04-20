<script lang="ts">
  import { t } from "svelte-i18n";
  import { Search } from "lucide-svelte";
  import type { InboxItemKind } from "./types";

  export type StatusFilter = "pending" | "resolved" | "all";

  interface Props {
    search: string;
    kinds: Set<InboxItemKind>;
    status: StatusFilter;
    agents: string[];
    selectedAgent: string | null;
    onsearch: (v: string) => void;
    ontogglekind: (k: InboxItemKind) => void;
    onstatuschange: (s: StatusFilter) => void;
    onagentchange: (a: string | null) => void;
  }

  let {
    search,
    kinds,
    status,
    agents,
    selectedAgent,
    onsearch,
    ontogglekind,
    onstatuschange,
    onagentchange,
  }: Props = $props();

  const KIND_OPTIONS: { value: InboxItemKind; labelKey: string }[] = [
    { value: "task", labelKey: "inbox.kind.task" },
    { value: "tool", labelKey: "inbox.kind.tool" },
    { value: "bash", labelKey: "inbox.kind.bash" },
    { value: "filesystem", labelKey: "inbox.kind.filesystem" },
    { value: "always_accept", labelKey: "inbox.kind.always_accept" },
  ];
</script>

<div class="flex flex-wrap items-center gap-2" data-testid="inbox-filters">
  <label class="relative flex-1 min-w-[220px]">
    <Search size={14} class="absolute left-2 top-1/2 -translate-y-1/2 text-muted-foreground" />
    <input
      type="search"
      value={search}
      oninput={(e) => onsearch(e.currentTarget.value)}
      placeholder={$t("inbox.search_placeholder")}
      aria-label={$t("inbox.search_placeholder")}
      class="h-8 w-full rounded-md border border-border bg-background pl-7 pr-2 text-[12px] focus:outline-none focus:ring-2 focus:ring-ring/40"
      data-testid="inbox-search"
    />
  </label>

  <select
    value={status}
    onchange={(e) => onstatuschange(e.currentTarget.value as StatusFilter)}
    class="h-8 rounded-md border border-border bg-background px-2 text-[12px]"
    aria-label={$t("inbox.status_label")}
    data-testid="inbox-status-filter"
  >
    <option value="pending">{$t("inbox.status.pending")}</option>
    <option value="resolved">{$t("inbox.status.resolved")}</option>
    <option value="all">{$t("inbox.status.all")}</option>
  </select>

  <select
    value={selectedAgent ?? ""}
    onchange={(e) => onagentchange(e.currentTarget.value || null)}
    class="h-8 rounded-md border border-border bg-background px-2 text-[12px]"
    aria-label={$t("inbox.agent_label")}
    data-testid="inbox-agent-filter"
  >
    <option value="">{$t("inbox.agent_all")}</option>
    {#each agents as agent}
      <option value={agent}>{agent}</option>
    {/each}
  </select>

  <div class="flex flex-wrap items-center gap-1">
    {#each KIND_OPTIONS as opt}
      <button
        type="button"
        onclick={() => ontogglekind(opt.value)}
        class="rounded-full px-2.5 py-1 text-[11px] transition-colors {kinds.has(opt.value)
          ? 'bg-primary text-primary-foreground'
          : 'bg-muted text-muted-foreground hover:bg-muted/80'}"
        aria-pressed={kinds.has(opt.value)}
        data-testid="inbox-kind-chip-{opt.value}"
      >
        {$t(opt.labelKey)}
      </button>
    {/each}
  </div>
</div>
