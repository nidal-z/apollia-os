<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Search, X } from "lucide-svelte";
  import { sessionSearchQuery } from "$lib/stores/chatSessions";

  let inputEl: HTMLInputElement | undefined = $state();

  function handleShortcut(event: KeyboardEvent) {
    const isSearchShortcut =
      (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "f";
    if (!isSearchShortcut) return;
    event.preventDefault();
    inputEl?.focus();
    inputEl?.select();
  }

  onMount(() => {
    window.addEventListener("keydown", handleShortcut);
    return () => window.removeEventListener("keydown", handleShortcut);
  });

  function clear() {
    sessionSearchQuery.set("");
    inputEl?.focus();
  }
</script>

<div class="relative" data-testid="session-search-field">
  <Search
    size={12}
    class="absolute left-2 top-1/2 -translate-y-1/2 text-muted-foreground/50 pointer-events-none"
  />
  <input
    bind:this={inputEl}
    type="text"
    bind:value={$sessionSearchQuery}
    placeholder={$t("chat.sidebar_search_placeholder")}
    class="h-7 w-full rounded-md border border-border/40 bg-background/60 pl-7 pr-7 text-[11px]
      placeholder:text-muted-foreground/40 focus:outline-none focus:ring-1 focus:ring-primary/40
      transition-colors"
    data-testid="session-search-input"
  />
  {#if $sessionSearchQuery}
    <button
      type="button"
      class="absolute right-1.5 top-1/2 -translate-y-1/2 rounded p-0.5
        text-muted-foreground/40 hover:text-foreground hover:bg-muted/50 transition-colors"
      onclick={clear}
      aria-label={$t("common.clear")}
      data-testid="session-search-clear"
    >
      <X size={11} />
    </button>
  {/if}
</div>
