<script lang="ts">
  /**
   * "Apply to the agent": the agent receives the model of the conversation
   * and its preset. Settings changed over the preset are kept as a new
   * preset first. The instructions of the conversation reach the mission of
   * the agent only when the box is checked, after the mission obtained is
   * shown.
   */
  import { t } from "svelte-i18n";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Input } from "$lib/components/ui/input";
  import { FormField } from "$lib/components/ui/form-field";
  import { getAgentDefinition } from "$lib/ipc/agentSheet";
  import { setAgentModel, setAgentOverride } from "$lib/ipc/agentAdjust";
  import { createModelPreset } from "$lib/ipc/chatSettings";
  import {
    applyToAgentSteps,
    missionWithInstructions,
    runApplySteps,
  } from "$lib/chat/sessionSettings";
  import type { SessionSettingsController } from "./useSessionSettings.svelte";

  interface Props {
    open: boolean;
    agentId: string;
    agentName: string;
    controller: SessionSettingsController;
    onclose: () => void;
    onapplied: () => void;
  }

  let { open, agentId, agentName, controller, onclose, onapplied }: Props = $props();

  let mission = $state<string | null>(null);
  let addInstructions = $state(false);
  let presetName = $state("");
  let busy = $state(false);
  let failure = $state<string | null>(null);

  const instructions = $derived(controller.settings.instructions ?? "");
  const createsPreset = $derived(controller.modified.length > 0);
  const temperature = $derived(
    controller.settings.temperature ?? controller.preset?.request?.temperature,
  );
  const maxTokens = $derived(
    controller.settings.max_tokens ?? controller.preset?.request?.max_tokens,
  );
  const ready = $derived(
    controller.model !== null &&
      mission !== null &&
      (!createsPreset || presetName.trim() !== ""),
  );

  $effect(() => {
    if (!open) return;
    addInstructions = false;
    failure = null;
    mission = null;
    presetName = $t("chat.session_settings.apply.preset_name", { values: { agent: agentName } });
    let cancelled = false;
    getAgentDefinition(agentId)
      .then((agent) => {
        if (!cancelled) mission = agent?.mission ?? "";
      })
      .catch((e: unknown) => {
        if (!cancelled) failure = e instanceof Error ? e.message : String(e);
      });
    return () => {
      cancelled = true;
    };
  });

  async function apply(): Promise<void> {
    const model = controller.model;
    if (!model || mission === null || busy) return;
    busy = true;
    failure = null;
    try {
      const steps = applyToAgentSteps({
        settings: controller.settings,
        preset: controller.preset,
        model,
        presetName,
        mission,
        addInstructions,
      });
      await runApplySteps(agentId, steps, {
        createPreset: createModelPreset,
        setModel: (agent, to, preset) =>
          setAgentModel(agent, to.model_source, to.model_id, preset),
        setMission: async (agent, text) => {
          await setAgentOverride(agent, "mission", text);
        },
      });
      await controller.refreshPresets();
      onapplied();
      onclose();
    } catch (e: unknown) {
      failure = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog
  {open}
  {onclose}
  size="md"
  title={$t("chat.session_settings.apply.title", { values: { agent: agentName } })}
  data-testid="session-apply-dialog"
>
  <div class="space-y-4">
    {#if controller.model === null}
      <p class="text-xs text-muted-foreground" data-testid="session-apply-no-model">
        {$t("chat.session_settings.apply.no_model")}
      </p>
    {:else}
      <div class="space-y-1.5">
        <p class="text-xs font-medium text-foreground">
          {$t("chat.session_settings.apply.receives")}
        </p>
        <ul class="space-y-1 text-xs text-muted-foreground">
          <li>
            {$t("chat.session_settings.apply.model", {
              values: { model: controller.model?.model_id ?? "" },
            })}
          </li>
          <li>
            {#if createsPreset}
              {$t("chat.session_settings.apply.preset_created")}
            {:else if controller.preset}
              {$t("chat.session_settings.apply.preset_existing", {
                values: { preset: controller.preset.name },
              })}
            {:else}
              {$t("chat.session_settings.apply.preset_none")}
            {/if}
          </li>
          {#if temperature !== undefined}
            <li>
              {$t("chat.session_settings.apply.temperature", { values: { value: temperature } })}
            </li>
          {/if}
          {#if maxTokens !== undefined}
            <li>
              {$t("chat.session_settings.apply.max_tokens", { values: { value: maxTokens } })}
            </li>
          {/if}
        </ul>
        <p class="text-micro text-muted-foreground">
          {$t("chat.session_settings.apply.not_applied")}
        </p>
      </div>
    {/if}

    {#if createsPreset}
      <FormField id="session-apply-preset-name" label={$t("chat.session_settings.preset.name_label")}>
        <Input
          id="session-apply-preset-name"
          bind:value={presetName}
          maxlength={80}
          data-testid="session-apply-preset-name"
        />
      </FormField>
    {/if}

    {#if instructions !== ""}
      <div class="space-y-2">
        <div class="flex items-start gap-2.5">
          <Checkbox
            id="session-apply-instructions"
            checked={addInstructions}
            onchange={(checked) => (addInstructions = checked)}
            class="mt-0.5"
            data-testid="session-apply-instructions"
          />
          <label for="session-apply-instructions" class="text-xs text-foreground">
            {$t("chat.session_settings.apply.add_instructions")}
          </label>
        </div>
        <div class="space-y-1">
          <p class="text-micro font-medium text-muted-foreground">
            {addInstructions
              ? $t("chat.session_settings.apply.mission_after")
              : $t("chat.session_settings.apply.mission_unchanged")}
          </p>
          <pre
            class="max-h-48 overflow-y-auto whitespace-pre-wrap rounded-md border border-border bg-surface-1 px-3 py-2 font-sans text-xs text-foreground"
            data-testid="session-apply-mission-preview"
            data-added={addInstructions ? "true" : "false"}>{mission === null
              ? $t("common.loading")
              : addInstructions
                ? missionWithInstructions(mission, instructions)
                : mission}</pre>
        </div>
      </div>
    {/if}

    {#if failure}
      <p class="text-xs text-destructive" role="alert" data-testid="session-apply-error">
        {failure}
      </p>
    {/if}
  </div>

  <DialogFooter>
    <Button variant="ghost" onclick={onclose} data-testid="session-apply-cancel">
      {$t("common.cancel")}
    </Button>
    <Button
      variant="primary-solid"
      onclick={() => void apply()}
      disabled={!ready}
      loading={busy}
      data-testid="session-apply-confirm"
    >
      {$t("chat.session_settings.apply.confirm")}
    </Button>
  </DialogFooter>
</Dialog>
