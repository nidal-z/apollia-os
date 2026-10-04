<script lang="ts">
  /**
   * BuiltinToolsDetail - the page of the tools Apollia ships, a source of tools
   * of its own on the Connections screen: turn each on, configure it, manage its
   * credentials. The contract of a tool shows with the developer mode on only.
   */
  import { t } from "svelte-i18n";
  import { Wrench } from "lucide-svelte";
  import { DetailHeader } from "$lib/components/operator";
  import { developerMode } from "$lib/stores/developerMode";
  import ConnectorTile from "../shared/ConnectorTile.svelte";
  import BuiltinToolsCatalog from "../../tools/BuiltinToolsCatalog.svelte";
</script>

<DetailHeader
  title={$t("connections.builtin_name")}
  titleTestid="builtin-tools-title"
  meta={$t("connections.builtin_description")}
>
  {#snippet leading()}
    <ConnectorTile size="lg" accent="hsl(var(--primary))" glow>
      {#snippet icon()}<Wrench size={18} />{/snippet}
    </ConnectorTile>
  {/snippet}
</DetailHeader>

<div class="flex-1 overflow-auto px-8 py-5" data-testid="builtin-tools-detail">
  <div class="max-w-3xl">
    <BuiltinToolsCatalog showContract={$developerMode} />
  </div>
</div>
