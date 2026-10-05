<script lang="ts">
  /**
   * WorkflowLaunchDialog - the form that launches a workflow: one field per
   * input the workflow declares, in the declared order, empty or filled with
   * the inputs of a past run.
   *
   * A value the form can refuse on its own (a required input left empty, a
   * text where a number is expected) is refused before the launch, every
   * field at fault marked at once. An input the workflow engine refuses comes
   * back on its field, with the engine's sentence. The dialog stays open
   * until a run is launched.
   */
  import { t } from "svelte-i18n";
  import { Play } from "lucide-svelte";
  import { ErrorBanner } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import { reportError } from "$lib/errors/reportError";
  import type { WorkflowField } from "$lib/ipc/workflows";
  import WorkflowFieldControl from "./WorkflowFieldControl.svelte";
  import {
    formErrors,
    initialValues,
    parseRefusal,
    toInputs,
    type FieldErrorCode,
    type FieldValue,
    type FormValues,
  } from "./workflowForm";
  import { familyLabelKey, type WorkflowAccess } from "./workflowRuns";

  interface Props {
    open: boolean;
    /** The name of the workflow, as its sheet shows it. */
    name: string;
    fields: WorkflowField[];
    /** The inputs of a past run the form opens with, when launching again. */
    previous?: Record<string, unknown>;
    access: WorkflowAccess | null;
    /** Launches on the inputs; rejects with the engine's message. */
    onlaunch: (inputs: Record<string, unknown>) => Promise<void>;
    onclose: () => void;
  }

  let { open, name, fields, previous, access, onlaunch, onclose }: Props = $props();

  let values = $state<FormValues>({});
  let errors = $state<Record<string, FieldErrorCode>>({});
  /** The engine's sentence on the field it refused. */
  let refused = $state<{ field: string; message: string } | null>(null);
  /** A failure that names no input. */
  let failure = $state<string | null>(null);
  let launching = $state(false);

  $effect(() => {
    if (!open) return;
    values = initialValues(fields, previous);
    errors = {};
    refused = null;
    failure = null;
  });

  const errorCount = $derived(Object.keys(errors).length);

  function change(field: string, value: FieldValue): void {
    values = { ...values, [field]: value };
    if (errors[field]) {
      const { [field]: _cleared, ...rest } = errors;
      errors = rest;
    }
    if (refused?.field === field) refused = null;
  }

  async function launch(): Promise<void> {
    if (launching) return;
    failure = null;
    refused = null;
    errors = formErrors(fields, values);
    if (Object.keys(errors).length > 0) return;
    launching = true;
    try {
      await onlaunch(toInputs(fields, values));
      onclose();
    } catch (err) {
      const refusal = parseRefusal(String(err));
      if (refusal && fields.some((field) => field.name === refusal.field)) {
        errors = { [refusal.field]: refusal.code };
        refused = { field: refusal.field, message: refusal.message };
      } else {
        failure = reportError(err, { surface: "inline" }).friendly_message;
      }
    } finally {
      launching = false;
    }
  }
</script>

<Dialog
  {open}
  {onclose}
  size="lg"
  title={$t("workflows.launch_title", { values: { name } })}
  data-testid="workflow-launch-dialog"
>
  <p class="m-0 mb-4 text-body-xs text-muted-foreground">
    {fields.length === 0 ? $t("workflows.launch_no_inputs") : $t("workflows.launch_intro")}
  </p>

  {#if errorCount > 0}
    <ErrorBanner class="mb-4" data-testid="workflow-launch-errors">
      <p class="m-0 font-medium">
        {$t("workflows.launch_errors", { values: { count: errorCount } })}
      </p>
      {#if refused}
        <p class="m-0 text-caption opacity-90" data-testid="workflow-launch-refused">{refused.message}</p>
      {/if}
    </ErrorBanner>
  {/if}
  {#if failure}
    <ErrorBanner class="mb-4" data-testid="workflow-launch-failure">
      <p class="m-0 font-medium">{failure}</p>
    </ErrorBanner>
  {/if}

  <form
    class="grid grid-cols-1 gap-x-4 gap-y-3 sm:grid-cols-2"
    onsubmit={(e) => {
      e.preventDefault();
      void launch();
    }}
    data-testid="workflow-launch-form"
  >
    {#each fields as field (field.name)}
      <WorkflowFieldControl
        {field}
        value={values[field.name] ?? ""}
        error={errors[field.name] ?? null}
        onchange={(value) => change(field.name, value)}
      />
    {/each}
  </form>

  {#if access?.sensitive}
    <p class="m-0 mt-4 text-caption text-muted-foreground" data-testid="workflow-launch-access">
      {$t("workflows.launch_access", { values: { family: $t(familyLabelKey(access.family)) } })}
    </p>
  {/if}

  <DialogFooter>
    <Button variant="outline" onclick={onclose} data-testid="workflow-launch-cancel">
      {$t("common.cancel")}
    </Button>
    <Button variant="primary-solid" disabled={launching} onclick={launch} data-testid="workflow-launch-confirm">
      {#snippet icon()}<Play size={12} />{/snippet}
      {$t("workflows.launch")}
    </Button>
  </DialogFooter>
</Dialog>
