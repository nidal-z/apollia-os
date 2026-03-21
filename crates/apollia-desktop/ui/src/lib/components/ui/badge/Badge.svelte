<script lang="ts">
  import { cn } from "$lib/utils";
  import type { HTMLAttributes } from "svelte/elements";

  type Variant = "default" | "secondary" | "destructive" | "outline" | "success" | "warning" | "info";

  interface Props extends HTMLAttributes<HTMLDivElement> {
    class?: string;
    variant?: Variant;
  }

  const variantClasses: Record<Variant, string> = {
    default: "bg-primary/10 text-primary dark:bg-primary/20",
    secondary: "bg-muted text-muted-foreground",
    destructive: "bg-red-50 text-red-700 dark:bg-red-950/30 dark:text-red-300",
    outline: "border border-border text-foreground",
    success: "bg-emerald-50 text-emerald-700 dark:bg-emerald-950/30 dark:text-emerald-300",
    warning: "bg-amber-50 text-amber-700 dark:bg-amber-950/30 dark:text-amber-300",
    info: "bg-sky-50 text-sky-700 dark:bg-sky-950/30 dark:text-sky-300",
  };

  let { class: className = "", variant = "default", children, ...restProps }: Props = $props();
</script>

<div
  class={cn(
    "inline-flex items-center rounded-full border border-transparent px-2.5 py-0.5 text-xs font-medium transition-colors",
    variantClasses[variant],
    className,
  )}
  {...restProps}
>
  {@render children?.()}
</div>
