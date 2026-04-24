<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { slide } from "svelte/transition";
  import type {
    AgentListItem,
    PipelineInfo,
    UpdateTriggerRequest,
    TriggerSourceInput,
    TriggerDefinitionView,
  } from "$lib/types";
  import {
    Calendar,
    Timer,
    CalendarCheck,
    FolderOpen,
    Webhook,
    PlusCircle,
    Pencil,
    Trash2,
    ChevronDown,
  } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";
  import { RadioItem } from "$lib/components/ui/radio";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Toggle } from "$lib/components/ui/toggle";
  import { Dialog } from "$lib/components/ui/dialog";
  import { DatePicker, TimePicker } from "$lib/components/ui/date-picker";
  import CronBuilder from "./CronBuilder.svelte";
  import IntervalPicker from "./IntervalPicker.svelte";

  interface Props {
    open: boolean;
    triggerId: string;
    onclose: () => void;
    onupdated: (id: string) => void;
  }

  let { open, triggerId, onclose, onupdated }: Props = $props();

  type SourceType = "cron" | "interval" | "oneshot" | "file_watch" | "webhook";
  type TargetKind = "agent" | "pipeline";

  const TRIGGER_TYPE_CONFIGS: {
    value: SourceType;
    labelKey: string;
    descKey: string;
    icon: typeof Calendar;
  }[] = [
    { value: "cron", labelKey: "triggers.type_cron_label", descKey: "triggers.type_cron_desc", icon: Calendar },
    { value: "interval", labelKey: "triggers.type_interval_label", descKey: "triggers.type_interval_desc", icon: Timer },
    { value: "oneshot", labelKey: "triggers.type_oneshot_label", descKey: "triggers.type_oneshot_desc", icon: CalendarCheck },
    { value: "file_watch", labelKey: "triggers.type_file_watch_label", descKey: "triggers.type_file_watch_desc", icon: FolderOpen },
    { value: "webhook", labelKey: "triggers.type_webhook_label", descKey: "triggers.type_webhook_desc", icon: Webhook },
  ];

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
  let advancedOpen = $state(false);

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
    oneshotDate = "";
    oneshotTime = "";
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
      case "oneshot": {
        const fa = (cfg.fire_at as string) ?? "";
        oneshotFireAt = fa;
        const [d, tm] = fa.split("T");
        oneshotDate = d ?? "";
        oneshotTime = (tm ?? "").slice(0, 5);
        break;
      }
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
    advancedOpen = false;
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

  $effect(() => {
    if (open && triggerId) {
      void loadTrigger();
    }
  });
</script>

<Dialog {open} {onclose} size="md" title={$t("triggers.edit_trigger")} data-testid="trigger-edit-dialog">
  {#if loading}
    <p class="py-8 text-center text-sm text-muted-foreground">{$t("triggers.saving")}…</p>
  {:else if loadError}
    <div class="space-y-4">
      <p class="text-sm text-destructive">{loadError}</p>
      <div class="flex justify-end">
        <Button variant="outline" size="sm" onclick={onclose}>{$t("common.cancel")}</Button>
      </div>
    </div>
  {:else}
    <div class="space-y-5">

      <!-- ID (readonly) — always visible for reference -->
      <div>
        <label class="mb-1 block text-[11px] text-muted-foreground" for="edit-trigger-id">{$t("triggers.field_id")}</label>
        <Input
          id="edit-trigger-id"
          class="bg-muted font-mono text-xs"
          value={triggerId}
          readonly
          data-testid="edit-trigger-id-input"
        />
      </div>

      <!-- Target: Agent / Pipeline -->
      <div>
        <p class="mb-1.5 block text-[11px] font-medium text-muted-foreground">{$t("triggers.field_target")}</p>
        <div class="mb-2 flex gap-3">
          <RadioItem
            value="agent"
            checked={targetKind === "agent"}
            onchange={() => targetKind = "agent"}
            data-testid="edit-trigger-target-agent-radio"
          >
            {$t("triggers.field_target_agent")}
          </RadioItem>
          <RadioItem
            value="pipeline"
            checked={targetKind === "pipeline"}
            onchange={() => targetKind = "pipeline"}
            data-testid="edit-trigger-target-pipeline-radio"
          >
            {$t("triggers.field_target_pipeline")}
          </RadioItem>
        </div>
        {#if targetKind === "agent"}
          <Select
            bind:value={selectedAgent}
            aria-label={$t("triggers.field_target_agent")}
            data-testid="edit-trigger-agent-select"
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
            data-testid="edit-trigger-pipeline-select"
          >
            <option value="" disabled>— {$t("triggers.field_target_pipeline")} —</option>
            {#each pipelines as pipeline}
              <option value={pipeline.id}>{pipeline.id}{pipeline.description ? ` — ${pipeline.description}` : ""}</option>
            {/each}
          </Select>
        {/if}
        {#if targetError}
          <p class="mt-0.5 text-xs text-destructive">{targetError}</p>
        {/if}
      </div>

      <!-- Source type: visual cards -->
      <div>
        <p class="mb-2 block text-[11px] font-medium text-muted-foreground">{$t("triggers.field_source_type")}</p>
        <div class="grid grid-cols-2 gap-2" role="radiogroup" aria-label={$t("triggers.field_source_type")}>
          {#each TRIGGER_TYPE_CONFIGS as typeConfig}
            <button
              type="button"
              role="radio"
              aria-checked={sourceType === typeConfig.value}
              class="flex items-start gap-2.5 rounded-lg border p-3 text-left transition-colors
                {sourceType === typeConfig.value
                  ? 'border-primary bg-primary/5 ring-1 ring-primary'
                  : 'border-border hover:border-primary/40 hover:bg-muted/30'}
                {typeConfig.value === 'webhook' ? 'col-span-2' : ''}"
              onclick={() => { sourceType = typeConfig.value; }}
              data-testid="edit-trigger-type-card-{typeConfig.value}"
            >
              <typeConfig.icon
                size={16}
                class="mt-0.5 shrink-0 {sourceType === typeConfig.value ? 'text-primary' : 'text-muted-foreground'}"
              />
              <div class="min-w-0">
                <p class="text-[13px] font-medium leading-tight">{$t(typeConfig.labelKey)}</p>
                <p class="mt-0.5 text-[11px] leading-snug text-muted-foreground">{$t(typeConfig.descKey)}</p>
              </div>
            </button>
          {/each}
        </div>
      </div>

      <!-- Dynamic source fields -->
      {#if sourceType === "cron"}
        <div>
          <p class="mb-2 block text-[11px] font-medium text-muted-foreground">{$t("triggers.field_schedule")}</p>
          <div data-testid="edit-trigger-schedule-input">
            <CronBuilder
              value={cronSchedule}
              onchange={(expr) => { cronSchedule = expr; }}
            />
          </div>
          {#if sourceError && sourceType === "cron"}
            <p class="mt-1 text-xs text-destructive">{sourceError}</p>
          {/if}
        </div>

      {:else if sourceType === "interval"}
        <div>
          <p class="mb-2 block text-[11px] font-medium text-muted-foreground">{$t("triggers.field_every")}</p>
          <div data-testid="edit-trigger-every-input">
            <IntervalPicker
              value={intervalEvery}
              onchange={(val) => { intervalEvery = val; }}
            />
          </div>
          {#if sourceError && sourceType === "interval"}
            <p class="mt-1 text-xs text-destructive">{sourceError}</p>
          {/if}
        </div>

      {:else if sourceType === "oneshot"}
        <div>
          <label class="mb-1.5 block text-[11px] font-medium text-muted-foreground" for="edit-trigger-fire-at">{$t("triggers.field_fire_at")}</label>
          <div class="grid grid-cols-2 gap-2">
            <DatePicker
              id="edit-trigger-fire-at"
              bind:value={oneshotDate}
              data-testid="edit-trigger-fire-at-input"
            />
            <TimePicker
              bind:value={oneshotTime}
              data-testid="edit-trigger-fire-at-time-input"
            />
          </div>
          {#if sourceError && sourceType === "oneshot"}
            <p class="mt-0.5 text-xs text-destructive">{sourceError}</p>
          {/if}
        </div>

      {:else if sourceType === "file_watch"}
        <div class="space-y-3">
          <div>
            <label class="mb-1.5 block text-[11px] font-medium text-muted-foreground" for="edit-trigger-watch-path">{$t("triggers.field_path")}</label>
            <Input
              id="edit-trigger-watch-path"
              class={sourceError && sourceType === 'file_watch' && !fileWatchPath.trim() ? 'border-destructive' : ''}
              placeholder={$t("triggers.field_path_placeholder")}
              bind:value={fileWatchPath}
              data-testid="edit-trigger-path-input"
            />
            <p class="mt-0.5 text-[11px] text-muted-foreground">The agent runs whenever a file event occurs in this folder.</p>
          </div>
          <div>
            <p class="mb-2 block text-[11px] font-medium text-muted-foreground">{$t("triggers.field_events")}</p>
            <div class="flex gap-2">
              <button
                type="button"
                class="flex items-center gap-1.5 rounded-full border px-3 py-1 text-xs font-medium transition-colors
                  {fileWatchCreate ? 'border-primary bg-primary/10 text-primary' : 'border-border text-muted-foreground hover:border-primary/40'}"
                onclick={() => { fileWatchCreate = !fileWatchCreate; }}
                data-testid="edit-trigger-event-create"
              >
                <PlusCircle size={11} />
                {$t("triggers.field_events_create")}
              </button>
              <button
                type="button"
                class="flex items-center gap-1.5 rounded-full border px-3 py-1 text-xs font-medium transition-colors
                  {fileWatchModify ? 'border-primary bg-primary/10 text-primary' : 'border-border text-muted-foreground hover:border-primary/40'}"
                onclick={() => { fileWatchModify = !fileWatchModify; }}
                data-testid="edit-trigger-event-modify"
              >
                <Pencil size={11} />
                {$t("triggers.field_events_modify")}
              </button>
              <button
                type="button"
                class="flex items-center gap-1.5 rounded-full border px-3 py-1 text-xs font-medium transition-colors
                  {fileWatchDelete ? 'border-destructive/80 bg-destructive/10 text-destructive' : 'border-border text-muted-foreground hover:border-destructive/40'}"
                onclick={() => { fileWatchDelete = !fileWatchDelete; }}
                data-testid="edit-trigger-event-delete"
              >
                <Trash2 size={11} />
                {$t("triggers.field_events_delete")}
              </button>
            </div>
            {#if sourceError && sourceType === "file_watch" && !fileWatchCreate && !fileWatchModify && !fileWatchDelete}
              <p class="mt-1 text-xs text-destructive">{$t("triggers.field_events_required")}</p>
            {/if}
          </div>
        </div>

      {:else if sourceType === "webhook"}
        <div class="space-y-3">
          <div class="rounded-md border border-border bg-muted/30 px-3.5 py-2.5 text-[12px] text-muted-foreground leading-relaxed">
            {$t("triggers.webhook_explain")}
          </div>
          <div>
            <label class="mb-1.5 block text-[11px] font-medium text-muted-foreground" for="edit-trigger-secret">{$t("triggers.field_secret")}</label>
            <div class="flex gap-2">
              <Input
                id="edit-trigger-secret"
                class="flex-1 font-mono text-xs {sourceError && sourceType === 'webhook' ? 'border-destructive' : ''}"
                placeholder={$t("triggers.field_secret_placeholder")}
                bind:value={webhookSecret}
                data-testid="edit-trigger-secret-input"
              />
              <Button size="sm" variant="outline" onclick={generateSecret} data-testid="edit-trigger-secret-generate-btn">
                {$t("triggers.field_secret_generate")}
              </Button>
            </div>
            {#if sourceError && sourceType === "webhook"}
              <p class="mt-0.5 text-xs text-destructive">{sourceError}</p>
            {/if}
          </div>
        </div>
      {/if}

      <!-- Enabled toggle -->
      <label class="flex items-center gap-2 text-sm">
        <Toggle bind:checked={enabled} data-testid="edit-trigger-enabled-toggle" />
        {$t("triggers.field_enabled")}
      </label>

      <!-- Advanced settings (collapsible, closed by default for both profiles) -->
      <div class="rounded-md border border-border">
        <button
          type="button"
          class="flex w-full items-center justify-between px-3 py-2 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-muted/30"
          onclick={() => { advancedOpen = !advancedOpen; }}
          data-testid="edit-trigger-advanced-toggle"
        >
          {$t("triggers.advanced_section")}
          <ChevronDown
            size={14}
            class="transition-transform duration-200 {advancedOpen ? 'rotate-180' : ''}"
          />
        </button>
        {#if advancedOpen}
          <div
            class="space-y-4 border-t border-border px-3 pb-3 pt-3"
            transition:slide={{ duration: 180 }}
          >
            <!-- On busy -->
            <div>
              <label class="mb-1 block text-[11px] text-muted-foreground" for="edit-trigger-on-busy">{$t("triggers.field_on_busy")}</label>
              <Select
                id="edit-trigger-on-busy"
                bind:value={onBusy}
                data-testid="edit-trigger-on-busy-select"
              >
                <option value="queue">{$t("triggers.field_on_busy_queue")}</option>
                <option value="drop">{$t("triggers.field_on_busy_drop")}</option>
              </Select>
            </div>

            <!-- Input template -->
            <div>
              <label class="mb-1 block text-[11px] text-muted-foreground" for="edit-trigger-input-template">
                {$t("triggers.field_input_template")}
                <span class="font-normal text-muted-foreground">({$t("pipelines.input_json_optional")})</span>
              </label>
              <Textarea
                id="edit-trigger-input-template"
                rows={2}
                placeholder={$t("triggers.field_input_template_placeholder")}
                bind:value={inputTemplate}
                data-testid="edit-trigger-input-template-textarea"
              />
            </div>
          </div>
        {/if}
      </div>

      <!-- Submit error -->
      {#if submitError}
        <p class="text-sm text-destructive" data-testid="edit-trigger-error">{submitError}</p>
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
</Dialog>
