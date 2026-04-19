<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { get } from "svelte/store";
  import { t } from "svelte-i18n";
  import { tourPrefill } from "$lib/stores/tour";
  import type {
    AgentListItem,
    PipelineInfo,
    CreateTriggerRequest,
    TriggerSourceInput,
    TriggerDefinitionView,
  } from "$lib/types";
  import { Clock } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { RadioItem } from "$lib/components/ui/radio";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Toggle } from "$lib/components/ui/toggle";
  import { Dialog } from "$lib/components/ui/dialog";
  import { DatePicker, TimePicker } from "$lib/components/ui/date-picker";

  interface Props {
    open: boolean;
    onclose: () => void;
    oncreated: (id: string) => void;
  }

  let { open, onclose, oncreated }: Props = $props();

  type SourceType = "cron" | "interval" | "oneshot" | "file_watch" | "webhook";
  type TargetKind = "agent" | "pipeline";

  let agents = $state<AgentListItem[]>([]);
  let pipelines = $state<PipelineInfo[]>([]);

  let triggerId = $state("");
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
  let oneshotDate = $state("");
  let oneshotTime = $state("");
  $effect(() => {
    oneshotFireAt = oneshotDate && oneshotTime ? `${oneshotDate}T${oneshotTime}` : "";
  });
  let fileWatchPath = $state("");
  let fileWatchCreate = $state(true);
  let fileWatchModify = $state(true);
  let fileWatchDelete = $state(false);
  let webhookSecret = $state("");

  let submitting = $state(false);
  let submitError = $state<string | null>(null);
  let touched = $state(false);

  const TRIGGER_ID_PATTERN = /^[a-z0-9][a-z0-9-]*[a-z0-9]$|^[a-z0-9]$/;

  const idError = $derived.by(() => {
    if (!touched) return null;
    if (!triggerId.trim()) return $t("triggers.field_id_required");
    if (!TRIGGER_ID_PATTERN.test(triggerId)) return $t("triggers.field_id_invalid");
    return null;
  });

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
    !!triggerId.trim() &&
    TRIGGER_ID_PATTERN.test(triggerId) &&
    ((targetKind === "agent" && !!selectedAgent) || (targetKind === "pipeline" && !!selectedPipeline)) &&
    !sourceError
  );

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
      const definition: CreateTriggerRequest = {
        id: triggerId.trim(),
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

      await invoke<TriggerDefinitionView>("create_trigger", { definition });
      oncreated(triggerId.trim());
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

  function resetForm() {
    triggerId = "";
    targetKind = "agent";
    selectedAgent = "";
    selectedPipeline = "";
    sourceType = "cron";
    enabled = true;
    onBusy = "queue";
    inputTemplate = "";
    cronSchedule = "";
    intervalEvery = "";
    oneshotFireAt = "";
    oneshotDate = "";
    oneshotTime = "";
    fileWatchPath = "";
    fileWatchCreate = true;
    fileWatchModify = true;
    fileWatchDelete = false;
    webhookSecret = "";
    submitting = false;
    submitError = null;
    touched = false;
  }

  async function loadOptions(): Promise<void> {
    try {
      agents = await invoke("list_agents");
    } catch {
      agents = [];
    }
    try {
      pipelines = await invoke("list_pipelines");
    } catch {
      pipelines = [];
    }
  }

  $effect(() => {
    if (open) {
      resetForm();
      void loadOptions();

      // Pre-fill from tour context if available.
      const prefill = get(tourPrefill);
      if (
        prefill !== null &&
        prefill.interaction_type === "create_trigger" &&
        prefill.prefilled_data !== null &&
        prefill.prefilled_data !== undefined
      ) {
        const data = prefill.prefilled_data;
        const agentId = data["agent_id"];
        const type = data["type"];
        const everySeconds = data["every_seconds"];

        if (typeof agentId === "string") {
          selectedAgent = agentId;
          targetKind = "agent";
        }
        if (type === "interval") {
          sourceType = "interval";
          if (typeof everySeconds === "number") {
            intervalEvery = `${everySeconds}s`;
          }
        }
        const label = data["label"];
        if (typeof label === "string") {
          triggerId = label
            .toLowerCase()
            .replace(/[^a-z0-9]+/g, "-")
            .replace(/^-+|-+$/g, "");
        }
      }
    }
  });
</script>

<Dialog open={open} onclose={onclose} size="md" title={$t("triggers.create_trigger")} data-testid="trigger-create-dialog">
  <div class="space-y-4">
    <!-- ID -->
    <div>
      <label class="mb-1 block text-[11px] text-muted-foreground" for="trigger-id">{$t("triggers.field_id")}</label>
      <Input
        id="trigger-id"
        class={idError ? 'border-destructive' : ''}
        placeholder={$t("triggers.field_id_placeholder")}
        bind:value={triggerId}
        data-testid="trigger-input-name"
      />
      <p class="mt-0.5 text-xs text-muted-foreground">{$t("triggers.field_id_help")}</p>
      {#if idError}
        <p class="mt-0.5 text-xs text-destructive" data-testid="trigger-id-error">{idError}</p>
      {/if}
    </div>

    <!-- Target: Agent / Pipeline -->
    <div>
      <p class="mb-1 block text-[11px] text-muted-foreground">{$t("triggers.field_target")}</p>
      <div class="mb-2 flex gap-3">
        <RadioItem
          value="agent"
          checked={targetKind === "agent"}
          onchange={() => targetKind = "agent"}
          data-testid="trigger-target-agent-radio"
        >
          {$t("triggers.field_target_agent")}
        </RadioItem>
        <RadioItem
          value="pipeline"
          checked={targetKind === "pipeline"}
          onchange={() => targetKind = "pipeline"}
          data-testid="trigger-target-pipeline-radio"
        >
          {$t("triggers.field_target_pipeline")}
        </RadioItem>
      </div>
      {#if targetKind === "agent"}
        <Select
          bind:value={selectedAgent}
          aria-label={$t("triggers.field_target_agent")}
          data-testid="trigger-input-agent"
        >
          <option value="" disabled>— {$t("triggers.field_target_agent")} —</option>
          {#each agents as agent}
            <option value={agent.name}>{agent.name}</option>
          {/each}
        </Select>
      {:else}
        <Select
          bind:value={selectedPipeline}
          aria-label={$t("triggers.field_target_pipeline")}
          data-testid="trigger-pipeline-select"
        >
          <option value="" disabled>— {$t("triggers.field_target_pipeline")} —</option>
          {#each pipelines as pipeline}
            <option value={pipeline.id}>{pipeline.id}{pipeline.description ? ` — ${pipeline.description}` : ""}</option>
          {/each}
        </Select>
      {/if}
      {#if targetError}
        <p class="mt-0.5 text-xs text-destructive" data-testid="trigger-target-error">{targetError}</p>
      {/if}
    </div>

    <!-- Source type -->
    <div>
      <label class="mb-1 block text-[11px] text-muted-foreground" for="trigger-source-type">{$t("triggers.field_source_type")}</label>
      <Select
        id="trigger-source-type"
        bind:value={sourceType}
        data-testid="trigger-source-select"
      >
        <option value="cron">{$t("triggers.field_type_cron")}</option>
        <option value="interval">{$t("triggers.field_type_interval")}</option>
        <option value="oneshot">{$t("triggers.field_type_oneshot")}</option>
        <option value="file_watch">{$t("triggers.field_type_file_watch")}</option>
        <option value="webhook">{$t("triggers.field_type_webhook")}</option>
      </Select>
    </div>

    <!-- Dynamic source fields -->
    {#if sourceType === "cron"}
      <div>
        <label class="mb-1 block text-[11px] text-muted-foreground" for="trigger-schedule">{$t("triggers.field_schedule")}</label>
        <Input
          id="trigger-schedule"
          icon={Clock}
          class="font-mono {sourceError && sourceType === 'cron' ? 'border-destructive' : ''}"
          placeholder={$t("triggers.field_schedule_placeholder")}
          bind:value={cronSchedule}
          data-testid="trigger-input-cron"
        />
        <p class="mt-0.5 text-xs text-muted-foreground">{$t("triggers.field_schedule_help")}</p>
      </div>
    {:else if sourceType === "interval"}
      <div>
        <label class="mb-1 block text-[11px] text-muted-foreground" for="trigger-every">{$t("triggers.field_every")}</label>
        <Input
          id="trigger-every"
          class="font-mono {sourceError && sourceType === 'interval' ? 'border-destructive' : ''}"
          placeholder={$t("triggers.field_every_placeholder")}
          bind:value={intervalEvery}
          data-testid="trigger-input-interval"
        />
        <p class="mt-0.5 text-xs text-muted-foreground">{$t("triggers.field_every_help")}</p>
      </div>
    {:else if sourceType === "oneshot"}
      <div>
        <label class="mb-1 block text-[11px] text-muted-foreground" for="trigger-fire-at">{$t("triggers.field_fire_at")}</label>
        <div class="grid grid-cols-2 gap-2">
          <DatePicker
            id="trigger-fire-at"
            class={sourceError && sourceType === 'oneshot' ? 'border-destructive' : ''}
            bind:value={oneshotDate}
            data-testid="trigger-fire-at-input"
          />
          <TimePicker
            class={sourceError && sourceType === 'oneshot' ? 'border-destructive' : ''}
            bind:value={oneshotTime}
            data-testid="trigger-fire-at-time-input"
          />
        </div>
      </div>
    {:else if sourceType === "file_watch"}
      <div>
        <label class="mb-1 block text-[11px] text-muted-foreground" for="trigger-watch-path">{$t("triggers.field_path")}</label>
        <Input
          id="trigger-watch-path"
          class={sourceError && sourceType === 'file_watch' && !fileWatchPath.trim() ? 'border-destructive' : ''}
          placeholder={$t("triggers.field_path_placeholder")}
          bind:value={fileWatchPath}
          data-testid="trigger-input-filepath"
        />
      </div>
      <div>
        <p class="mb-1 block text-[11px] text-muted-foreground">{$t("triggers.field_events")}</p>
        <div class="flex gap-4">
          <label class="flex items-center gap-1.5 text-sm">
            <Checkbox bind:checked={fileWatchCreate} data-testid="trigger-event-create" />
            {$t("triggers.field_events_create")}
          </label>
          <label class="flex items-center gap-1.5 text-sm">
            <Checkbox bind:checked={fileWatchModify} data-testid="trigger-event-modify" />
            {$t("triggers.field_events_modify")}
          </label>
          <label class="flex items-center gap-1.5 text-sm">
            <Checkbox bind:checked={fileWatchDelete} data-testid="trigger-event-delete" />
            {$t("triggers.field_events_delete")}
          </label>
        </div>
        {#if sourceError && sourceType === "file_watch" && !fileWatchCreate && !fileWatchModify && !fileWatchDelete}
          <p class="mt-0.5 text-xs text-destructive">{$t("triggers.field_events_required")}</p>
        {/if}
      </div>
    {:else if sourceType === "webhook"}
      <div>
        <label class="mb-1 block text-[11px] text-muted-foreground" for="trigger-secret">{$t("triggers.field_secret")}</label>
        <div class="flex gap-2">
          <Input
            id="trigger-secret"
            class="flex-1 font-mono {sourceError && sourceType === 'webhook' ? 'border-destructive' : ''}"
            placeholder={$t("triggers.field_secret_placeholder")}
            bind:value={webhookSecret}
            data-testid="trigger-input-webhook-path"
          />
          <Button size="sm" variant="outline" onclick={generateSecret} data-testid="trigger-secret-generate-btn">
            {$t("triggers.field_secret_generate")}
          </Button>
        </div>
      </div>
    {/if}

    {#if sourceError}
      <p class="text-xs text-destructive" data-testid="trigger-source-error">{sourceError}</p>
    {/if}

    <!-- Input template -->
    <div>
      <label class="mb-1 block text-[11px] text-muted-foreground" for="trigger-input-template">
        {$t("triggers.field_input_template")}
        <span class="font-normal text-muted-foreground">({$t("pipelines.input_json_optional")})</span>
      </label>
      <Textarea
        id="trigger-input-template"
        rows={2}
        placeholder={$t("triggers.field_input_template_placeholder")}
        bind:value={inputTemplate}
        data-testid="trigger-input-template-textarea"
      />
    </div>

    <!-- On busy + Enabled -->
    <div class="flex items-center gap-6">
      <div>
        <label class="mb-1 block text-[11px] text-muted-foreground" for="trigger-on-busy">{$t("triggers.field_on_busy")}</label>
        <Select
          id="trigger-on-busy"
          bind:value={onBusy}
          data-testid="trigger-on-busy-select"
        >
          <option value="queue">{$t("triggers.field_on_busy_queue")}</option>
          <option value="drop">{$t("triggers.field_on_busy_drop")}</option>
        </Select>
      </div>
      <label class="flex items-center gap-2 text-sm">
        <Toggle bind:checked={enabled} data-testid="trigger-enabled-toggle" />
        {$t("triggers.field_enabled")}
      </label>
    </div>

    <!-- Submit error -->
    {#if submitError}
      <p class="text-sm text-destructive" data-testid="create-trigger-error">{submitError}</p>
    {/if}

    <!-- Actions -->
    <div class="flex justify-end gap-2">
      <Button variant="outline" size="sm" onclick={onclose} data-testid="create-trigger-cancel-btn">
        {$t("common.cancel")}
      </Button>
      <Button
        size="sm"
        onclick={handleSubmit}
        disabled={submitting || (touched && !isValid)}
        data-testid="create-trigger-submit-btn"
      >
        {submitting ? $t("triggers.creating") : $t("triggers.create_trigger")}
      </Button>
    </div>
  </div>
</Dialog>
