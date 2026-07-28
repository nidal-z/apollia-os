<script lang="ts">
  /**
   * AgentRowContextMenu - right-click / long-press menu for a sidebar row.
   *
   * A thin adapter over the shared `ContextMenu` primitive: it takes the row's
   * action list (start / stop / logs / config / uninstall) and wraps the row's
   * main control as the trigger, so the same actions are reachable by mouse,
   * keyboard, and pointer without duplicating the item list per row type.
   */
  import type { Snippet } from "svelte";
  import { ContextMenu } from "$lib/components/ui/context-menu";
  import type { ActionMenuItem } from "$lib/components/ui/action-menu";

  interface Props {
    items: ActionMenuItem[];
    children: Snippet;
    class?: string;
    "data-testid"?: string;
  }

  let { items, children, class: className = "", "data-testid": testid }: Props =
    $props();
</script>

<ContextMenu {items} class={className} data-testid={testid}>
  {@render children()}
</ContextMenu>
