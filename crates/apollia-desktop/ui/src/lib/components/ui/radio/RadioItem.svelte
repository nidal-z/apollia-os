<script lang="ts">
  import { cn } from "$lib/utils";
  import { Loader2 } from "lucide-svelte";

  interface Props {
    value: string;
    checked?: boolean;
    onchange?: (value: string) => void;
    disabled?: boolean;
    loading?: boolean;
    class?: string;
    id?: string;
    name?: string;
    children?: import("svelte").Snippet;
    "data-testid"?: string;
  }

  let {
    value,
    checked = false,
    onchange,
    disabled = false,
    loading = false,
    class: className = "",
    id,
    name,
    children,
    ...restProps
  }: Props = $props();

  function handleClick() {
    if (disabled || loading) return;
    onchange?.(value);
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === " " || event.key === "Enter") {
      event.preventDefault();
      handleClick();
    }
  }
</script>

<label
  class={cn(
    "flex items-center gap-2 cursor-pointer",
    (disabled || loading) && "cursor-not-allowed opacity-50",
    className,
  )}
  {...restProps}
>
  <button
    type="button"
    role="radio"
    aria-checked={checked}
    aria-busy={loading || undefined}
    disabled={disabled || loading}
    {id}
    class="inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full border border-border ring-offset-background transition-all duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
    class:bg-primary={checked && !loading}
    class:border-primary={checked && !loading}
    onclick={handleClick}
    onkeydown={handleKeydown}
  >
    {#if loading}
      <Loader2 size={12} strokeWidth={2.5} class="animate-spin" />
    {:else if checked}
      <div class="h-1.5 w-1.5 rounded-full bg-primary-foreground"></div>
    {/if}
  </button>
  {#if children}
    <span class="text-sm">{@render children()}</span>
  {/if}
  {#if name}
    <input type="hidden" {name} value={checked ? value : ""} />
  {/if}
</label>
