<script lang="ts">
  /**
   * Empty state for `/automations`.
   *
   * Primary CTA wires the creation wizard (US-SP42-050).
   * Secondary CTA browses the template gallery (US-SP42-058).
   */
  import { t } from "svelte-i18n";
  import { Sparkles } from "lucide-svelte";
  import { EmptyState } from "$lib/components/layout";

  interface Props {
    onCreate: () => void;
    onBrowseTemplates: () => void;
  }

  let { onCreate, onBrowseTemplates }: Props = $props();
</script>

<EmptyState
  icon={Sparkles}
  title={$t("automations.empty.title")}
  description={$t("automations.empty.description")}
  primaryLabel={$t("automations.empty.primary")}
  primaryAction={onCreate}
  secondaryLabel={$t("automations.empty.secondary")}
  secondaryAction={onBrowseTemplates}
  page="automations"
/>
