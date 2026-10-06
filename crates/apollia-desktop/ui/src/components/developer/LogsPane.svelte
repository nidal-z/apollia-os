<script lang="ts">
  /** The logs pane of the developer screen, under the local server. */
  import { t } from "svelte-i18n";
  import { developerMode } from "$lib/stores/developerMode";
</script>

{#if $developerMode}
  <section
    class="flex shrink-0 items-center border-t border-border px-4 py-2"
    aria-labelledby="developer-logs-title"
    data-testid="developer-pane-logs"
  >
    <h2 id="developer-logs-title" class="text-sm font-medium text-foreground">
      {$t("dev.developer.logs.title")}
    </h2>
  </section>
{/if}
