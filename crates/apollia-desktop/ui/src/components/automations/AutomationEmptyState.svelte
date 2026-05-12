<script lang="ts">
  /**
   * Empty state for `/automations`.
   *
   * Single CTA wires the creation wizard.
   */
  import { t } from "svelte-i18n";
  import { Sparkles } from "lucide-svelte";
  import { EmptyState } from "$lib/components/layout";

  interface Props {
    onCreate: () => void;
  }

  let { onCreate }: Props = $props();
</script>

<EmptyState
  icon={Sparkles}
  title={$t("automations.empty.title")}
  description={$t("automations.empty.description")}
  primaryLabel={$t("automations.empty.primary")}
  primaryAction={onCreate}
  page="automations"
/>
