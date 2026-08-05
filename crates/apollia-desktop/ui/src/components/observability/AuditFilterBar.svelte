<script lang="ts">
  // Filter bar for the audit trail (tool + agent selects) plus the optional
  // verify action. Extracted verbatim from AuditTrailTable so the table stays
  // focused on data orchestration. Filter values are bound back to the parent.
  import { t } from "svelte-i18n";
  import { Select } from "$lib/components/ui/select";
  import { FormField } from "$lib/components/ui/form-field";
  import AuditVerifyButton from "./AuditVerifyButton.svelte";

  interface Props {
    filterTool: string;
    filterAgent: string;
    uniqueTools: string[];
    uniqueAgents: string[];
    runId?: string | undefined;
    /** Forwarded to the verify action: a verdict was produced. */
    onverified?: (() => void) | undefined;
  }

  let {
    filterTool = $bindable(),
    filterAgent = $bindable(),
    uniqueTools,
    uniqueAgents,
    runId = undefined,
    onverified = undefined,
  }: Props = $props();
</script>

<div class="flex flex-wrap items-center gap-x-5 gap-y-3">
  <FormField inline id="filter-tool" label={$t('observability.tool_filter')} labelClass="text-body-xs font-normal">
    <Select id="filter-tool" size="sm" class="w-auto" bind:value={filterTool}>
      <option value="all">{$t('observability.all_tools')}</option>
      {#each uniqueTools as tool (tool)}
        <option value={tool}>{tool}</option>
      {/each}
    </Select>
  </FormField>

  <FormField inline id="filter-agent" label={$t('observability.agent_filter')} labelClass="text-body-xs font-normal">
    <Select id="filter-agent" size="sm" class="w-auto" bind:value={filterAgent}>
      <option value="all">{$t('observability.all_agents')}</option>
      {#each uniqueAgents as agentName (agentName)}
        <option value={agentName}>{agentName}</option>
      {/each}
    </Select>
  </FormField>

  {#if runId}
    <div class="ml-auto">
      <AuditVerifyButton {runId} {onverified} />
    </div>
  {/if}
</div>
