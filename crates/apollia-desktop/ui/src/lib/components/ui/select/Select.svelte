<script lang="ts">
  import { cn } from "$lib/utils";
  import { ChevronDown, type Icon } from "lucide-svelte";
  import type { HTMLSelectAttributes } from "svelte/elements";

  type Size = "sm" | "default" | "lg";

  interface Props extends Omit<HTMLSelectAttributes, "size"> {
    class?: string;
    value?: string;
    icon?: typeof Icon;
    /**
     * Vertical density - mirrors `Input.size`.
     * - `sm`      = `h-8 px-2 py-1 text-xs`
     * - `default` = `h-10 px-3 py-2 text-sm`
     * - `lg`      = `h-12 px-4 py-2.5 text-base`
     */
    size?: Size;
  }

  let {
    class: className = "",
    children,
    value = $bindable(""),
    icon: IconComponent,
    disabled,
    size = "default",
    ...restProps
  }: Props = $props();

  const sizeClasses: Record<Size, string> = {
    sm: "h-8 px-2 py-1 pr-7 text-xs",
    default: "h-10 px-3 py-2 pr-8 text-sm",
    lg: "h-12 px-4 py-2.5 pr-10 text-base",
  };

  const hasIcon = $derived(IconComponent !== undefined);
  const iconPaddingLeft = $derived(
    size === "sm" ? "pl-7" : size === "lg" ? "pl-11" : "pl-9",
  );
  const chevronSize = $derived(size === "sm" ? 12 : size === "lg" ? 16 : 14);
</script>

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
  <select
    bind:value
    class={cn(
      "flex w-full appearance-none rounded-md border border-border bg-background ring-offset-background transition-shadow duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:border-primary/50 disabled:cursor-not-allowed disabled:opacity-50",
      sizeClasses[size],
      hasIcon && iconPaddingLeft,
      className,
    )}
    {disabled}
    {...restProps}
  >
    {@render children?.()}
  </select>
  <div
    class={cn(
      "pointer-events-none absolute inset-y-0 right-0 flex items-center text-muted-foreground",
      size === "sm" ? "pr-2" : size === "lg" ? "pr-3" : "pr-2.5",
      disabled && "opacity-40",
    )}
  >
    <ChevronDown size={chevronSize} strokeWidth={1.75} />
  </div>
</div>
