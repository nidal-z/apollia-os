<script lang="ts">
  import { cn } from "$lib/utils";
  import type { Snippet } from "svelte";
  import type { Icon } from "lucide-svelte";
  import type { HTMLInputAttributes } from "svelte/elements";

  type Size = "sm" | "default" | "lg";

  interface Props extends Omit<HTMLInputAttributes, "size"> {
    class?: string;
    /** Accepts string (text-like inputs) or number (numeric inputs). Bindable. */
    value?: string | number | null;
    icon?: typeof Icon;
    trailing?: Snippet;
    /**
     * Vertical density:
     * - `sm`      = `h-8 px-2 py-1 text-xs` — table cells, filter rows, dense forms.
     * - `default` = `h-10 px-3 py-2 text-sm` — standard form input (current default).
     * - `lg`      = `h-12 px-4 py-2.5 text-base` — hero search bars.
     */
    size?: Size;
  }

  let {
    class: className = "",
    value = $bindable(""),
    icon: IconComponent,
    trailing,
    disabled,
    size = "default",
    ...restProps
  }: Props = $props();

  const sizeClasses: Record<Size, string> = {
    sm: "h-8 px-2 py-1 text-xs",
    default: "h-10 px-3 py-2 text-sm",
    lg: "h-12 px-4 py-2.5 text-base",
  };

  const hasIcon = $derived(IconComponent !== undefined);
  const hasTrailing = $derived(trailing !== undefined);
  const wrapped = $derived(hasIcon || hasTrailing);

  // Icon offset adapts to size so the icon stays vertically centered.
  const iconPaddingLeft = $derived(
    size === "sm" ? "pl-8" : size === "lg" ? "pl-12" : "pl-10",
  );
  const trailingPaddingRight = $derived(
    size === "sm" ? "pr-8" : size === "lg" ? "pr-12" : "pr-10",
  );
</script>

{#if wrapped}
  <div class="relative">
    {#if IconComponent}
      <span
        class={cn(
          "pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground",
          disabled && "opacity-40",
        )}
        aria-hidden="true"
      >
        <IconComponent size={size === "sm" ? 14 : 16} aria-hidden="true" />
      </span>
    {/if}
    <input
      class={cn(
        "flex w-full rounded-md border border-border bg-background ring-offset-background transition-shadow duration-150 placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:border-primary disabled:cursor-not-allowed disabled:opacity-50",
        sizeClasses[size],
        hasIcon && iconPaddingLeft,
        hasTrailing && trailingPaddingRight,
        className,
      )}
      bind:value
      {disabled}
      {...restProps}
    />
    {#if trailing}
      <span class={cn("absolute right-2 top-1/2 -translate-y-1/2 flex items-center", disabled && "opacity-40")}>
        {@render trailing()}
      </span>
    {/if}
  </div>
{:else}
  <input
    class={cn(
      "flex w-full rounded-md border border-border bg-background ring-offset-background transition-shadow duration-150 placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:border-primary disabled:cursor-not-allowed disabled:opacity-50",
      sizeClasses[size],
      className,
    )}
    bind:value
    {disabled}
    {...restProps}
  />
{/if}
