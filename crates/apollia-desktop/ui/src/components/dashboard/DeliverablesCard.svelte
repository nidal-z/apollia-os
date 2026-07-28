<script lang="ts">
  /**
   * DeliverablesCard - secondary bento card listing today's ready deliverables.
   * Reuses ListRow (nav variant); rows are keyboard-navigable and expose a
   * right-click ContextMenu.
   */
  import { t } from "svelte-i18n";
  import { FileCheck, ExternalLink } from "lucide-svelte";
  import { ListRow } from "$lib/components/operator";
  import { ContextMenu, type ActionMenuItem } from "$lib/components/ui/context-menu";
  import { listNavigation } from "$lib/components/operator/listNavigation";
  import DashboardCardHeader from "./DashboardCardHeader.svelte";
  import type { DeliverableRow } from "./dashboardData";

  interface Props {
    items: DeliverableRow[];
    onOpen: () => void;
  }

  let { items, onOpen }: Props = $props();

  function rowMenu(): ActionMenuItem[] {
    return [
      {
        id: "open",
        label: $t("dashboard.ctx_open_deliverable"),
        icon: ExternalLink,
        onclick: onOpen,
      },
    ];
  }
</script>

<div class="bg-card border border-border rounded-xl overflow-hidden">
  <DashboardCardHeader
    title={$t("dashboard.card_deliverables_title")}
    count={items.length}
    dense
  />
  {#if items.length === 0}
    <div class="px-4 pb-4 text-body-xs text-muted-foreground/80">
      {$t("dashboard.deliverables_empty")}
    </div>
  {:else}
    <div class="flex flex-col px-2 pb-2" use:listNavigation>
      {#each items as item (item.id)}
        <ContextMenu items={rowMenu()}>
          <ListRow
            variant="nav"
            align="center"
            onclick={onOpen}
            aria-label={$t("dashboard.open_task_aria", { values: { name: item.name } })}
            data-testid="dashboard-deliverable-{item.id}"
          >
            <div
              class="w-7 h-7 rounded-md inline-flex items-center justify-center shrink-0 bg-success/10 text-success"
            >
              <FileCheck size={12} />
            </div>
            <div class="flex-1 min-w-0">
              <div class="text-body-xs font-medium text-foreground truncate">
                {item.name}
              </div>
              <div class="text-caption text-muted-foreground truncate">
                {item.time}
              </div>
            </div>
          </ListRow>
        </ContextMenu>
      {/each}
    </div>
  {/if}
</div>
