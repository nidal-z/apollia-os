<script lang="ts">
  /**
   * ContextMenu - right-click / long-press menu.
   *
   * Wraps bits-ui `ContextMenu` and reuses the `ActionMenuItem` type from the
   * kebab `ActionMenu`, so a row can feed one item list to both its overflow
   * menu and its right-click menu. lucide icons, destructive variant, and
   * token-only styling matching the popover surface. Items are `role="menuitem"`
   * and keyboard-traversable (bits-ui handles arrow / type-ahead / Escape).
   */
  import { ContextMenu as ContextMenuPrimitive } from "bits-ui";
  import type { Snippet } from "svelte";
  import { cn } from "$lib/utils";
  import type { ActionMenuItem } from "$lib/components/ui/action-menu";

  interface Props {
    /** Menu items, shared with the row's kebab `ActionMenu`. */
    items: ActionMenuItem[];
    /** The right-click / long-press target area. */
    children: Snippet;
    /** Override class on the trigger wrapper. */
    class?: string;
    /** Optional `data-testid` forwarded to the content. */
    "data-testid"?: string;
  }

  let {
    items,
    children,
    class: className = "",
    "data-testid": testid,
  }: Props = $props();
</script>

<ContextMenuPrimitive.Root>
  <ContextMenuPrimitive.Trigger class={cn("block", className)}>
    {@render children()}
  </ContextMenuPrimitive.Trigger>
  <ContextMenuPrimitive.Portal>
    <ContextMenuPrimitive.Content
      class="glass-card glass-border rounded-lg shadow-lg p-1 min-w-[10rem]"
      style="z-index: var(--z-overlay);"
      data-testid={testid}
    >
      {#each items as item (item.id)}
        <ContextMenuPrimitive.Item
          disabled={item.disabled}
          onSelect={() => item.onclick()}
          class={cn(
            "flex w-full cursor-pointer items-center gap-2 rounded-sm px-2 py-1.5 text-left text-body-xs outline-none transition-colors data-[highlighted]:bg-muted",
            item.variant === "destructive"
              ? "text-destructive data-[highlighted]:bg-destructive/10"
              : "text-foreground",
            item.disabled && "cursor-not-allowed opacity-50",
          )}
          data-testid={item.testid}
        >
          {#if item.icon}
            {@const IconComp = item.icon}
            <IconComp size={14} aria-hidden="true" />
          {/if}
          <span class="flex-1">{item.label}</span>
        </ContextMenuPrimitive.Item>
      {/each}
    </ContextMenuPrimitive.Content>
  </ContextMenuPrimitive.Portal>
</ContextMenuPrimitive.Root>
