<script lang="ts">
  /**
   * CreateAgentForm - the short form an agent is created from: what it is
   * for, what it gives back, what it must never do, the requests it will
   * receive, what it may consult and what it may do. Only the first field is
   * needed; the agent workshop asks about what the others leave unsaid.
   *
   * The sources and the rights listed are those of the machine: a box never
   * names something that does not exist here. Nothing is stored.
   */
  import { t } from "svelte-i18n";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { FormField } from "$lib/components/ui/form-field";
  import { Textarea } from "$lib/components/ui/textarea";
  import type { DraftOffer } from "$lib/ipc/agentCreate";
  import { RIGHT_LABEL, offeredRights, type WorkshopForm } from "./agentDraft";

  interface Props {
    /** The form; the same object the launch reads. */
    form: WorkshopForm;
    /** What the machine holds; `null` while it is read or when it cannot be. */
    offer: DraftOffer | null;
  }

  let { form, offer }: Props = $props();

  const rights = $derived(offeredRights(offer));
  const sources = $derived(offer?.sources ?? []);

  function sourceLabel(id: string, label: string): string {
    if (id === "profile") return $t("agent_sheet.source_profile");
    if (id === "memory") return $t("agent_sheet.source_memory");
    return label;
  }

  function toggle(list: string[], value: string, on: boolean): string[] {
    const without = list.filter((entry) => entry !== value);
    return on ? [...without, value] : without;
  }
</script>

<div class="max-w-3xl space-y-5" data-testid="agent-create-form">
  <FormField
    label={$t("agent_create.purpose_label")}
    id="agent-create-purpose"
    hint={$t("agent_create.purpose_help")}
    required
  >
    <Textarea
      id="agent-create-purpose"
      bind:value={form.purpose}
      rows={4}
      placeholder={$t("agent_create.purpose_placeholder")}
      data-testid="agent-create-purpose"
    />
  </FormField>
  <FormField
    label={$t("agent_create.result_label")}
    id="agent-create-result"
    hint={$t("agent_create.result_help")}
    optional
  >
    <Textarea
      id="agent-create-result"
      bind:value={form.result}
      rows={2}
      placeholder={$t("agent_create.result_placeholder")}
      data-testid="agent-create-result"
    />
  </FormField>
  <FormField
    label={$t("agent_create.rules_label")}
    id="agent-create-rules"
    hint={$t("agent_create.rules_help")}
    optional
  >
    <Textarea
      id="agent-create-rules"
      bind:value={form.rules}
      rows={2}
      placeholder={$t("agent_create.rules_placeholder")}
      data-testid="agent-create-rules"
    />
  </FormField>
  <FormField
    label={$t("agent_create.examples_label")}
    id="agent-create-examples"
    hint={$t("agent_create.examples_help")}
    optional
  >
    <Textarea
      id="agent-create-examples"
      bind:value={form.examples}
      rows={2}
      placeholder={$t("agent_create.examples_placeholder")}
      data-testid="agent-create-examples"
    />
  </FormField>
  {#if sources.length > 0}
    <fieldset class="m-0 space-y-1.5 border-0 p-0" data-testid="agent-create-sources">
      <legend class="p-0 text-body-sm font-medium text-foreground">{$t("agent_create.sources_label")}</legend>
      <p class="m-0 text-caption text-muted-foreground">{$t("agent_create.sources_help")}</p>
      <ul class="m-0 list-none space-y-1 p-0">
        {#each sources as source (source.id)}
          <li class="flex items-center gap-2">
            <Checkbox
              id="agent-create-source-{source.id}"
              checked={form.sources.includes(source.id)}
              onchange={(on) => (form.sources = toggle(form.sources, source.id, on))}
              data-testid="agent-create-source"
            />
            <label for="agent-create-source-{source.id}" class="text-body-xs text-foreground">
              {sourceLabel(source.id, source.label)}
            </label>
          </li>
        {/each}
      </ul>
    </fieldset>
  {/if}
  {#if rights.length > 0}
    <fieldset class="m-0 space-y-1.5 border-0 p-0" data-testid="agent-create-rights">
      <legend class="p-0 text-body-sm font-medium text-foreground">{$t("agent_create.rights_label")}</legend>
      <p class="m-0 text-caption text-muted-foreground">{$t("agent_create.rights_help")}</p>
      <ul class="m-0 list-none space-y-1 p-0">
        {#each rights as family (family)}
          <li class="flex items-center gap-2">
            <Checkbox
              id="agent-create-right-{family}"
              checked={form.rights.includes(family)}
              onchange={(on) => (form.rights = toggle(form.rights, family, on))}
              data-testid="agent-create-right"
            />
            <label for="agent-create-right-{family}" class="text-body-xs text-foreground">
              {$t(RIGHT_LABEL[family])}
            </label>
          </li>
        {/each}
      </ul>
    </fieldset>
  {/if}
</div>
