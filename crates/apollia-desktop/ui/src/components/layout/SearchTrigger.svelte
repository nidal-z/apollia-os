<script lang="ts">
  /**
   * Topbar search pill. Click — or `⌘K` globally — opens
   * the command palette. Collapses to an icon-only button
   * below `sm`.
   */
  import { t } from "svelte-i18n";
  import { Search } from "lucide-svelte";
  import { openCommandPalette } from "$lib/stores/commandPalette";

  const isMac = typeof navigator !== "undefined" && navigator.platform.includes("Mac");
  const mod = isMac ? "⌘" : "Ctrl";
</script>

<!-- Desktop: pill with placeholder + kbd -->
<button
  type="button"
  onclick={openCommandPalette}
  class="hidden sm:inline-flex h-9 min-w-0 max-w-sm flex-1 items-center gap-2 rounded-full border border-border bg-muted/50 px-3 text-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
  aria-label={$t('topbar.search.aria_label')}
  data-testid="topbar-search-trigger"
>
  <Search size={14} strokeWidth={1.75} class="shrink-0" />
  <span class="flex-1 truncate text-left">{$t('topbar.search.placeholder')}</span>
  <kbd
    class="shrink-0 rounded border border-border bg-background px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground"
    aria-hidden="true"
  >
    {mod}K
  </kbd>
</button>

<!-- Mobile: icon only -->
<button
  type="button"
  onclick={openCommandPalette}
  class="sm:hidden inline-flex h-10 w-10 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
  aria-label={$t('topbar.search.aria_label')}
  data-testid="topbar-search-trigger-mobile"
>
  <Search size={18} strokeWidth={1.75} />
</button>
