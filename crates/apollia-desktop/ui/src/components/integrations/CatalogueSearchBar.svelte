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

  /**
   * Sync when the parent externally resets `value` to something we
   * did not emit ourselves (e.g. the "clear all" button in FilterBar).
   */
  $effect(() => {
    if (value !== lastEmitted) {
      inputValue = value;
      lastEmitted = value;
    }
  });

  /** Debounce: emit the current query 200 ms after the last keystroke. */
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

  /** Clear the field immediately and restore focus to the input. */
  function clearSearch(): void {
    inputValue = "";
    lastEmitted = "";
    onchange("");
    containerEl?.querySelector<HTMLInputElement>("input")?.focus();
  }
</script>

<div class="relative" bind:this={containerEl} data-testid="catalogue-search-bar">
  <Search
    size={15}
    class="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground pointer-events-none"
    aria-hidden="true"
  />
  <Input
    type="text"
    class="pl-9{inputValue.length > 0 ? ' pr-9' : ''}"
    value={inputValue}
    placeholder={resolvedPlaceholder}
    oninput={(e) => {
      inputValue = (e.currentTarget as HTMLInputElement).value;
    }}
    data-testid="catalogue-search-input"
    aria-label={resolvedPlaceholder}
  />
  {#if inputValue.length > 0}
    <button
      type="button"
      class="absolute right-2.5 top-1/2 -translate-y-1/2 flex items-center justify-center rounded p-0.5
             text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none
             focus-visible:ring-1 focus-visible:ring-ring"
      onclick={clearSearch}
      aria-label={$t("integrations.catalogue.search_clear")}
      data-testid="catalogue-search-clear"
    >
      <X size={14} />
    </button>
  {/if}
</div>
