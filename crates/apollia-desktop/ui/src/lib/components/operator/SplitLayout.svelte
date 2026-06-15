<script lang="ts">
  /**
   * SplitLayout - canonical sidebar + detail route shell.
   *
   * Replaces the hand-rolled `flex min-h-0` + `w-[NNNpx] shrink-0 border-r` +
   * `flex-1` scaffolding duplicated across Tasks (split), Agents, Projects,
   * Connections and Settings. The sidebar width is standardized (280 by default)
   * to remove the 240/280/300/320 fragmentation; override only with a documented
   * reason.
   */
  import { cn } from "$lib/utils";
  import type { Snippet } from "svelte";

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
    sidebar,
    children,
    "data-testid": testid,
  }: Props = $props();
</script>

<div class="flex-1 flex min-h-0" data-testid={testid}>
  <aside
    class={cn("shrink-0 border-r border-border flex flex-col bg-background", sidebarClass)}
    style:width="{sidebarWidth}px"
    data-testid={sidebarTestid}
  >
    {@render sidebar()}
  </aside>
  <section
    class={cn("flex-1 flex flex-col min-w-0 overflow-hidden bg-background", detailClass)}
    data-testid={detailTestid}
  >
    {@render children()}
  </section>
</div>
