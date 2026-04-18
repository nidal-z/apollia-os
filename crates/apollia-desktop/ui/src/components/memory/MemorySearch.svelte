<script lang="ts">
  import { t } from "svelte-i18n";
  import { Input } from "$lib/components/ui/input";

  interface Props {
    value: string;
    onsearch: (query: string) => void;
  }

  let { value, onsearch }: Props = $props();

  let internalValue = $state(value);
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  const MIN_QUERY_LENGTH = 3;
  const DEBOUNCE_MS = 300;

  function handleInput(e: Event) {
    internalValue = (e.target as HTMLInputElement).value;

    if (debounceTimer !== null) {
      clearTimeout(debounceTimer);
    }

    if (internalValue.length === 0) {
      onsearch("");
      return;
    }

    if (internalValue.length < MIN_QUERY_LENGTH) {
      return;
    }

    debounceTimer = setTimeout(() => {
      debounceTimer = null;
      onsearch(internalValue);
    }, DEBOUNCE_MS);
  }

  function handleClear() {
    internalValue = "";
    if (debounceTimer !== null) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }
    onsearch("");
  }
</script>

<div class="relative">
  <span class="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground">
    &#x1F50D;
  </span>
  <Input
    type="text"
    placeholder={$t('memory.search_placeholder')}
    class="w-full sm:w-64 pl-9 pr-8"
    value={internalValue}
    oninput={handleInput}
  />
  {#if internalValue.length > 0}
    <button
      class="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
      onclick={handleClear}
      aria-label="Clear search"
    >
      &times;
    </button>
  {/if}
</div>
