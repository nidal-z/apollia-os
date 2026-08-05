<script lang="ts">
  /**
   * Edit an existing automation in place.
   *
   * Loads the stored definition (`get_trigger_definition`), shows it in the
   * shared definition form, and writes it back with `update_trigger`. Changing
   * a schedule therefore no longer means delete plus recreate, which used to
   * invalidate the webhook HMAC secret and force the calling service to be
   * reconfigured.
   *
   * The stored webhook secret never crosses the IPC boundary: the host strips
   * it from the definition and reports its presence with a boolean. Leaving the
   * field empty keeps it, filling it replaces it.
   */
  import { t } from "svelte-i18n";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { SkeletonList } from "$lib/components/operator";
  import type { AgentListItem } from "$lib/types";
  import { getTriggerDefinition, updateTrigger } from "$lib/ipc/triggers";
  import { listAgents } from "$lib/ipc/connections";
  import AutomationDefinitionForm from "./AutomationDefinitionForm.svelte";
  import {
    emptyAutomationModel,
    isModelValid,
    modelFromDefinition,
    toUpdateRequest,
    validateModel,
    type AutomationFormErrors,
    type AutomationFormModel,
  } from "./automationDefinition";

  interface Props {
    open: boolean;
    /** Identifier of the automation to edit. */
    triggerId: string;
    onclose: () => void;
    /** Fired after a successful write, with the edited identifier. */
    onsaved: (id: string) => void;
  }

  let { open, triggerId, onclose, onsaved }: Props = $props();

  let model = $state<AutomationFormModel>(emptyAutomationModel());
  let agents = $state<AgentListItem[]>([]);
  let loaded = $state(false);
  let loadError = $state<string | null>(null);
  let notEditable = $state(false);
  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let touched = $state(false);

  const NO_ERRORS: AutomationFormErrors = { id: null, target: null, source: null };
  const liveErrors = $derived(validateModel(model, "edit"));
  const shownErrors = $derived(touched ? liveErrors : NO_ERRORS);
  const canSave = $derived(loaded && isModelValid(liveErrors));

  $effect(() => {
    if (open && triggerId) {
      void load(triggerId);
    }
  });

  async function load(id: string): Promise<void> {
    loaded = false;
    loadError = null;
    notEditable = false;
    saveError = null;
    touched = false;
    try {
      const definition = await getTriggerDefinition(id);
      model = modelFromDefinition(definition);
      loaded = true;
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      // A trigger declared in apollia.toml runs in the engine but has no row in
      // the definition store, so the CRUD read answers 404. It is not an error
      // the operator can act on from here.
      notEditable = message.includes("(404)");
      loadError = message;
    }

    try {
      agents = await listAgents();
    } catch {
      agents = [];
    }
  }

  async function handleSave(): Promise<void> {
    touched = true;
    if (!canSave || saving) return;
    const request = toUpdateRequest(model);
    if (!request) return;
    saving = true;
    saveError = null;
    try {
      await updateTrigger(model.id, request);
      onsaved(model.id);
    } catch (err: unknown) {
      saveError = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }
</script>

<Dialog
  {open}
  {onclose}
  size="md"
  title={$t("triggers.edit_trigger")}
  data-testid="automation-edit-dialog"
>
  {#if !loaded && !loadError}
    <!-- Covers both the frame before the load starts and the load itself, so
         the fields never flash empty before the definition arrives. -->
    <div data-testid="automation-edit-loading">
      <SkeletonList count={4} />
    </div>
  {:else if notEditable}
    <div class="space-y-2" data-testid="automation-edit-not-editable">
      <p class="text-sm text-foreground">{$t("triggers.edit_not_editable_title")}</p>
      <p class="text-caption text-muted-foreground">{$t("triggers.edit_not_editable_hint")}</p>
    </div>
  {:else if loadError}
    <div class="space-y-2" data-testid="automation-edit-load-error">
      <p class="text-sm text-destructive">{$t("triggers.edit_load_error")}</p>
      <p class="text-caption text-muted-foreground break-words">{loadError}</p>
    </div>
  {:else}
    <AutomationDefinitionForm
      bind:model
      mode="edit"
      {agents}
      errors={shownErrors}
      testidPrefix="automation-edit"
    />
    {#if saveError}
      <p class="mt-3 text-sm text-destructive" data-testid="automation-edit-error">
        {$t("triggers.update_error", { values: { message: saveError } })}
      </p>
    {/if}
  {/if}

  <DialogFooter>
    <Button variant="outline" size="sm" onclick={onclose} data-testid="automation-edit-cancel">
      {$t("common.cancel")}
    </Button>
    {#if loaded}
      <Button
        size="sm"
        onclick={handleSave}
        disabled={saving || (touched && !canSave)}
        data-testid="automation-edit-submit"
      >
        {saving ? $t("triggers.saving") : $t("common.save")}
      </Button>
    {/if}
  </DialogFooter>
</Dialog>
