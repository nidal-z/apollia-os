<script lang="ts">
  /**
   * PinnedProjectsStrip - horizontal scroller of the most-recent projects.
   * Reuses ProjectCard; each card carries a right-click ContextMenu.
   */
  import { fly } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { t } from "svelte-i18n";
  import { FolderOpen, ExternalLink } from "lucide-svelte";
  import { SectionTitle, EmptyState, ProjectCard } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { ContextMenu, type ActionMenuItem } from "$lib/components/ui/context-menu";
  import { listFlip, rowIn } from "$lib/design/listMotion";
  import type { ProjectRow } from "./dashboardData";

  interface Props {
    items: ProjectRow[];
    onOpen: () => void;
  }

  let { items, onOpen }: Props = $props();

  function rowMenu(): ActionMenuItem[] {
    return [
      {
        id: "open",
        label: $t("dashboard.ctx_open_project"),
        icon: ExternalLink,
        onclick: onOpen,
      },
    ];
  }
</script>

<div class="pt-4 pb-8">
  <SectionTitle count={items.length}>
    {$t("dashboard.pinned_projects_title")}
    {#snippet action()}
      <button
        type="button"
        class="text-caption text-primary hover:text-primary/80 transition-colors rounded focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/50"
        onclick={onOpen}
        data-testid="dashboard-projects-see-all"
      >
        {$t("dashboard.see_all_projects_arrow")}
      </button>
    {/snippet}
  </SectionTitle>

  {#if items.length === 0}
    <div class="px-8">
      <EmptyState
        title={$t("dashboard.projects_empty_title")}
        desc={$t("dashboard.projects_empty_desc")}
        tone="info"
      >
        {#snippet icon()}<FolderOpen size={22} />{/snippet}
        {#snippet action()}
          <Button variant="outline" size="sm" onclick={onOpen}>
            {$t("dashboard.open_projects")}
          </Button>
        {/snippet}
      </EmptyState>
    </div>
  {:else}
    <div
      class="px-8 flex gap-3 overflow-x-auto pb-2"
      data-testid="dashboard-pinned-projects"
    >
      {#each items as p, i (p.id)}
        <div
          class="shrink-0 w-64"
          animate:flip={listFlip()}
          in:fly={{ ...rowIn(), delay: i * 30 }}
          data-testid="dashboard-project-{p.id}"
        >
          <ContextMenu items={rowMenu()} class="h-full">
            <ProjectCard
              title={p.name}
              description={p.description}
              status={p.status}
              lastActivity={p.lastActivity}
              hover
              onclick={onOpen}
            />
          </ContextMenu>
        </div>
      {/each}
    </div>
  {/if}
</div>
