<script lang="ts">
  import { t } from "svelte-i18n";
  import { navigateTo } from "$lib/stores/navigation";
  import { ShieldCheck } from "lucide-svelte";
  import InboxItem from "../inbox/InboxItem.svelte";
  import type { InboxItem as InboxItemT } from "../inbox/types";

  interface Props {
    items: InboxItemT[];
    totalPending: number;
    limit?: number;
  }

  let { items, totalPending, limit = 5 }: Props = $props();

  const visible = $derived(items.slice(0, limit));
  const hasMore = $derived(totalPending > visible.length);

  function goToInbox() {
    navigateTo("inbox");
  }
</script>

<section
  class="glass-card glass-border rounded-lg p-4"
  data-testid="pending-actions-block"
>
  <header class="mb-3 flex items-center justify-between">
    <h2 class="text-sm font-medium uppercase tracking-wider text-muted-foreground">
      {$t("dashboard.pending_actions_title")}
    </h2>
    <span class="text-[11px] text-muted-foreground/70">{totalPending}</span>
  </header>

  {#if visible.length === 0}
    <div class="flex flex-col items-center gap-2 py-6 text-center">
      <ShieldCheck size={20} class="text-success/70" />
      <p class="text-xs text-muted-foreground">{$t("dashboard.pending_actions_empty")}</p>
    </div>
  {:else}
    <div class="flex flex-col gap-2">
      {#each visible as item (item.id)}
        <InboxItem
          {item}
          selected={false}
          active={false}
          onselect={goToInbox}
          ontoggle={goToInbox}
        />
      {/each}
    </div>
    {#if hasMore}
      <button
        class="mt-3 w-full rounded-md border border-border/60 bg-card px-3 py-1.5 text-xs text-primary hover:bg-muted/40"
        onclick={goToInbox}
        data-testid="pending-actions-see-all"
      >
        {$t("dashboard.pending_actions_see_all", { values: { n: totalPending } })}
      </button>
    {/if}
  {/if}
</section>
