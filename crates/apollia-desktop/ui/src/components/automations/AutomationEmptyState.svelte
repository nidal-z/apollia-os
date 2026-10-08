<script lang="ts">
  /**
   * Empty state for `/automations`.
   *
   * Single CTA wires the creation wizard.
   */
  import { t } from "svelte-i18n";
  import { Repeat } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { EmptyState } from "$lib/components/operator";

  interface Props {
    onCreate: () => void;
  }

  let { onCreate }: Props = $props();
</script>

<EmptyState title={$t("automations.empty.title")} desc={$t("automations.empty.description")} tone="neutral">
  {#snippet icon()}<Repeat size={24} />{/snippet}
  {#snippet action()}
    <Button variant="primary-solid" size="sm" onclick={onCreate}>
      {$t("automations.empty.primary")}
    </Button>
  {/snippet}
</EmptyState>
