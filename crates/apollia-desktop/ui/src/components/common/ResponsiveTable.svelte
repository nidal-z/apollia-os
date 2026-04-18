<script lang="ts">
  import { cn } from "$lib/utils";

  interface Props {
    /** Breakpoint Tailwind à partir duquel on affiche la table (sinon : cards). */
    breakpoint?: "sm" | "md" | "lg";
    class?: string;
    /** Table complète (thead + tbody) rendue >= breakpoint. */
    table?: import("svelte").Snippet;
    /** Liste de cartes rendue sous le breakpoint (mobile). */
    cards?: import("svelte").Snippet;
    "data-testid"?: string;
  }

  let {
    breakpoint = "md",
    class: className = "",
    table,
    cards,
    ...restProps
  }: Props = $props();

  const hiddenBelow: Record<"sm" | "md" | "lg", string> = {
    sm: "hidden sm:block",
    md: "hidden md:block",
    lg: "hidden lg:block",
  };

  const visibleBelow: Record<"sm" | "md" | "lg", string> = {
    sm: "block sm:hidden",
    md: "block md:hidden",
    lg: "block lg:hidden",
  };
</script>

<div class={cn("w-full", className)} {...restProps}>
  {#if cards}
    <div class={cn("space-y-3", visibleBelow[breakpoint])}>
      {@render cards()}
    </div>
    <div class={cn("overflow-x-auto rounded-md glass-border glass-surface", hiddenBelow[breakpoint])}>
      {@render table?.()}
    </div>
  {:else}
    <div class="overflow-x-auto rounded-md glass-border glass-surface">
      {@render table?.()}
    </div>
  {/if}
</div>
