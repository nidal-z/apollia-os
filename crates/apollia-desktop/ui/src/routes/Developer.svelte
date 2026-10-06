<script lang="ts">
  /**
   * The developer screen, reached from the developer entry of the sidebar with
   * the developer mode on. It holds its title and nothing else yet.
   */
  import { t } from "svelte-i18n";
</script>

<section class="flex h-full flex-col p-6" data-testid="developer-page">
  <h1 class="text-heading-lg font-semibold text-foreground">{$t("nav.developer")}</h1>
</section>
