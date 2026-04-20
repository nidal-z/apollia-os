<!--
  Liste / Grille toggle for the Projects route.

  Preference persisted in localStorage under `projects.view`. Reads the
  initial value on mount; writes back on every change. Emits the new
  value through a prop callback so the parent can react reactively.
-->
<script lang="ts">
  import { List, LayoutGrid } from "lucide-svelte";
  import { t } from "svelte-i18n";

  export type ProjectsView = "list" | "grid";

  interface Props {
    view: ProjectsView;
    onchange: (next: ProjectsView) => void;
  }

  let { view, onchange }: Props = $props();

  function set(next: ProjectsView) {
    if (next === view) return;
    onchange(next);
  }
</script>

<div
  class="inline-flex rounded-md border border-border/60 bg-muted/30 p-0.5"
  role="group"
  aria-label={$t("projects.view_toggle_label")}
  data-testid="projects-view-toggle"
>
  <button
    class="inline-flex h-7 items-center gap-1.5 rounded px-2 text-xs font-medium transition-colors {view === 'list' ? 'bg-card text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'}"
    onclick={() => set("list")}
    aria-pressed={view === "list"}
    data-testid="projects-view-list-btn"
    title={$t("projects.view_list")}
  >
    <List size={12} strokeWidth={1.75} />
    <span>{$t("projects.view_list")}</span>
  </button>
  <button
    class="inline-flex h-7 items-center gap-1.5 rounded px-2 text-xs font-medium transition-colors {view === 'grid' ? 'bg-card text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'}"
    onclick={() => set("grid")}
    aria-pressed={view === "grid"}
    data-testid="projects-view-grid-btn"
    title={$t("projects.view_grid")}
  >
    <LayoutGrid size={12} strokeWidth={1.75} />
    <span>{$t("projects.view_grid")}</span>
  </button>
</div>
