<script lang="ts">
  /**
   * ProjectSidebarRow - premium master-list row for the Projects split.
   *
   * Wraps the operator `ListRow` (nav variant) in a right-click `ContextMenu`
   * and exposes the same actions through a hover/focus kebab (`ActionMenu`),
   * so a project can be opened, renamed, or deleted from either gesture. The
   * accent bar is token-driven; a live project shows a pulsing `StatusDot`.
   * Row enter / leave / reorder motion is owned by the parent `{#each}`
   * (listMotion presets), so this component stays thin.
   *
   * Duplicate / archive are intentionally absent: no backend command backs
   * them today, so surfacing them would be a dead affordance.
   */
  import { t } from "svelte-i18n";
  import { FolderOpen, Pencil, Trash2 } from "lucide-svelte";
  import { ListRow, StatusDot } from "$lib/components/operator";
  import { ContextMenu, type ActionMenuItem } from "$lib/components/ui/context-menu";
  import { ActionMenu } from "$lib/components/ui/action-menu";
  import type { ProjectSummary } from "$lib/types";

  interface Props {
    project: ProjectSummary;
    /** Accent token color string, e.g. "hsl(var(--primary))". */
    accent: string;
    active: boolean;
    /** Humanized relative time for the last activity line. */
    relativeTime: string;
    /** Pulsing dot when the project is the live selection. */
    live?: boolean;
    onSelect: (id: string) => void;
    onRename: (project: ProjectSummary) => void;
    onDelete: (id: string, name: string) => void;
  }

  let {
    project,
    accent,
    active,
    relativeTime,
    live = false,
    onSelect,
    onRename,
    onDelete,
  }: Props = $props();

  const menuItems = $derived.by<ActionMenuItem[]>(() => [
    {
      id: "open",
      label: $t("projects.open"),
      icon: FolderOpen,
      onclick: () => onSelect(project.id),
      testid: `project-ctx-open-${project.id}`,
    },
    {
      id: "rename",
      label: $t("projects.edit_project"),
      icon: Pencil,
      onclick: () => onRename(project),
      testid: `project-ctx-rename-${project.id}`,
    },
    {
      id: "delete",
      label: $t("common.delete"),
      icon: Trash2,
      variant: "destructive",
      onclick: () => onDelete(project.id, project.name),
      testid: `project-ctx-delete-${project.id}`,
    },
  ]);
</script>

<ContextMenu
  items={menuItems}
  class="group relative mb-0.5"
  data-testid="project-ctx-menu-{project.id}"
>
  <ListRow
    variant="nav"
    state={active ? "active" : "default"}
    class="text-left pr-8"
    onclick={() => onSelect(project.id)}
    data-testid="project-row-{project.id}"
    aria-label={project.name}
  >
    <div
      class="w-1 self-stretch rounded-sm shrink-0 my-0.5"
      style="background: {accent};"
    ></div>
    <div class="min-w-0 flex-1">
      <div
        class="text-body-xs truncate text-foreground"
        style:font-weight={active ? 600 : 500}
      >
        {project.name}
      </div>
      <div class="text-caption text-muted-foreground mt-0.5 inline-flex items-center gap-1.5 truncate">
        {#if live}
          <StatusDot color="hsl(var(--success))" glow />
        {/if}
        <span class="truncate">{relativeTime}</span>
      </div>
    </div>
  </ListRow>

  <div
    class="absolute right-1.5 top-1.5 opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100"
  >
    <ActionMenu
      items={menuItems}
      triggerLabel={$t("projects.open")}
      data-testid="project-kebab-{project.id}"
    />
  </div>
</ContextMenu>
