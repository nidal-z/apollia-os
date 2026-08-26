<script lang="ts">
  /**
   * SidebarHeader - canonical top of a split-screen sidebar.
   *
   * A title (+ optional count) with right-aligned actions, followed by an
   * optional search row and an optional filters row. Unifies the per-screen
   * sidebar tops (Memory, Projects, Connections, Agents, Tasks split) that
   * each used to roll their own layout.
   */
  import type { Snippet } from "svelte";

  interface Props {
    title?: string;
    /** Count shown next to the title (e.g. number of items). */
    count?: number | string;
    /** Right-aligned action buttons next to the title. */
    actions?: Snippet;
    /** Search field rendered under the title row. */
    search?: Snippet;
    /** Filter row (chips / FilterChipBar) rendered under the search. */
    filters?: Snippet;
    class?: string;
  }

  let { title, count, actions, search, filters, class: className = "" }: Props = $props();
</script>

<div class="flex flex-col gap-2.5 px-4 pt-4 pb-2.5 {className}">
  {#if title || actions}
    <div class="flex min-w-0 items-center gap-2">
      {#if title}
        <h2 class="m-0 truncate text-body-sm font-semibold tracking-tight text-foreground">
          {title}
        </h2>
      {/if}
      {#if count !== undefined}
        <span class="text-micro-lg tabular-nums text-muted-foreground">{count}</span>
      {/if}
      {#if actions}
        <div class="ml-auto flex items-center gap-1.5">{@render actions()}</div>
      {/if}
    </div>
  {/if}
  {#if search}{@render search()}{/if}
  {#if filters}{@render filters()}{/if}
</div>
