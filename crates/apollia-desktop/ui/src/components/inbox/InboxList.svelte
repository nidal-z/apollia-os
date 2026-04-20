<script lang="ts">
  import { t } from "svelte-i18n";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import InboxItem from "./InboxItem.svelte";
  import type { InboxItem as InboxItemType } from "./types";

  interface Props {
    items: InboxItemType[];
    selectedIds: Set<string>;
    activeId: string | null;
    urgencyThresholdMs?: number;
    onselect: (id: string) => void;
    ontoggle: (id: string) => void;
    onselectAll: () => void;
    onclearSelection: () => void;
  }

  let {
    items,
    selectedIds,
    activeId,
    urgencyThresholdMs,
    onselect,
    ontoggle,
    onselectAll,
    onclearSelection,
  }: Props = $props();

  const allSelected = $derived(items.length > 0 && items.every((i) => selectedIds.has(i.id)));

  function handleSelectAll(checked: boolean) {
    if (checked) onselectAll();
    else onclearSelection();
  }
</script>

<div class="flex h-full flex-col" data-testid="inbox-list">
  <div class="flex items-center justify-between border-b border-border px-3 py-2">
    <label class="flex items-center gap-2 text-[11px] text-muted-foreground">
      <Checkbox
        checked={allSelected}
        onchange={handleSelectAll}
        data-testid="inbox-select-all"
      />
      {$t("inbox.select_all_visible")}
    </label>
    <span class="text-[11px] text-muted-foreground">
      {$t("inbox.item_count", { values: { count: items.length } })}
    </span>
  </div>

  <div
    role="list"
    class="flex flex-1 flex-col gap-1 overflow-y-auto p-2"
    aria-label={$t("inbox.list_aria")}
  >
    {#each items as item (item.id)}
      <InboxItem
        {item}
        {urgencyThresholdMs}
        selected={selectedIds.has(item.id)}
        active={activeId === item.id}
        onselect={() => onselect(item.id)}
        ontoggle={() => ontoggle(item.id)}
      />
    {/each}
  </div>
</div>
