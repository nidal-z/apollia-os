<script lang="ts">
  /**
   * The target section of the creation dialog.
   *
   * Opened from the sheet of an agent or a workflow, the target is fixed and
   * shown typed, with a line saying it was prefilled; opened from the
   * Automations screen, the agent is picked from the list. An agent then
   * receives a message, a workflow one text per declared input. Every text
   * may hold `{{variables}}` of the fire (date, file, request body).
   */
  import { t } from "svelte-i18n";
  import type { AgentListItem } from "$lib/types";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";
  import { Textarea } from "$lib/components/ui/textarea";
  import { FormField } from "$lib/components/ui/form-field";
  import TargetChip from "./TargetChip.svelte";
  import type { AutomationTarget } from "./triggerTarget";

  interface Props {
    /** The fixed target, `undefined` to pick an agent. */
    target: AutomationTarget | undefined;
    agents: AgentListItem[];
    selectedAgent: string;
    message: string;
    inputs: Record<string, string>;
    targetError: string | null;
    missing: string[];
  }

  let {
    target,
    agents,
    selectedAgent = $bindable(),
    message = $bindable(),
    inputs = $bindable(),
    targetError,
    missing,
  }: Props = $props();

  const fields = $derived(target?.kind === "workflow" ? target.fields : []);
</script>

<div class="space-y-3">
  <div>
    <p class="mb-1.5 block text-caption font-medium text-muted-foreground">{$t("triggers.field_target")}</p>
    {#if target}
      <div class="space-y-1" data-testid="trigger-target-fixed">
        <TargetChip kind={target.kind} name={target.name} testid="trigger-target-chip" />
        <p class="text-caption text-muted-foreground/80">{$t("triggers.prefilled_hint")}</p>
      </div>
    {:else}
      <Select
        bind:value={selectedAgent}
        aria-label={$t("triggers.field_target_agent")}
        data-testid="trigger-input-agent"
      >
        <option value="" disabled>- {$t("triggers.field_target_agent")} -</option>
        {#each agents as agent}
          <option value={agent.name}>{agent.name}</option>
        {/each}
      </Select>
    {/if}
    {#if targetError}
      <p class="mt-0.5 text-xs text-destructive" data-testid="trigger-target-error">{targetError}</p>
    {/if}
  </div>

  {#if target?.kind === "workflow"}
    <div class="space-y-2" data-testid="trigger-workflow-inputs">
      <p class="block text-caption font-medium text-muted-foreground">{$t("triggers.field_workflow_inputs")}</p>
      {#if fields.length === 0}
        <p class="text-caption text-muted-foreground/80">{$t("triggers.workflow_no_inputs")}</p>
      {/if}
      {#each fields as field (field.name)}
        <FormField
          id="trigger-workflow-input-{field.name}"
          label={field.name}
          labelClass="text-caption font-normal mb-0"
          hint={field.description}
          optional={!field.required}
          error={missing.includes(field.name) ? $t("triggers.field_workflow_input_required") : undefined}
        >
          <Input
            id="trigger-workflow-input-{field.name}"
            bind:value={inputs[field.name]}
            data-testid="trigger-workflow-input-{field.name}"
          />
        </FormField>
      {/each}
    </div>
  {:else}
    <FormField
      id="trigger-input-template"
      label={$t("triggers.field_message")}
      labelClass="text-caption font-normal mb-0"
      hint={$t("triggers.field_message_hint")}
      optional
    >
      <Textarea
        id="trigger-input-template"
        rows={2}
        placeholder={$t("triggers.field_input_template_placeholder")}
        bind:value={message}
        data-testid="trigger-input-template-textarea"
      />
    </FormField>
  {/if}
</div>
