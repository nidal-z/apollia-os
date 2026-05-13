<script lang="ts">
  import { cn } from "$lib/utils";
  import type { HTMLAttributes } from "svelte/elements";

  type Surface = "glass" | "solid";

  interface Props extends HTMLAttributes<HTMLDivElement> {
    class?: string;
    /** Adds the spring-press effect + glass-card-hover lift on hover. */
    interactive?: boolean;
    /** Adds the primary-tinted glow border for promoted cards. */
    premium?: boolean;
    /**
     * Surface treatment:
     * - `"glass"` (default) — warm glassmorphism (`glass-card` + `glass-border`).
     * - `"solid"` — opaque surface (`bg-card` + `border-border`) for dense
     *   listings / sidebars where the blur backdrop would be noisy.
     */
    surface?: Surface;
  }

  let {
    class: className = "",
    interactive = false,
    premium = false,
    surface = "glass",
    children,
    ...restProps
  }: Props = $props();

  const glassClasses = $derived(
    interactive ? "glass-card-hover card-spring" : "glass-card",
  );
  const solidClasses = $derived(
    interactive
      ? "bg-card border border-border hover-lift cursor-pointer"
      : "bg-card border border-border",
  );
</script>

<div
  class={cn(
    surface === "glass" ? glassClasses : solidClasses,
    surface === "glass" && "glass-border",
    "rounded-xl text-card-foreground",
    premium && "card-premium",
    className
  )}
  {...restProps}
>
  {@render children?.()}
</div>

<style>
  /* Spring press (F.1 / F.3) — cubic-bezier tuned for an elastic
     return. The hover lift is already owned by `.glass-card-hover`
     (see app.css), so we only refine the pressed state here. */
  .card-spring:active {
    transform: translateY(-0.5px) scale(0.98);
    transition:
      transform var(--motion-fast) var(--ease-spring),
      box-shadow var(--motion-fast) var(--ease-spring);
  }

  .card-premium {
    border-width: 2px;
    border-color: hsl(var(--primary) / 0.20);
    box-shadow:
      0 0 20px -2px hsl(var(--primary) / 0.20),
      var(--shadow-elev-2);
  }
  :global(.dark) .card-premium {
    border-color: hsl(var(--primary) / 0.30);
    box-shadow:
      0 0 20px -2px hsl(var(--primary) / 0.35),
      var(--shadow-elev-2);
  }

  @media (prefers-reduced-motion: reduce) {
    .card-spring:active {
      transform: none;
    }
  }
</style>
