<script lang="ts">
  import { cn } from "$lib/utils";
  import type { HTMLButtonAttributes } from "svelte/elements";

  interface Props extends HTMLButtonAttributes {
    variant?: "default" | "destructive" | "outline" | "secondary" | "ghost" | "link";
    size?: "default" | "sm" | "lg" | "icon";
    class?: string;
  }

  let {
    variant = "default",
    size = "default",
    class: className = "",
    children,
    ...restProps
  }: Props = $props();

  const variantClasses: Record<string, string> = {
    default: "bg-[rgba(52,53,245,0.10)] text-primary border border-primary/20 backdrop-blur-sm shadow-[0_2px_12px_-2px_rgba(52,53,245,0.15)] hover:bg-[rgba(52,53,245,0.16)] hover:shadow-[0_4px_20px_-2px_rgba(52,53,245,0.25)] hover:border-primary/30",
    destructive: "bg-destructive text-destructive-foreground hover:bg-destructive/90",
    outline: "glass-border glass-surface text-foreground hover:bg-[rgba(52,53,245,0.05)]",
    secondary: "bg-secondary/10 text-secondary border border-secondary/20 hover:bg-secondary/15",
    ghost: "text-foreground hover:bg-[rgba(52,53,245,0.04)]",
    link: "text-primary underline-offset-4 hover:underline",
  };

  const sizeClasses: Record<string, string> = {
    default: "h-10 px-4 py-2",
    sm: "h-9 rounded-md px-3",
    lg: "h-11 rounded-md px-8",
    icon: "h-10 w-10",
  };
</script>

<button
  class={cn(
    "inline-flex items-center justify-center whitespace-nowrap rounded-md text-sm font-medium ring-offset-background transition-all duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50 hover:scale-[1.02] active:scale-[0.98]",
    variantClasses[variant],
    sizeClasses[size],
    className
  )}
  {...restProps}
>
  {@render children?.()}
</button>
