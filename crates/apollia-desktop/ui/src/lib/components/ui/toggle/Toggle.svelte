<script lang="ts">
  import { cn } from "$lib/utils";
  import { Loader2 } from "lucide-svelte";

  interface Props {
    checked?: boolean;
    onchange?: (checked: boolean) => void;
    disabled?: boolean;
    loading?: boolean;
    size?: "sm" | "default";
    class?: string;
    id?: string;
    "data-testid"?: string;
    "aria-label"?: string;
  }

  let {
    checked = $bindable(false),
    onchange,
    disabled = false,
    loading = false,
    size = "default",
    class: className = "",
    id,
    ...restProps
  }: Props = $props();

  function handleToggle() {
    if (disabled || loading) return;
    checked = !checked;
    onchange?.(checked);
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === " " || event.key === "Enter") {
      event.preventDefault();
      handleToggle();
    }
  }

  const sizes = {
    sm: { track: "h-4 w-7", dot: "h-3 w-3", translate: "translate-x-3", spinner: 10 },
    default: { track: "h-5 w-9", dot: "h-4 w-4", translate: "translate-x-4", spinner: 12 },
  };
</script>

<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-busy={loading || undefined}
  disabled={disabled || loading}
  {id}
  class={cn(
    "relative inline-flex shrink-0 cursor-pointer items-center rounded-full border-2 border-transparent ring-offset-background transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50",
    checked ? "bg-primary" : "bg-border",
    sizes[size].track,
    className,
  )}
  onclick={handleToggle}
  onkeydown={handleKeydown}
  {...restProps}
>
  <span
    class={cn(
      "pointer-events-none flex items-center justify-center rounded-full shadow-sm transition-transform duration-fast",
      loading ? "bg-transparent text-primary-foreground" : "bg-primary-foreground",
      sizes[size].dot,
      checked ? sizes[size].translate : "translate-x-0",
    )}
  >
    {#if loading}
      <Loader2 size={sizes[size].spinner} strokeWidth={2.5} class={cn("animate-spin", checked ? "text-primary-foreground" : "text-foreground/60")} />
    {/if}
  </span>
</button>
