<script lang="ts">
  /**
   * ActionMenu - canonical kebab/overflow menu.
   *
   * Wraps `<Popover>` with a default kebab trigger and a vertically-stacked
   * list of `role="menuitem"` buttons. Either feed `items` (declarative) or
   * provide a `body` snippet for custom menu content.
   */
  import { MoreVertical } from "lucide-svelte";
  import { Popover } from "$lib/components/ui/popover";
  import { cn } from "$lib/utils";
  import type { Snippet } from "svelte";
  import type { Icon } from "lucide-svelte";
  import { t } from "svelte-i18n";

  export interface ActionMenuItem {
    /** Stable identifier (used for keying + telemetry). */
    id: string;
    /** Displayed label. */
    label: string;
    /** Optional leading icon. */
    icon?: typeof Icon;
    /** Click handler. */
    onclick: () => void;
    /** Variant - `destructive` renders the row in red. */
    variant?: "default" | "destructive";
    /** Disables the row. */
    disabled?: boolean;
    /** Optional `data-testid`. */
    testid?: string;
  }

  interface Props {
    /** Controlled/bindable open state. Defaults to internally managed. */
    open?: boolean;
    /** Static list of menu items. Mutually exclusive with `body`. */
    items?: ActionMenuItem[];
    /** Custom content snippet (full control over the menu body). */
    body?: Snippet<[{ close: () => void }]>;
    /** Custom trigger snippet. Defaults to a kebab icon button. */
    triggerSlot?: Snippet<[Record<string, unknown>]>;
    /** Accessible label for the default trigger button. Falls back to `a11y.actions_menu`. */
    triggerLabel?: string;
    /** Popover alignment. */
    align?: "start" | "center" | "end";
    /** Popover side. */
    side?: "top" | "right" | "bottom" | "left";
    /** Optional override class on the popover content. */
    class?: string;
    /** Optional `data-testid` forwarded to the trigger. */
    "data-testid"?: string;
  }

  let {
    open = $bindable(false),
    items,
    body,
    triggerSlot,
    triggerLabel,
    align = "end",
    side = "bottom",
    class: className = "",
    "data-testid": testid,
  }: Props = $props();

  function close() {
    open = false;
  }
</script>

<Popover bind:open {side} {align} class={cn("p-1 min-w-[10rem]", className)}>
  {#snippet trigger(triggerProps)}
    {#if triggerSlot}
      {@render triggerSlot(triggerProps)}
    {:else}
      <button
        type="button"
        {...triggerProps}
        class="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
        aria-label={triggerLabel ?? $t("a11y.actions_menu")}
        data-testid={testid}
      >
        <MoreVertical size={14} strokeWidth={1.75} aria-hidden="true" />
      </button>
    {/if}
  {/snippet}

  {#snippet content()}
    {#if items}
      <ul class="flex flex-col gap-0.5" role="menu">
        {#each items as item (item.id)}
          <li role="none">
            <button
              type="button"
              role="menuitem"
              disabled={item.disabled}
              class={cn(
                "flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-[12px] transition-colors",
                item.variant === "destructive"
                  ? "text-destructive hover:bg-destructive/10"
                  : "text-foreground hover:bg-muted",
                item.disabled && "opacity-50 cursor-not-allowed",
              )}
              onclick={() => {
                item.onclick();
                close();
              }}
              data-testid={item.testid}
            >
              {#if item.icon}
                {@const IconComp = item.icon}
                <IconComp size={14} aria-hidden="true" />
              {/if}
              <span class="flex-1">{item.label}</span>
            </button>
          </li>
        {/each}
      </ul>
    {:else if body}
      {@render body({ close })}
    {/if}
  {/snippet}
</Popover>
