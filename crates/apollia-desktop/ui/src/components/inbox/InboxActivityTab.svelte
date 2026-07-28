<script lang="ts">
  /**
   * "Activity" tab: runtime events (failures, degradations, LLM down) with a
   * filter chip row. Loading shows a skeleton, failure a humanized error box
   * (never a swallowed empty list), and the empty state points to Observability.
   */
  import { t } from "svelte-i18n";
  import { Activity as ActivityIcon } from "lucide-svelte";
  import { FilterChipBar, EmptyState, SkeletonList } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { navigateTo } from "$lib/stores/navigation";
  import ActivityRow from "./ActivityRow.svelte";
  import InboxErrorBox from "./InboxErrorBox.svelte";
  import type { ActivityFilter } from "./inboxModel";
  import type { NotificationLogEntry } from "$lib/types";

  interface Props {
    entries: NotificationLogEntry[];
    loading: boolean;
    error: unknown;
    filter: ActivityFilter;
    onFilterChange: (f: ActivityFilter) => void;
    onRetry: () => void;
  }

  let { entries, loading, error, filter, onFilterChange, onRetry }: Props = $props();

  const GROUPS: Record<ActivityFilter, string[]> = {
    all: [],
    failures: ["task.failed", "trigger.error"],
    degradations: ["agent.degraded"],
    llm: ["llm.backend_down"],
  };

  const filtered = $derived.by(() => {
    if (filter === "all") return entries;
    const wanted = new Set(GROUPS[filter]);
    return entries.filter((e) => wanted.has(e.event_name));
  });
</script>

<FilterChipBar
  chips={[
    { key: "all", label: $t("inbox.activity.filter.all") },
    { key: "failures", label: $t("inbox.activity.filter.failures") },
    { key: "degradations", label: $t("inbox.activity.filter.degradations") },
    { key: "llm", label: $t("inbox.activity.filter.llm") },
  ]}
  activeKey={filter}
  onchange={(key) => onFilterChange(key as ActivityFilter)}
  testidPrefix="inbox-activity-filter"
/>

<div class="min-h-0 flex-1 overflow-y-auto px-8 pb-10">
  {#if loading}
    <SkeletonList count={4} class="pt-3" />
  {:else if error}
    <div class="pt-3">
      <InboxErrorBox {error} {onRetry} testid="inbox-activity-error" />
    </div>
  {:else if filtered.length === 0}
    <div class="py-10">
      <EmptyState title={$t("inbox.activity.empty")} desc={$t("inbox.activity.empty_desc")} tone="success">
        {#snippet icon()}<ActivityIcon size={22} />{/snippet}
        {#snippet action()}
          <Button
            variant="ghost"
            size="sm"
            type="button"
            class="text-primary underline-offset-2 hover:underline"
            onclick={() => navigateTo("observability")}
            data-testid="inbox-activity-empty-cta"
          >
            {$t("inbox.activity.empty_cta")}
          </Button>
        {/snippet}
      </EmptyState>
    </div>
  {:else}
    <div class="flex flex-col gap-2 pt-3">
      {#each filtered as entry (entry.id)}
        <ActivityRow {entry} />
      {/each}
    </div>
  {/if}
</div>
