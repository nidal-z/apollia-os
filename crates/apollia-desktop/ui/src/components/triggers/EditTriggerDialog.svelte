<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type {
    AgentListItem,
    PipelineInfo,
    UpdateTriggerRequest,
    TriggerSourceInput,
    TriggerDefinitionView,
  } from "$lib/types";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    open: boolean;
    triggerId: string;
    onclose: () => void;
    onupdated: (id: string) => void;
  }

  let { open, triggerId, onclose, onupdated }: Props = $props();

  type SourceType = "cron" | "interval" | "oneshot" | "file_watch" | "webhook";
  type TargetKind = "agent" | "pipeline";

  let agents = $state<AgentListItem[]>([]);
  let pipelines = $state<PipelineInfo[]>([]);
  let loading = $state(false);
  let loadError = $state<string | null>(null);

  let targetKind = $state<TargetKind>("agent");
  let selectedAgent = $state("");
  let selectedPipeline = $state("");
  let sourceType = $state<SourceType>("cron");
  let enabled = $state(true);
  let onBusy = $state<"queue" | "drop">("queue");
  let inputTemplate = $state("");

  let cronSchedule = $state("");
  let intervalEvery = $state("");
  let oneshotFireAt = $state("");
  let fileWatchPath = $state("");
  let fileWatchCreate = $state(true);
  let fileWatchModify = $state(true);
  let fileWatchDelete = $state(false);
  let webhookSecret = $state("");

  let submitting = $state(false);
  let submitError = $state<string | null>(null);
  let touched = $state(false);

  const targetError = $derived.by(() => {
    if (!touched) return null;
    if (targetKind === "agent" && !selectedAgent) return $t("triggers.field_target_required");
    if (targetKind === "pipeline" && !selectedPipeline) return $t("triggers.field_target_required");
    return null;
  });

  const sourceError = $derived.by(() => {
    if (!touched) return null;
    if (sourceType === "cron" && !cronSchedule.trim()) return $t("triggers.field_schedule_required");
    if (sourceType === "interval" && !intervalEvery.trim()) return $t("triggers.field_every_required");
    if (sourceType === "oneshot" && !oneshotFireAt) return $t("triggers.field_fire_at_required");
    if (sourceType === "file_watch" && !fileWatchPath.trim()) return $t("triggers.field_path_required");
    if (sourceType === "file_watch" && !fileWatchCreate && !fileWatchModify && !fileWatchDelete) {
      return $t("triggers.field_events_required");
    }
    if (sourceType === "webhook" && !webhookSecret.trim()) return $t("triggers.field_secret_required");
    return null;
  });

  const isValid = $derived(
    ((targetKind === "agent" && !!selectedAgent) || (targetKind === "pipeline" && !!selectedPipeline)) &&
    !sourceError
  );

  function populateFromDefinition(def: TriggerDefinitionView) {
    if (def.agent) {
      targetKind = "agent";
      selectedAgent = def.agent;
      selectedPipeline = "";
    } else if (def.pipeline) {
      targetKind = "pipeline";
      selectedPipeline = def.pipeline;
      selectedAgent = "";
    }

    enabled = def.enabled;
    onBusy = def.on_busy;
    inputTemplate = def.input_template ?? "";
    sourceType = def.source_type;

    cronSchedule = "";
    intervalEvery = "";
    oneshotFireAt = "";
    fileWatchPath = "";
    fileWatchCreate = true;
    fileWatchModify = true;
    fileWatchDelete = false;
    webhookSecret = "";

    const cfg = def.source_config;
    switch (def.source_type) {
      case "cron":
        cronSchedule = (cfg.schedule as string) ?? (cfg.expression as string) ?? "";
        break;
      case "interval":
        intervalEvery = (cfg.every as string) ?? (cfg.seconds != null ? `${cfg.seconds}s` : "");
        break;
      case "oneshot":
        oneshotFireAt = (cfg.fire_at as string) ?? "";
        break;
      case "file_watch":
        fileWatchPath = (cfg.path as string) ?? "";
        if (Array.isArray(cfg.events)) {
          fileWatchCreate = cfg.events.includes("create");
          fileWatchModify = cfg.events.includes("modify");
          fileWatchDelete = cfg.events.includes("delete");
        }
        break;
      case "webhook":
        webhookSecret = (cfg.secret as string) ?? "";
        break;
    }
  }

  function buildSource(): TriggerSourceInput {
    switch (sourceType) {
      case "cron":
        return { type: "cron", schedule: cronSchedule.trim() };
      case "interval":
        return { type: "interval", every: intervalEvery.trim() };
      case "oneshot":
        return { type: "oneshot", fire_at: oneshotFireAt };
      case "file_watch": {
        const events: string[] = [];
        if (fileWatchCreate) events.push("create");
        if (fileWatchModify) events.push("modify");
        if (fileWatchDelete) events.push("delete");
        return { type: "file_watch", path: fileWatchPath.trim(), events };
      }
      case "webhook":
        return { type: "webhook", secret: webhookSecret.trim() };
    }
  }

  async function handleSubmit(): Promise<void> {
    touched = true;
    if (!isValid) return;

    submitting = true;
    submitError = null;
    try {
      const definition: UpdateTriggerRequest = {
        enabled,
        on_busy: onBusy,
        source: buildSource(),
      };
      if (targetKind === "agent") {
        definition.agent = selectedAgent;
      } else {
        definition.pipeline = selectedPipeline;
      }
      if (inputTemplate.trim()) {
        definition.input_template = inputTemplate.trim();
      }

      await invoke<TriggerDefinitionView>("update_trigger", { id: triggerId, definition });
      onupdated(triggerId);
      onclose();
    } catch (err: unknown) {
      submitError = err instanceof Error ? err.message : String(err);
    } finally {
      submitting = false;
    }
  }

  function generateSecret() {
    const bytes = new Uint8Array(16);
    crypto.getRandomValues(bytes);
    webhookSecret = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
  }

  async function loadTrigger(): Promise<void> {
    loading = true;
    loadError = null;
    submitError = null;
    touched = false;
    try {
      const [agentsList, pipelinesList] = await Promise.all([
        invoke<AgentListItem[]>("list_agents").catch(() => [] as AgentListItem[]),
        invoke<PipelineInfo[]>("list_pipelines").catch(() => [] as PipelineInfo[]),
      ]);
      agents = agentsList;
      pipelines = pipelinesList;

      const def: TriggerDefinitionView = await invoke("get_trigger_definition", { id: triggerId });
      populateFromDefinition(def);
    } catch (err: unknown) {
      loadError = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      onclose();
    }
  }

  $effect(() => {
    if (open && triggerId) {
      void loadTrigger();
    }
  });
</script>

{#if open}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
    role="button"
    tabindex="-1"
    onclick={onclose}
    onkeydown={handleKeydown}
  >
    <div
      class="max-h-[85vh] w-[480px] overflow-y-auto rounded-lg border bg-background p-6 shadow-lg"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={handleKeydown}
      data-testid="edit-trigger-dialog"
    >
      <h3 class="mb-4 text-lg font-semibold">{$t("triggers.edit_trigger")}</h3>

      {#if loading}
        <p class="py-8 text-center text-sm text-muted-foreground">{$t("triggers.saving")}…</p>
      {:else if loadError}
        <div class="space-y-4">
          <p class="text-sm text-[hsl(var(--destructive))]">{loadError}</p>
          <div class="flex justify-end">
            <Button variant="outline" size="sm" onclick={onclose}>{$t("common.cancel")}</Button>
          </div>
        </div>
      {:else}
        <div class="space-y-4">
          <!-- ID (readonly) -->
          <div>
            <label class="mb-1 block text-sm font-medium" for="edit-trigger-id">{$t("triggers.field_id")}</label>
            <input
              id="edit-trigger-id"
              type="text"
              class="w-full rounded-md border bg-muted px-3 py-2 text-sm"
              value={triggerId}
              readonly
              data-testid="edit-trigger-id-input"
            />
          </div>

          <!-- Target: Agent / Pipeline -->
          <div>
            <label class="mb-1 block text-sm font-medium">{$t("triggers.field_target")}</label>
            <div class="mb-2 flex gap-3">
              <label class="flex items-center gap-1.5 text-sm">
                <input
                  type="radio"
                  name="edit-target-kind"
                  value="agent"
                  bind:group={targetKind}
                  data-testid="edit-trigger-target-agent-radio"
                />
                {$t("triggers.field_target_agent")}
              </label>
              <label class="flex items-center gap-1.5 text-sm">
                <input
                  type="radio"
                  name="edit-target-kind"
                  value="pipeline"
                  bind:group={targetKind}
                  data-testid="edit-trigger-target-pipeline-radio"
                />
                {$t("triggers.field_target_pipeline")}
              </label>
            </div>
            {#if targetKind === "agent"}
              <select
                class="w-full rounded-md border bg-background px-3 py-2 text-sm"
                bind:value={selectedAgent}
                data-testid="edit-trigger-agent-select"
              >
                <option value="" disabled>— {$t("triggers.field_target_agent")} —</option>
                {#each agents as agent}
                  <option value={agent.name}>{agent.name}</option>
                {/each}
              </select>
            {:else}
              <select
                class="w-full rounded-md border bg-background px-3 py-2 text-sm"
                bind:value={selectedPipeline}
                data-testid="edit-trigger-pipeline-select"
              >
                <option value="" disabled>— {$t("triggers.field_target_pipeline")} —</option>
                {#each pipelines as pipeline}
                  <option value={pipeline.id}>{pipeline.id}{pipeline.description ? ` — ${pipeline.description}` : ""}</option>
                {/each}
              </select>
            {/if}
            {#if targetError}
              <p class="mt-0.5 text-xs text-[hsl(var(--destructive))]">{targetError}</p>
            {/if}
          </div>

          <!-- Source type -->
          <div>
            <label class="mb-1 block text-sm font-medium" for="edit-trigger-source-type">{$t("triggers.field_source_type")}</label>
            <select
              id="edit-trigger-source-type"
              class="w-full rounded-md border bg-background px-3 py-2 text-sm"
              bind:value={sourceType}
              data-testid="edit-trigger-source-select"
            >
              <option value="cron">Cron</option>
              <option value="interval">Interval</option>
              <option value="oneshot">Oneshot</option>
              <option value="file_watch">File Watch</option>
              <option value="webhook">Webhook</option>
            </select>
          </div>

          <!-- Dynamic source fields -->
          {#if sourceType === "cron"}
            <div>
              <label class="mb-1 block text-sm font-medium" for="edit-trigger-schedule">{$t("triggers.field_schedule")}</label>
              <input
                id="edit-trigger-schedule"
                type="text"
                class="w-full rounded-md border bg-background px-3 py-2 font-mono text-sm"
                placeholder={$t("triggers.field_schedule_placeholder")}
                bind:value={cronSchedule}
                data-testid="edit-trigger-schedule-input"
              />
              <p class="mt-0.5 text-xs text-muted-foreground">{$t("triggers.field_schedule_help")}</p>
            </div>
          {:else if sourceType === "interval"}
            <div>
              <label class="mb-1 block text-sm font-medium" for="edit-trigger-every">{$t("triggers.field_every")}</label>
              <input
                id="edit-trigger-every"
                type="text"
                class="w-full rounded-md border bg-background px-3 py-2 font-mono text-sm"
                placeholder={$t("triggers.field_every_placeholder")}
                bind:value={intervalEvery}
                data-testid="edit-trigger-every-input"
              />
              <p class="mt-0.5 text-xs text-muted-foreground">{$t("triggers.field_every_help")}</p>
            </div>
          {:else if sourceType === "oneshot"}
            <div>
              <label class="mb-1 block text-sm font-medium" for="edit-trigger-fire-at">{$t("triggers.field_fire_at")}</label>
              <input
                id="edit-trigger-fire-at"
                type="datetime-local"
                class="w-full rounded-md border bg-background px-3 py-2 text-sm"
                bind:value={oneshotFireAt}
                data-testid="edit-trigger-fire-at-input"
              />
            </div>
          {:else if sourceType === "file_watch"}
            <div>
              <label class="mb-1 block text-sm font-medium" for="edit-trigger-watch-path">{$t("triggers.field_path")}</label>
              <input
                id="edit-trigger-watch-path"
                type="text"
                class="w-full rounded-md border bg-background px-3 py-2 text-sm"
                placeholder={$t("triggers.field_path_placeholder")}
                bind:value={fileWatchPath}
                data-testid="edit-trigger-path-input"
              />
            </div>
            <div>
              <label class="mb-1 block text-sm font-medium">{$t("triggers.field_events")}</label>
              <div class="flex gap-4">
                <label class="flex items-center gap-1.5 text-sm">
                  <input type="checkbox" bind:checked={fileWatchCreate} data-testid="edit-trigger-event-create" />
                  {$t("triggers.field_events_create")}
                </label>
                <label class="flex items-center gap-1.5 text-sm">
                  <input type="checkbox" bind:checked={fileWatchModify} data-testid="edit-trigger-event-modify" />
                  {$t("triggers.field_events_modify")}
                </label>
                <label class="flex items-center gap-1.5 text-sm">
                  <input type="checkbox" bind:checked={fileWatchDelete} data-testid="edit-trigger-event-delete" />
                  {$t("triggers.field_events_delete")}
                </label>
              </div>
            </div>
          {:else if sourceType === "webhook"}
            <div>
              <label class="mb-1 block text-sm font-medium" for="edit-trigger-secret">{$t("triggers.field_secret")}</label>
              <div class="flex gap-2">
                <input
                  id="edit-trigger-secret"
                  type="text"
                  class="flex-1 rounded-md border bg-background px-3 py-2 font-mono text-sm"
                  placeholder={$t("triggers.field_secret_placeholder")}
                  bind:value={webhookSecret}
                  data-testid="edit-trigger-secret-input"
                />
                <Button size="sm" variant="outline" onclick={generateSecret} data-testid="edit-trigger-secret-generate-btn">
                  {$t("triggers.field_secret_generate")}
                </Button>
              </div>
            </div>
          {/if}

          {#if sourceError}
            <p class="text-xs text-[hsl(var(--destructive))]">{sourceError}</p>
          {/if}

          <!-- Input template -->
          <div>
            <label class="mb-1 block text-sm font-medium" for="edit-trigger-input-template">
              {$t("triggers.field_input_template")}
              <span class="font-normal text-muted-foreground">({$t("pipelines.input_json_optional")})</span>
            </label>
            <textarea
              id="edit-trigger-input-template"
              class="w-full rounded-md border bg-background px-3 py-2 text-sm"
              rows="2"
              placeholder={$t("triggers.field_input_template_placeholder")}
              bind:value={inputTemplate}
              data-testid="edit-trigger-input-template-textarea"
            ></textarea>
          </div>

          <!-- On busy + Enabled -->
          <div class="flex items-center gap-6">
            <div>
              <label class="mb-1 block text-sm font-medium" for="edit-trigger-on-busy">{$t("triggers.field_on_busy")}</label>
              <select
                id="edit-trigger-on-busy"
                class="rounded-md border bg-background px-3 py-2 text-sm"
                bind:value={onBusy}
                data-testid="edit-trigger-on-busy-select"
              >
                <option value="queue">{$t("triggers.field_on_busy_queue")}</option>
                <option value="drop">{$t("triggers.field_on_busy_drop")}</option>
              </select>
            </div>
            <label class="flex items-center gap-2 text-sm">
              <input type="checkbox" bind:checked={enabled} data-testid="edit-trigger-enabled-toggle" />
              {$t("triggers.field_enabled")}
            </label>
          </div>

          <!-- Submit error -->
          {#if submitError}
            <p class="text-sm text-[hsl(var(--destructive))]" data-testid="edit-trigger-error">{submitError}</p>
          {/if}

          <!-- Actions -->
          <div class="flex justify-end gap-2">
            <Button variant="outline" size="sm" onclick={onclose} data-testid="edit-trigger-cancel-btn">
              {$t("common.cancel")}
            </Button>
            <Button
              size="sm"
              onclick={handleSubmit}
              disabled={submitting || (touched && !isValid)}
              data-testid="edit-trigger-submit-btn"
            >
              {submitting ? $t("triggers.saving") : $t("triggers.edit_trigger")}
            </Button>
          </div>
        </div>
      {/if}
    </div>
  </div>
{/if}
