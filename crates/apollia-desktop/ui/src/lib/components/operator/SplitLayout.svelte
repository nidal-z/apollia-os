<script lang="ts">
  /**
   * SplitLayout - canonical sidebar + detail route shell.
   *
   * Replaces the hand-rolled `flex min-h-0` + `w-[NNNpx] shrink-0 border-r` +
   * `flex-1` scaffolding duplicated across Tasks (split), Agents, Projects,
   * Connections and Settings. The sidebar width is standardized (280 by default)
   * to remove the 240/280/300/320 fragmentation; override only with a documented
   * reason.
   *
   * Responsive: at lg+ the sidebar is an inline column. Below lg it collapses to
   * a toggleable overlay drawer so the detail pane keeps full width on narrow
   * windows; a built-in toggle in the detail header opens it.
   */
  import { onMount, type Snippet } from "svelte";
  import { t } from "svelte-i18n";
  import { PanelLeft } from "lucide-svelte";
  import { cn } from "$lib/utils";

  interface Props {
    /** Sidebar width in px. Standard is 280; override sparingly. */
    sidebarWidth?: number;
    /** Extra classes on the <aside>. */
    sidebarClass?: string;
    /** Extra classes on the detail <section>. */
    detailClass?: string;
    /** data-testid on the <aside> (preserves prior per-sidebar selectors). */
    sidebarTestid?: string;
    /** data-testid on the detail <section>. */
    detailTestid?: string;
    /** Label for the narrow-viewport "open list" toggle. */
    sidebarLabel?: string;
    sidebar: Snippet;
    children: Snippet;
    "data-testid"?: string;
  }

  let {
    sidebarWidth = 280,
    sidebarClass = "",
    detailClass = "",
    sidebarTestid,
    detailTestid,
    sidebarLabel,
    sidebar,
    children,
    "data-testid": testid,
  }: Props = $props();

  let isWide = $state(true);
  let drawerOpen = $state(false);

  onMount(() => {
    const mql = globalThis.matchMedia("(min-width: 1024px)");
    const update = () => {
      isWide = mql.matches;
      if (isWide) drawerOpen = false;
    };
    update();
    mql.addEventListener("change", update);
    return () => mql.removeEventListener("change", update);
  });
</script>

<div class="relative flex-1 flex min-h-0" data-testid={testid}>
  {#if !isWide && drawerOpen}
    <button
      type="button"
      class="absolute inset-0 z-drawer-backdrop bg-foreground/20"
      aria-label={$t("common.close")}
      onclick={() => (drawerOpen = false)}
      data-testid="split-sidebar-backdrop"
    ></button>
  {/if}

  <aside
    class={cn(
      "border-r border-border flex flex-col bg-background",
      isWide
        ? "shrink-0"
        : "absolute inset-y-0 left-0 z-drawer max-w-[85%] shadow-elev-3 transition-transform duration-base " +
            (drawerOpen ? "translate-x-0" : "-translate-x-full"),
      sidebarClass,
    )}
    style:width="{sidebarWidth}px"
    data-testid={sidebarTestid}
    aria-hidden={!isWide && !drawerOpen}
  >
    {@render sidebar()}
  </aside>

  <section
    class={cn("flex-1 flex flex-col min-w-0 overflow-hidden bg-background", detailClass)}
    data-testid={detailTestid}
  >
    {#if !isWide}
      <div class="flex items-center border-b border-border/60 px-2 py-1.5">
        <button
          type="button"
          onclick={() => (drawerOpen = true)}
          class="inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-body-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          data-testid="split-open-sidebar"
        >
          <PanelLeft size={14} />
          {sidebarLabel ?? $t("common.show_list")}
        </button>
      </div>
    {/if}
    {@render children()}
  </section>
</div>
