<script lang="ts">
  import { untrack } from "svelte";
  import { Search, X } from "lucide-svelte";
  import { t } from "svelte-i18n";
  import { Input } from "$lib/components/ui/input";

  interface Props {
    value: string;
    placeholder?: string;
    onchange: (value: string) => void;
  }

  let { value, placeholder, onchange }: Props = $props();

  /**
   * Last value emitted via `onchange` — lets us distinguish an external
   * parent reset (e.g. "clear all") from a round-trip of our own emit.
   */
  let lastEmitted = untrack(() => value);

  /** Immediate display value — updated on every keystroke. */
  let inputValue = $state(untrack(() => value));

  /** Container ref used to re-focus the inner <input> after clearing. */
  let containerEl = $state<HTMLElement | null>(null);

  const resolvedPlaceholder = $derived(
    placeholder ?? $t("integrations.catalogue.search_placeholder"),
  );

  $effect(() => {
    if (value !== lastEmitted) {
      inputValue = value;
      lastEmitted = value;
    }
  });

  $effect(() => {
    const pending = inputValue;
    const timer = setTimeout(() => {
      if (pending !== lastEmitted) {
        lastEmitted = pending;
        onchange(pending);
      }
    }, 200);
    return () => clearTimeout(timer);
  });

  function clearSearch(): void {
    inputValue = "";
    lastEmitted = "";
    onchange("");
    containerEl?.querySelector<HTMLInputElement>("input")?.focus();
  }
</script>

<div bind:this={containerEl} data-testid="catalogue-search-bar">
  <Input
    type="text"
    value={inputValue}
    placeholder={resolvedPlaceholder}
    icon={Search}
    oninput={(e) => {
      inputValue = (e.currentTarget as HTMLInputElement).value;
    }}
    data-testid="catalogue-search-input"
    aria-label={resolvedPlaceholder}
  >
    {#snippet trailing()}
      {#if inputValue.length > 0}
        <button
          type="button"
          class="flex items-center justify-center rounded p-0.5 text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
          onclick={clearSearch}
          aria-label={$t("integrations.catalogue.search_clear")}
          data-testid="catalogue-search-clear"
        >
          <X size={14} />
        </button>
      {/if}
    {/snippet}
  </Input>
</div>
