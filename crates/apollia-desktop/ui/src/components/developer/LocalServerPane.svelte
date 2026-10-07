<script lang="ts">
  /**
   * The local server pane of the developer screen: the bar of the server,
   * then what the server serves (the models in memory, the exposed agents
   * and workflows), and the endpoints it answers.
   */
  import { t } from "svelte-i18n";
  import { developerMode } from "$lib/stores/developerMode";
  import ExposedItemsSection from "./ExposedItemsSection.svelte";
  import ServerBar from "./ServerBar.svelte";
  import ServerModelsSection from "./ServerModelsSection.svelte";
  import SupportedEndpoints from "./SupportedEndpoints.svelte";

  interface Props {
    /** Opens a page in the documentation pane; absent when the pane reads none. */
    onpage?: (page: string) => void;
  }

  let { onpage }: Props = $props();
</script>

{#if $developerMode}
  <section
    class="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-4"
    aria-labelledby="developer-server-title"
    data-testid="developer-pane-server"
  >
    <h2 id="developer-server-title" class="sr-only">{$t("dev.developer.views.server")}</h2>
    <ServerBar />
    <ServerModelsSection />
    <ExposedItemsSection />
    <div class="mt-auto">
      <SupportedEndpoints {onpage} />
    </div>
  </section>
{/if}
