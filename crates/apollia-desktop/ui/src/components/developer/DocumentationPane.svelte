<script lang="ts">
  /** The documentation pane of the developer screen. */
  import { t } from "svelte-i18n";
  import { developerMode } from "$lib/stores/developerMode";
</script>

{#if $developerMode}
  <section
    class="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-4"
    aria-labelledby="developer-documentation-title"
    data-testid="developer-pane-documentation"
  >
    <h2 id="developer-documentation-title" class="text-heading-sm font-semibold text-foreground">
      {$t("dev.developer.views.documentation")}
    </h2>
  </section>
{/if}
