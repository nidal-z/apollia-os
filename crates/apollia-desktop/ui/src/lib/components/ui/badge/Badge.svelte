<script lang="ts">
  import { cn } from "$lib/utils";
  import type { HTMLAttributes } from "svelte/elements";

  type Variant = "default" | "secondary" | "destructive" | "outline";

  interface Props extends HTMLAttributes<HTMLDivElement> {
    class?: string;
    variant?: Variant;
  }

  const variantClasses: Record<Variant, string> = {
    default: "border-transparent bg-primary text-primary-foreground hover:bg-primary/80",
    secondary: "border-transparent bg-secondary text-secondary-foreground hover:bg-secondary/80",
    destructive: "border-transparent bg-destructive text-destructive-foreground hover:bg-destructive/80",
    outline: "text-foreground",
  };

  let { class: className = "", variant = "default", children, ...restProps }: Props = $props();
</script>

<div
  class={cn(
    "inline-flex items-center rounded-full border px-2.5 py-0.5 text-xs font-semibold transition-colors focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2",
    variantClasses[variant],
    className,
  )}
  {...restProps}
>
  {@render children?.()}
</div>
