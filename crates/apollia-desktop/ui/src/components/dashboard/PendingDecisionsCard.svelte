<script lang="ts">
  /**
   * PendingDecisionsCard - the dashboard focal card.
   *
   * Carries the signature gradient rim + primary halo only while decisions
   * are actually waiting (the single focal point of the page). Reuses the
   * InboxRow primitive and wraps each row in a right-click ContextMenu.
   */
  import { fly } from "svelte/transition";
  import { t } from "svelte-i18n";
  import { FileCheck, Inbox } from "lucide-svelte";
  import { InboxRow, EmptyState } from "$lib/components/operator";
  import { ContextMenu, type ActionMenuItem } from "$lib/components/ui/context-menu";
  import { rowIn } from "$lib/design/listMotion";
  import DashboardCardHeader from "./DashboardCardHeader.svelte";
  import type { PendingRow } from "./dashboardData";

  interface Props {
    rows: PendingRow[];
    total: number;
    limit: number;
    onSeeAll: () => void;
    onOpenInbox: () => void;
    emptyTitle: string;
    emptyDesc: string;
    moreLabel: string;
  }

  let {
    rows,
    total,
    limit,
    onSeeAll,
    onOpenInbox,
    emptyTitle,
    emptyDesc,
    moreLabel,
  }: Props = $props();

  const visible = $derived(rows.slice(0, limit));
  const overflow = $derived(rows.length - limit);
  const isFocal = $derived(total > 0);

  function rowMenu(): ActionMenuItem[] {
    return [
      {
        id: "open-inbox",
        label: $t("dashboard.ctx_open_inbox"),
        icon: Inbox,
        onclick: onOpenInbox,
      },
    ];
  }
</script>

<div
  class="rounded-xl overflow-hidden h-full {isFocal
    ? 'signature-gradient-border glow-primary'
    : 'bg-card border border-border'}"
  data-testid="dashboard-pending-card"
>
  <DashboardCardHeader
    title={$t("dashboard.card_pending_title")}
    count={total}
    onSeeAll={total > 0 ? onSeeAll : undefined}
    seeAllLabel={$t("dashboard.see_all_arrow")}
    seeAllAria={$t("dashboard.see_all_pending_aria")}
    seeAllTestid="dashboard-see-all-pending"
  />

  {#if rows.length === 0}
    <EmptyState title={emptyTitle} desc={emptyDesc} tone="success">
      {#snippet icon()}<FileCheck size={22} />{/snippet}
    </EmptyState>
  {:else}
    <div class="flex flex-col" data-testid="dashboard-pending-list">
      {#each visible as row, i (row.id)}
        <div in:fly={{ ...rowIn(), delay: i * 30 }}>
          <ContextMenu items={rowMenu()}>
            <InboxRow
              type={row.type}
              title={row.title}
              agent={row.agent}
              timestamp={row.timestamp}
              unread={i === 0}
              onAction={onOpenInbox}
            />
          </ContextMenu>
        </div>
      {/each}
      {#if overflow > 0}
        <button
          type="button"
          class="px-4 py-2.5 text-caption text-primary text-left transition-colors hover:bg-muted/40 focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/50"
          onclick={onOpenInbox}
          data-testid="dashboard-inbox-more"
        >
          {moreLabel}
        </button>
      {/if}
    </div>
  {/if}
</div>
