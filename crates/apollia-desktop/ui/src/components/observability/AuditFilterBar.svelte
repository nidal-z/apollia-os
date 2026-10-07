<script lang="ts">
  // Filter bar for the audit trail (tool + agent selects). Extracted from
  // AuditTrailTable so the table stays focused on data orchestration. Filter
  // values are bound back to the parent.
  import { t } from "svelte-i18n";
  import { Select } from "$lib/components/ui/select";
  import { FormField } from "$lib/components/ui/form-field";

  interface Props {
    filterTool: string;
    filterAgent: string;
    uniqueTools: string[];
    uniqueAgents: string[];
  }

  let {
    filterTool = $bindable(),
    filterAgent = $bindable(),
    uniqueTools,
    uniqueAgents,
  }: Props = $props();
</script>

<div class="flex flex-wrap items-center gap-x-5 gap-y-3">
  <FormField inline id="filter-tool" label={$t('observability.tool_filter')} labelClass="text-body-xs font-normal">
    <Select id="filter-tool" size="sm" class="w-auto" bind:value={filterTool} data-testid="audit-filter-tool">
      <option value="all">{$t('observability.all_tools')}</option>
      {#each uniqueTools as tool (tool)}
        <option value={tool}>{tool}</option>
      {/each}
    </Select>
  </FormField>

  <FormField inline id="filter-agent" label={$t('observability.agent_filter')} labelClass="text-body-xs font-normal">
    <Select id="filter-agent" size="sm" class="w-auto" bind:value={filterAgent} data-testid="audit-filter-agent">
      <option value="all">{$t('observability.all_agents')}</option>
      {#each uniqueAgents as agentName (agentName)}
        <option value={agentName}>{agentName}</option>
      {/each}
    </Select>
  </FormField>
</div>
