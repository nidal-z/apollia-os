<script lang="ts">
  import { cn } from "$lib/utils";
  import type { Snippet } from "svelte";
  import type { Icon } from "lucide-svelte";
  import type { HTMLTextareaAttributes } from "svelte/elements";

  type Size = "sm" | "default" | "lg";

  interface Props extends HTMLTextareaAttributes {
    class?: string;
    value?: string | null;
    icon?: typeof Icon;
    trailing?: Snippet;
    /**
     * Vertical density — mirrors `Input.size`.
     * - `sm`      = `min-h-[64px] px-2 py-1 text-xs`
     * - `default` = `min-h-[80px] px-3 py-2 text-sm` (current default)
     * - `lg`      = `min-h-[120px] px-4 py-2.5 text-base`
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
    sm: "min-h-[64px] px-2 py-1 text-xs",
    default: "min-h-[80px] px-3 py-2 text-sm",
    lg: "min-h-[120px] px-4 py-2.5 text-base",
  };

  const hasIcon = $derived(IconComponent !== undefined);
  const hasTrailing = $derived(trailing !== undefined);
  const wrapped = $derived(hasIcon || hasTrailing);
  const iconPaddingLeft = $derived(
    size === "sm" ? "pl-7" : size === "lg" ? "pl-12" : "pl-10",
  );
  const trailingPaddingRight = $derived(
    size === "sm" ? "pr-7" : size === "lg" ? "pr-12" : "pr-10",
  );
</script>

{#if wrapped}
  <div class="relative">
    {#if IconComponent}
      <span
        class={cn(
          "pointer-events-none absolute left-3 top-3 text-muted-foreground",
          disabled && "opacity-40",
        )}
        aria-hidden="true"
      >
        <IconComponent size={size === "sm" ? 14 : 16} aria-hidden="true" />
      </span>
    {/if}
    <textarea
      bind:value
      class={cn(
        "flex w-full rounded-md border border-border bg-background ring-offset-background transition-shadow duration-150 placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:border-primary/50 disabled:cursor-not-allowed disabled:opacity-50 resize-y",
        sizeClasses[size],
        hasIcon && iconPaddingLeft,
        hasTrailing && trailingPaddingRight,
        className,
      )}
      {disabled}
      {...restProps}
    ></textarea>
    {#if trailing}
      <span class={cn("absolute bottom-2 right-2 flex items-center", disabled && "opacity-40")}>
        {@render trailing()}
      </span>
    {/if}
  </div>
{:else}
  <textarea
    bind:value
    class={cn(
      "flex w-full rounded-md border border-border bg-background ring-offset-background transition-shadow duration-150 placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:border-primary/50 disabled:cursor-not-allowed disabled:opacity-50 resize-y",
      sizeClasses[size],
      className,
    )}
    {disabled}
    {...restProps}
  ></textarea>
{/if}
