<script lang="ts">
  import { cn } from "$lib/utils";
  import { Check, Loader2 } from "lucide-svelte";

  interface Props {
    checked?: boolean;
    onchange?: (checked: boolean) => void;
    disabled?: boolean;
    loading?: boolean;
    class?: string;
    id?: string;
    name?: string;
    "data-testid"?: string;
  }

  let {
    checked = $bindable(false),
    onchange,
    disabled = false,
    loading = false,
    class: className = "",
    id,
    name,
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
</script>

<button
  type="button"
  role="checkbox"
  aria-checked={checked}
  aria-busy={loading || undefined}
  disabled={disabled || loading}
  {id}
  class={cn(
    "inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-[3px] border border-border ring-offset-background transition-all duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50",
    checked && !loading ? "bg-primary border-primary text-primary-foreground" : "bg-background",
    className,
  )}
  onclick={handleToggle}
  onkeydown={handleKeydown}
  {...restProps}
>
  {#if loading}
    <Loader2 size={12} strokeWidth={2.5} class="animate-spin" />
  {:else if checked}
    <Check size={12} strokeWidth={2.5} />
  {/if}
</button>
{#if name}
  <input type="hidden" {name} value={checked ? "on" : ""} />
{/if}
