<script lang="ts">
  /**
   * "Pending" tab: filter chips + agent select, then the grouped list (or the
   * loading skeleton / humanized error / celebratory inbox-zero state) and the
   * recent-history section. Owns only the local filter state; every effect is
   * delegated to the route via callbacks.
   */
  import { t } from "svelte-i18n";
  import { Inbox as InboxIcon } from "lucide-svelte";
  import { EmptyState, FilterChipBar, SkeletonList } from "$lib/components/operator";
  import type { AlwaysScope } from "$lib/components/operator/HITLCard.svelte";
  import { Select } from "$lib/components/ui/select";
  import InboxPendingList from "./InboxPendingList.svelte";
  import InboxHistoryList from "./InboxHistoryList.svelte";
  import InboxErrorBox from "./InboxErrorBox.svelte";
  import { rowFilterKey, groupOf, type FilterKey, type GroupKey } from "./inboxModel";
  import type { HistoryEntry } from "./historyModel";
  import type { InboxItem } from "./types";
  import type { AskUserAnswer } from "$lib/types";

  interface Props {
    items: InboxItem[];
    loading: boolean;
    error: unknown;
    partialError: unknown;
    history: HistoryEntry[];
    historyError: unknown;
    /** One history origin failed while the other answered. */
    historyPartialError: unknown;
    submitting: boolean;
    expandedId: string | null;
    relTime: (iso: string) => string;
    onToggleExpand: (item: InboxItem) => void;
    onApprove: (item: InboxItem) => void;
    onReject: (item: InboxItem) => void;
    onAlwaysAccept: (item: InboxItem, scope: AlwaysScope) => void;
    onRespondAskUser: (item: InboxItem, answers: AskUserAnswer[]) => void;
    onRejectAskUser: (item: InboxItem, reason: string) => void;
    onOpenChat: (sessionId: string) => void;
    onCopyId: (item: InboxItem) => void;
    onRetry: () => void;
    onRetryHistory: () => void;
  }

  let {
    items,
    loading,
    error,
    partialError,
    history,
    historyError,
    historyPartialError,
    submitting,
    expandedId,
    relTime,
    onToggleExpand,
    onApprove,
    onReject,
    onAlwaysAccept,
    onRespondAskUser,
    onRejectAskUser,
    onOpenChat,
    onCopyId,
    onRetry,
    onRetryHistory,
  }: Props = $props();

  let activeFilter = $state<FilterKey>("all");
  let agentFilter = $state<string>("all");

  const agentOptions = $derived.by(() => {
    const set = new Map<string, string>();
    for (const i of items) set.set(i.agentName, i.agentName);
    return [...set.values()].sort((a, b) => a.localeCompare(b));
  });

  const scoped = $derived(agentFilter === "all" ? items : items.filter((i) => i.agentName === agentFilter));

  const counts = $derived.by(() => {
    const c: Record<FilterKey, number> = { all: scoped.length, approval: 0, ask_user: 0 };
    for (const i of scoped) c[rowFilterKey(i)] += 1;
    return c;
  });

  const filteredItems = $derived(
    activeFilter === "all" ? scoped : scoped.filter((i) => rowFilterKey(i) === activeFilter),
  );

  const grouped = $derived.by(() => {
    const g: Record<GroupKey, InboxItem[]> = { today: [], yesterday: [], earlier: [] };
    for (const i of filteredItems) g[groupOf(i.suspendedAt)].push(i);
    return g;
  });
</script>

<FilterChipBar
  chips={[
    { key: "all", label: $t("inbox.chips.all"), count: counts.all },
    { key: "approval", label: $t("inbox.chips.approvals"), count: counts.approval },
    { key: "ask_user", label: $t("inbox.chips.questions"), count: counts.ask_user },
  ]}
  activeKey={activeFilter}
  onchange={(key) => (activeFilter = key as FilterKey)}
  testidPrefix="inbox-filter"
>
  {#snippet rightSlot()}
    {#if agentOptions.length > 0}
      <div class="flex items-center gap-2">
        <label for="inbox-agent-filter" class="text-caption text-muted-foreground">
          {$t("inbox.filter_by_agent")}
        </label>
        <Select id="inbox-agent-filter" class="h-8 w-auto text-body-xs" bind:value={agentFilter} data-testid="inbox-agent-filter">
          <option value="all">{$t("inbox.all_agents")}</option>
          {#each agentOptions as agent}
            <option value={agent}>{agent}</option>
          {/each}
        </Select>
      </div>
    {/if}
  {/snippet}
</FilterChipBar>

<div class="min-h-0 flex-1 overflow-y-auto">
  {#if loading}
    <div class="px-8 pt-3"><SkeletonList count={4} /></div>
  {:else if error}
    <div class="px-8 py-6"><InboxErrorBox {error} {onRetry} testid="inbox-pending-error" /></div>
  {:else if filteredItems.length === 0}
    <div class="animate-fade-in px-8 py-12">
      <EmptyState title={$t("inbox.empty_title")} desc={$t("inbox.empty_subtitle")} tone="success">
        {#snippet icon()}<InboxIcon size={22} />{/snippet}
      </EmptyState>
    </div>
  {:else}
    {#if partialError}
      <div class="px-8 pt-3">
        <InboxErrorBox error={partialError} {onRetry} testid="inbox-pending-partial-error" />
      </div>
    {/if}
    <InboxPendingList
      {grouped}
      {expandedId}
      {submitting}
      {relTime}
      {onToggleExpand}
      {onApprove}
      {onReject}
      {onAlwaysAccept}
      {onRespondAskUser}
      {onRejectAskUser}
      {onOpenChat}
      {onCopyId}
    />
    {#if history.length > 0 || historyError || historyPartialError}
      <InboxHistoryList
        {history}
        error={historyError}
        partialError={historyPartialError}
        {relTime}
        onRetry={onRetryHistory}
      />
    {/if}
  {/if}
</div>
