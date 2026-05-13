<script lang="ts">
  import { cn } from "$lib/utils";
  import type { HTMLAttributes } from "svelte/elements";

  interface Props extends HTMLAttributes<HTMLDivElement> {
    class?: string;
    interactive?: boolean;
    premium?: boolean;
  }

  let {
    class: className = "",
    interactive = false,
    premium = false,
    children,
    ...restProps
  }: Props = $props();
</script>

<div
  class={cn(
    interactive ? "glass-card-hover card-spring" : "glass-card",
    "glass-border rounded-xl text-card-foreground",
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
