<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type {
    AgentListItem,
    CreatePipelineRequest,
    PipelineStepInput,
    PipelineDefinitionView,
  } from "$lib/types";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Dialog } from "$lib/components/ui/dialog";

  interface Props {
    open: boolean;
    onclose: () => void;
    oncreated: (id: string) => void;
  }

  let { open, onclose, oncreated }: Props = $props();

  let agents = $state<AgentListItem[]>([]);

  let pipelineId = $state("");
  let description = $state("");
  let onFailure = $state<"fail" | "continue">("fail");
  let enabled = $state(true);
  let steps = $state<StepFormData[]>([]);

  let submitting = $state(false);
  let submitError = $state<string | null>(null);
  let touched = $state(false);

  interface StepFormData {
    id: string;
    agent: string;
    input: string;
    dependsOn: string[];
    onFailure: "fail" | "skip" | "fallback";
    fallbackFor: string;
    conditionOpen: boolean;
    conditionWhen: "contains" | "equals" | "starts_with" | "ends_with" | "regex";
    conditionField: string;
    conditionValue: string;
  }

  const PIPELINE_ID_PATTERN = /^[a-z0-9][a-z0-9-]*[a-z0-9]$|^[a-z0-9]$/;

  const idError = $derived.by(() => {
    if (!touched) return null;
    if (!pipelineId.trim()) return $t("pipelines.field_id_required");
    if (!PIPELINE_ID_PATTERN.test(pipelineId)) return $t("pipelines.field_id_invalid");
    return null;
  });

  const stepsError = $derived.by(() => {
    if (!touched) return null;
    if (steps.length === 0) return $t("pipelines.field_steps_required");
    return null;
  });

  const stepIdErrors = $derived.by(() => {
    const errors: (string | null)[] = [];
    const seen = new Set<string>();
    for (const step of steps) {
      if (!touched) {
        errors.push(null);
        continue;
      }
      if (!step.id.trim()) {
        errors.push($t("pipelines.field_step_id_required"));
      } else if (seen.has(step.id.trim())) {
        errors.push($t("pipelines.field_step_id_duplicate"));
      } else {
        errors.push(null);
      }
      seen.add(step.id.trim());
    }
    return errors;
  });

  const cycleError = $derived.by(() => {
    return detectCycle(steps);
  });

  const dependsOnError = $derived.by(() => {
    if (!touched) return null;
    const ids = new Set(steps.map((s) => s.id.trim()).filter(Boolean));
    for (const step of steps) {
      for (const dep of step.dependsOn) {
        if (!ids.has(dep)) {
          return $t("pipelines.field_depends_on_invalid", { values: { dep } });
        }
      }
    }
    return null;
  });

  const isValid = $derived(
    !!pipelineId.trim() &&
    PIPELINE_ID_PATTERN.test(pipelineId) &&
    steps.length > 0 &&
    stepIdErrors.every((e) => e === null) &&
    !cycleError &&
    !dependsOnError &&
    steps.every((s) => !!s.agent)
  );

  function detectCycle(stepsData: StepFormData[]): string | null {
    const ids = stepsData.map((s) => s.id.trim()).filter(Boolean);
    const idSet = new Set(ids);
    const inDegree = new Map<string, number>();
    const adj = new Map<string, string[]>();

    for (const id of ids) {
      inDegree.set(id, 0);
      adj.set(id, []);
    }

    for (const step of stepsData) {
      const stepId = step.id.trim();
      if (!stepId) continue;
      for (const dep of step.dependsOn) {
        if (idSet.has(dep)) {
          adj.get(dep)?.push(stepId);
          inDegree.set(stepId, (inDegree.get(stepId) ?? 0) + 1);
        }
      }
    }

    const queue: string[] = [];
    for (const [id, deg] of inDegree) {
      if (deg === 0) queue.push(id);
    }

    let visited = 0;
    while (queue.length > 0) {
      const node = queue.shift()!;
      visited++;
      for (const neighbor of adj.get(node) ?? []) {
        const newDeg = (inDegree.get(neighbor) ?? 1) - 1;
        inDegree.set(neighbor, newDeg);
        if (newDeg === 0) queue.push(neighbor);
      }
    }

    if (visited < ids.length) {
      return $t("pipelines.field_cycle_detected");
    }
    return null;
  }

  function createEmptyStep(): StepFormData {
    return {
      id: "",
      agent: "",
      input: "",
      dependsOn: [],
      onFailure: "fail",
      fallbackFor: "",
      conditionOpen: false,
      conditionWhen: "contains",
      conditionField: "",
      conditionValue: "",
    };
  }

  function addStep() {
    steps = [...steps, createEmptyStep()];
  }

  function removeStep(index: number) {
    steps = steps.filter((_, i) => i !== index);
  }

  function toggleDependency(stepIndex: number, depId: string) {
    const step = steps[stepIndex];
    if (step.dependsOn.includes(depId)) {
      step.dependsOn = step.dependsOn.filter((d) => d !== depId);
    } else {
      step.dependsOn = [...step.dependsOn, depId];
    }
    steps = [...steps];
  }

  function buildSteps(): PipelineStepInput[] {
    return steps.map((s) => {
      const step: PipelineStepInput = {
        id: s.id.trim(),
        agent: s.agent,
        input: s.input,
      };
      if (s.dependsOn.length > 0) step.depends_on = s.dependsOn;
      if (s.onFailure !== "fail") step.on_failure = s.onFailure;
      if (s.onFailure === "fallback" && s.fallbackFor) step.fallback_for = s.fallbackFor;
      if (s.conditionOpen && s.conditionField.trim()) {
        step.condition = {
          when: s.conditionWhen,
          field: s.conditionField.trim(),
          value: s.conditionValue.trim(),
        };
      }
      return step;
    });
  }

  async function handleSubmit(): Promise<void> {
    touched = true;
    if (!isValid) return;

    submitting = true;
    submitError = null;
    try {
      const definition: CreatePipelineRequest = {
        id: pipelineId.trim(),
        steps: buildSteps(),
      };
      if (description.trim()) definition.description = description.trim();
      if (onFailure !== "fail") definition.on_failure = onFailure;
      definition.enabled = enabled;

      await invoke<PipelineDefinitionView>("create_pipeline", { definition });
      oncreated(pipelineId.trim());
      onclose();
    } catch (err: unknown) {
      submitError = err instanceof Error ? err.message : String(err);
    } finally {
      submitting = false;
    }
  }

  function resetForm() {
    pipelineId = "";
    description = "";
    onFailure = "fail";
    enabled = true;
    steps = [];
    submitting = false;
    submitError = null;
    touched = false;
  }

  async function loadAgents(): Promise<void> {
    try {
      agents = await invoke("list_agents");
    } catch {
      agents = [];
    }
  }

  $effect(() => {
    if (open) {
      resetForm();
      void loadAgents();
    }
  });

  function otherStepIds(currentIndex: number): string[] {
    return steps
      .map((s, i) => (i !== currentIndex ? s.id.trim() : ""))
      .filter(Boolean);
  }
</script>

<Dialog open={open} onclose={onclose} size="lg" title={$t("pipelines.create_pipeline")} data-testid="create-pipeline-dialog">
  <div class="space-y-4">
    <!-- ID -->
    <div>
      <label class="mb-1 block text-[11px] text-muted-foreground" for="pipeline-id">{$t("pipelines.field_id")}</label>
      <Input
        id="pipeline-id"
        class={idError ? "border-destructive" : ""}
        placeholder={$t("pipelines.field_id_placeholder")}
        bind:value={pipelineId}
        data-testid="pipeline-create-input-name"
      />
      <p class="mt-0.5 text-xs text-muted-foreground">{$t("pipelines.field_id_help")}</p>
      {#if idError}
        <p class="mt-0.5 text-xs text-destructive" data-testid="pipeline-id-error">{idError}</p>
      {/if}
    </div>

    <!-- Description -->
    <div>
      <label class="mb-1 block text-[11px] text-muted-foreground" for="pipeline-description">
        {$t("pipelines.field_description")}
        <span>({$t("pipelines.input_json_optional")})</span>
      </label>
      <Input
        id="pipeline-description"
        bind:value={description}
        data-testid="pipeline-create-input-desc"
      />
    </div>

    <!-- On failure + Enabled -->
    <div class="flex items-center gap-6">
      <div>
        <label class="mb-1 block text-[11px] text-muted-foreground" for="pipeline-on-failure">{$t("pipelines.def_on_failure")}</label>
        <Select
          id="pipeline-on-failure"
          bind:value={onFailure}
          data-testid="pipeline-on-failure-select"
        >
          <option value="fail">fail</option>
          <option value="continue">continue</option>
        </Select>
      </div>
      <label class="flex items-center gap-2 text-[11px] text-muted-foreground">
        <Checkbox bind:checked={enabled} data-testid="pipeline-create-checkbox-enabled" />
        {$t("pipelines.def_enabled")}
      </label>
    </div>

    <!-- Steps section -->
    <div>
      <div class="mb-2 flex items-center justify-between">
        <p class="block text-[11px] text-muted-foreground">{$t("pipelines.field_steps_label")}</p>
        <Button size="sm" variant="outline" onclick={addStep} data-testid="add-step-btn">
          {$t("pipelines.add_step")}
        </Button>
      </div>

      {#if stepsError}
        <p class="mb-2 text-xs text-destructive" data-testid="pipeline-steps-error">{stepsError}</p>
      {/if}

      {#if cycleError}
        <p class="mb-2 text-xs text-destructive" data-testid="pipeline-cycle-error">{cycleError}</p>
      {/if}

      {#if dependsOnError}
        <p class="mb-2 text-xs text-destructive" data-testid="pipeline-depends-error">{dependsOnError}</p>
      {/if}

      <div class="space-y-3">
        {#each steps as step, index}
          <div class="rounded-md border bg-muted/30 p-3" data-testid="pipeline-step-{index}">
            <div class="mb-2 flex items-center justify-between">
              <span class="text-xs font-medium text-muted-foreground">{$t("pipelines.step_label")} {index + 1}</span>
              <button
                class="text-xs text-destructive hover:underline"
                onclick={() => removeStep(index)}
                data-testid="pipeline-step-{index}-remove"
              >
                {$t("pipelines.remove_step")}
              </button>
            </div>

            <div class="space-y-2">
              <!-- Step ID -->
              <div>
                <label class="mb-0.5 block text-[11px] text-muted-foreground" for="step-{index}-id">{$t("pipelines.field_step_id")}</label>
                <Input
                  id="step-{index}-id"
                  class="h-8 px-2 py-1.5 text-xs {stepIdErrors[index] ? 'border-destructive' : ''}"
                  placeholder="extract, validate, generate..."
                  bind:value={step.id}
                  data-testid="pipeline-step-{index}-id"
                />
                {#if stepIdErrors[index]}
                  <p class="mt-0.5 text-xs text-destructive">{stepIdErrors[index]}</p>
                {/if}
              </div>

              <!-- Agent select -->
              <div>
                <label class="mb-0.5 block text-[11px] text-muted-foreground" for="step-{index}-agent">{$t("pipelines.field_step_agent")}</label>
                <Select
                  id="step-{index}-agent"
                  class="w-full px-2 py-1.5 text-xs"
                  bind:value={step.agent}
                  data-testid="pipeline-step-{index}-agent"
                >
                  <option value="" disabled>— {$t("pipelines.field_step_agent")} —</option>
                  {#each agents as agent}
                    <option value={agent.name}>{agent.name}</option>
                  {/each}
                </Select>
              </div>

              <!-- Input -->
              <div>
                <label class="mb-0.5 block text-[11px] text-muted-foreground" for="step-{index}-input">{$t("pipelines.field_step_input")}</label>
                <Textarea
                  id="step-{index}-input"
                  class="px-2 py-1.5 text-xs"
                  rows={2}
                  bind:value={step.input}
                  data-testid="pipeline-step-{index}-input"
                />
              </div>

              <!-- Depends on (multi-select checkboxes) -->
              {#if otherStepIds(index).length > 0}
                <div>
                  <p class="mb-0.5 block text-[11px] text-muted-foreground">{$t("pipelines.field_depends_on")}</p>
                  <div class="flex flex-wrap gap-2">
                    {#each otherStepIds(index) as depId}
                      <label class="flex items-center gap-1 text-xs">
                        <Checkbox
                          checked={step.dependsOn.includes(depId)}
                          onchange={() => toggleDependency(index, depId)}
                          data-testid="pipeline-step-{index}-dep-{depId}"
                        />
                        {depId}
                      </label>
                    {/each}
                  </div>
                </div>
              {/if}

              <!-- On failure -->
              <div class="flex items-center gap-4">
                <div>
                  <label class="mb-0.5 block text-[11px] text-muted-foreground" for="step-{index}-on-failure">{$t("pipelines.def_on_failure")}</label>
                  <Select
                    id="step-{index}-on-failure"
                    class="px-2 py-1.5 text-xs"
                    bind:value={step.onFailure}
                    data-testid="pipeline-step-{index}-on-failure"
                  >
                    <option value="fail">fail</option>
                    <option value="skip">skip</option>
                    <option value="fallback">fallback</option>
                  </Select>
                </div>

                {#if step.onFailure === "fallback"}
                  <div>
                    <label class="mb-0.5 block text-[11px] text-muted-foreground" for="step-{index}-fallback-for">{$t("pipelines.field_fallback_for")}</label>
                    <Select
                      id="step-{index}-fallback-for"
                      class="px-2 py-1.5 text-xs"
                      bind:value={step.fallbackFor}
                      data-testid="pipeline-step-{index}-fallback-for"
                    >
                      <option value="" disabled>—</option>
                      {#each otherStepIds(index) as sid}
                        <option value={sid}>{sid}</option>
                      {/each}
                    </Select>
                  </div>
                {/if}
              </div>

              <!-- Condition (collapsible) -->
              <div>
                <button
                  class="text-xs text-muted-foreground hover:text-foreground"
                  onclick={() => { step.conditionOpen = !step.conditionOpen; steps = [...steps]; }}
                  data-testid="pipeline-step-{index}-condition-toggle"
                >
                  {step.conditionOpen ? "▾" : "▸"} {$t("pipelines.field_condition")}
                </button>
                {#if step.conditionOpen}
                  <div class="mt-1 flex gap-2">
                    <Select
                      class="px-2 py-1 text-xs"
                      bind:value={step.conditionWhen}
                      aria-label={$t("pipelines.field_condition_operator")}
                      data-testid="pipeline-step-{index}-condition-when"
                    >
                      <option value="contains">contains</option>
                      <option value="equals">equals</option>
                      <option value="starts_with">starts_with</option>
                      <option value="ends_with">ends_with</option>
                      <option value="regex">regex</option>
                    </Select>
                    <Input
                      class="h-8 flex-1 px-2 py-1 text-xs"
                      placeholder="field"
                      aria-label={$t("pipelines.field_condition_field")}
                      bind:value={step.conditionField}
                      data-testid="pipeline-step-{index}-condition-field"
                    />
                    <Input
                      class="h-8 flex-1 px-2 py-1 text-xs"
                      placeholder="value"
                      aria-label={$t("pipelines.field_condition_value")}
                      bind:value={step.conditionValue}
                      data-testid="pipeline-step-{index}-condition-value"
                    />
                  </div>
                {/if}
              </div>
            </div>
          </div>
        {/each}
      </div>
    </div>

    <!-- Submit error -->
    {#if submitError}
      <p class="text-sm text-destructive" data-testid="create-pipeline-error">{submitError}</p>
    {/if}

    <!-- Actions -->
    <div class="flex justify-end gap-2">
      <Button variant="outline" size="sm" onclick={onclose} data-testid="create-pipeline-cancel-btn">
        {$t("common.cancel")}
      </Button>
      <Button
        size="sm"
        onclick={handleSubmit}
        disabled={submitting || (touched && !isValid) || !!cycleError}
        data-testid="create-pipeline-submit-btn"
      >
        {submitting ? $t("pipelines.creating") : $t("pipelines.create_pipeline")}
      </Button>
    </div>
  </div>
</Dialog>
