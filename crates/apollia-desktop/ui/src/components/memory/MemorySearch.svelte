<script lang="ts">
  import { Search } from "lucide-svelte";
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

<div class="w-full sm:w-64">
  <Input
    type="text"
    placeholder={$t('memory.search_placeholder')}
    aria-label={$t('memory.search_placeholder')}
    value={internalValue}
    oninput={handleInput}
    icon={Search}
  >
    {#snippet trailing()}
      {#if internalValue.length > 0}
        <button
          type="button"
          class="text-muted-foreground hover:text-foreground"
          onclick={handleClear}
          aria-label={$t("a11y.clear_search")}
        >
          &times;
        </button>
      {/if}
    {/snippet}
  </Input>
</div>
