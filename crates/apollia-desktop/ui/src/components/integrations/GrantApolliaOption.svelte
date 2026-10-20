<script lang="ts">
  /**
   * GrantApolliaOption - the offer, checked, to let the agent of the chat use
   * a connection being added. A connection added is granted to no agent:
   * unchecked, nothing is granted and the sheet of each agent decides.
   */
  import { t } from "svelte-i18n";
  import { Checkbox } from "$lib/components/ui/checkbox";

  interface Props {
    checked: boolean;
    disabled?: boolean;
  }

  let { checked = $bindable(), disabled = false }: Props = $props();
</script>

<label
  class="flex cursor-pointer items-start gap-2.5 rounded-lg border border-border bg-card px-3 py-2.5"
  data-testid="grant-apollia-option"
>
  <Checkbox bind:checked class="mt-0.5" {disabled} />
  <span class="min-w-0">
    <span class="block text-body-sm font-medium text-foreground">
      {$t("connections.grant_apollia.label")}
    </span>
    <span class="block text-caption text-muted-foreground">
      {$t("connections.grant_apollia.hint")}
    </span>
  </span>
</label>
