<script lang="ts">
  /**
   * The foot of the local server pane: which endpoints the server answers,
   * and the way to the page that describes them, in the documentation pane
   * when the screen can open a page there, on the documentation site
   * otherwise.
   */
  import { t } from "svelte-i18n";
  import { ChevronRight } from "lucide-svelte";
  import { developerMode } from "$lib/stores/developerMode";
  import { docsUrl } from "$lib/utils/docsUrl";
  import { openExternalUrl } from "$lib/utils/externalLink";
  import { openEndpoints } from "./servedItems";

  interface Props {
    /** Opens a page in the documentation pane; absent when the pane reads none. */
    onpage?: (page: string) => void;
  }

  let { onpage }: Props = $props();
</script>

{#if $developerMode}
  <button
    type="button"
    class="flex w-full shrink-0 items-center gap-3 rounded-lg border border-border bg-card px-3 py-2 text-left transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
    aria-label={$t("dev.developer.endpoints.open")}
    onclick={() => openEndpoints(onpage, (url) => void openExternalUrl(url), docsUrl)}
    data-testid="developer-endpoints"
  >
    <span class="text-sm font-medium text-foreground">{$t("dev.developer.endpoints.title")}</span>
    <span class="min-w-0 flex-1 truncate text-xs text-muted-foreground">
      {$t("dev.developer.endpoints.summary")}
    </span>
    <ChevronRight size={14} class="shrink-0 text-muted-foreground" aria-hidden="true" />
  </button>
{/if}
