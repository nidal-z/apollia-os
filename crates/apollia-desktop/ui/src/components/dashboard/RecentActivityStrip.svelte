<script lang="ts">
  /**
   * RecentActivityStrip - four most-recent task events as mini activity cards.
   * Each card is a Button (hover-lift Card) with a right-click ContextMenu.
   */
  import { t } from "svelte-i18n";
  import { Activity, ExternalLink } from "lucide-svelte";
  import { SectionTitle, Card } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { ContextMenu, type ActionMenuItem } from "$lib/components/ui/context-menu";
  import type { ActivityRow } from "./dashboardData";

  interface Props {
    items: ActivityRow[];
    onOpen: () => void;
  }

  let { items, onOpen }: Props = $props();

  function statusClass(status: string): string {
    const s = status.toLowerCase();
    if (s === "completed") return "text-success-a11y";
    if (s === "running" || s === "working") return "text-primary";
    if (s === "failed") return "text-danger-a11y";
    return "text-warning-a11y";
  }

  function rowMenu(): ActionMenuItem[] {
    return [
      {
        id: "open",
        label: $t("dashboard.ctx_open_task"),
        icon: ExternalLink,
        onclick: onOpen,
      },
    ];
  }
</script>

<div class="px-8 pt-2">
  <SectionTitle count={items.length}>
    {$t("dashboard.recent_activity_title")}
    {#snippet action()}
      <button
        type="button"
        class="text-caption text-primary hover:text-primary/80 transition-colors rounded focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/50"
        onclick={onOpen}
        data-testid="dashboard-recent-see-all"
      >
        {$t("dashboard.see_all_short_arrow")}
      </button>
    {/snippet}
  </SectionTitle>
</div>

{#if items.length === 0}
  <div class="px-8 pb-2 text-body-xs text-muted-foreground/80">
    {$t("dashboard.no_recent_activity")}
  </div>
{:else}
  <div class="px-8 grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
    {#each items as item (item.id)}
      <ContextMenu items={rowMenu()}>
        <Card hover>
          <Button
            variant="ghost"
            size="auto"
            type="button"
            class="w-full text-left px-3 py-2.5 flex-col items-stretch rounded-xl focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/50"
            onclick={onOpen}
            data-testid="dashboard-activity-{item.id}"
          >
            <div class="flex items-center gap-1.5 mb-1 {statusClass(item.status)}">
              <Activity size={10} />
              <span class="text-overline font-mono uppercase">{item.status}</span>
            </div>
            <div class="text-body-xs font-medium text-foreground truncate">
              {item.name}
            </div>
            <div class="text-caption text-muted-foreground/80 mt-0.5">
              {item.time}
            </div>
          </Button>
        </Card>
      </ContextMenu>
    {/each}
  </div>
{/if}
