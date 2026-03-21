<script lang="ts">
  import { cn } from "$lib/utils";
  import { Check } from "lucide-svelte";

  interface Props {
    checked?: boolean;
    onchange?: (checked: boolean) => void;
    disabled?: boolean;
    class?: string;
    id?: string;
    name?: string;
    "data-testid"?: string;
  }

  let {
    checked = $bindable(false),
    onchange,
    disabled = false,
    class: className = "",
    id,
    name,
    ...restProps
  }: Props = $props();

  function handleToggle() {
    if (disabled) return;
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
  {disabled}
  {id}
  class={cn(
    "inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-[3px] border border-border ring-offset-background transition-all duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50",
    checked ? "bg-primary border-primary text-primary-foreground" : "bg-background",
    className,
  )}
  onclick={handleToggle}
  onkeydown={handleKeydown}
  {...restProps}
>
  {#if checked}
    <Check size={12} strokeWidth={2.5} />
  {/if}
</button>
{#if name}
  <input type="hidden" {name} value={checked ? "on" : ""} />
{/if}
