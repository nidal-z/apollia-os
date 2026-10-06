<script lang="ts">
  /**
   * The local server pane of the developer screen: the bar of the server,
   * then what the server exposes.
   */
  import { t } from "svelte-i18n";
  import { developerMode } from "$lib/stores/developerMode";
  import ServerBar from "./ServerBar.svelte";
</script>

{#if $developerMode}
  <section
    class="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-4"
    aria-labelledby="developer-server-title"
    data-testid="developer-pane-server"
  >
    <h2 id="developer-server-title" class="sr-only">{$t("dev.developer.views.server")}</h2>
    <ServerBar />
  </section>
{/if}
