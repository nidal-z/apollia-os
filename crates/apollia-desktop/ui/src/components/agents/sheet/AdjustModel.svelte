<script lang="ts">
  /**
   * AdjustModel - the model chosen for the agent and its preset. "Default
   * model" follows the model of the machine; "Default preset" keeps the
   * settings of the chosen model.
   */
  import { t } from "svelte-i18n";
  import { Select } from "$lib/components/ui/select";
  import SheetSection from "./SheetSection.svelte";
  import {
    chooseModel,
    presetsOf,
    type AdjustDraft,
    type ModelOption,
    type PresetOffer,
  } from "./agentAdjust";

  interface Props {
    draft: AdjustDraft;
    options: ModelOption[];
    presets: PresetOffer[];
    modified: boolean;
  }

  let { draft, options, presets, modified }: Props = $props();

  const chosen = $derived(draft.model?.model_id ?? "");
  const offered = $derived(presetsOf(presets, draft.model));
  const preset = $derived(draft.model?.preset_id === null || draft.model === null ? "" : String(draft.model.preset_id));

  function onModel(event: Event): void {
    const id = (event.currentTarget as HTMLSelectElement).value;
    draft.model = chooseModel(options, draft.model, id);
  }

  function onPreset(event: Event): void {
    if (!draft.model) return;
    const raw = (event.currentTarget as HTMLSelectElement).value;
    draft.model = { ...draft.model, preset_id: raw === "" ? null : Number(raw) };
  }
</script>

<SheetSection
  title={$t("agent_sheet.model_title")}
  legend={$t("agent_adjust.model_legend")}
  testid="agent-sheet-model"
  {modified}
>
  <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
    <label class="space-y-1">
      <span class="text-caption font-medium text-foreground">{$t("agent_adjust.model_label")}</span>
      <Select value={chosen} onchange={onModel} data-testid="agent-adjust-model">
        <option value="">{$t("agent_adjust.model_default")}</option>
        {#each options as option (option.id)}
          <option value={option.id}>
            {option.source === "local"
              ? $t("agent_adjust.model_option_local", { values: { name: option.id } })
              : $t("agent_adjust.model_option_server", { values: { name: option.id } })}
          </option>
        {/each}
        {#if draft.model && !options.some((o) => o.id === draft.model?.model_id)}
          <option value={draft.model.model_id}>{draft.model.model_id}</option>
        {/if}
      </Select>
    </label>
    <label class="space-y-1">
      <span class="text-caption font-medium text-foreground">{$t("agent_adjust.preset_label")}</span>
      <Select value={preset} onchange={onPreset} disabled={!draft.model} data-testid="agent-adjust-preset">
        <option value="">{$t("agent_adjust.preset_default")}</option>
        {#each offered as offer (offer.id)}
          <option value={String(offer.id)}>{offer.name}</option>
        {/each}
      </Select>
    </label>
  </div>
</SheetSection>
