<script lang="ts">
  /**
   * PolicyRule - one line of a policy section, read only: what is imposed on
   * the left, its value on the right. An identifier, an address or a path is
   * set in the fixed-width font.
   */
  import { t } from "svelte-i18n";
  import type { PolicyRuleLine } from "$lib/policy/policyRules";

  let { rule }: { rule: PolicyRuleLine } = $props();

  const label = $derived(rule.labelKey ? $t(rule.labelKey) : (rule.label ?? ""));
  const value = $derived(
    rule.valueKey ? $t(rule.valueKey, { values: rule.valueValues ?? {} }) : (rule.value ?? ""),
  );
</script>

<div
  class="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 py-2"
  data-testid="policy-rule-{rule.id}"
>
  <dt class="text-sm text-foreground {rule.label ? 'font-mono' : ''}">{label}</dt>
  <dd class="text-sm text-muted-foreground {rule.code ? 'break-all font-mono' : ''}">{value}</dd>
</div>
