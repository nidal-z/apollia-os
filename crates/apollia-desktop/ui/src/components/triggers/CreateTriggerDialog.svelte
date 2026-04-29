<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { get } from "svelte/store";
  import { t } from "svelte-i18n";
  import { slide } from "svelte/transition";
  import { tourPrefill } from "$lib/stores/tour";
  import { uiMode } from "$lib/stores/mode";
  import type {
    AgentListItem,
    CreateTriggerRequest,
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
  import { Textarea } from "$lib/components/ui/textarea";
  import { Toggle } from "$lib/components/ui/toggle";
  import { Dialog } from "$lib/components/ui/dialog";
  import { DatePicker, TimePicker } from "$lib/components/ui/date-picker";
  import CronBuilder from "./CronBuilder.svelte";
  import IntervalPicker from "./IntervalPicker.svelte";

  interface Props {
    open: boolean;
    onclose: () => void;
    oncreated: (id: string) => void;
  }

  let { open, onclose, oncreated }: Props = $props();

  type SourceType = "cron" | "interval" | "oneshot" | "file_watch" | "webhook";

  const isBuilder = $derived($uiMode === "builder");

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

  let triggerId = $state("");
  let idCustomized = $state(false);
  let selectedAgent = $state("");
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

  const TRIGGER_ID_PATTERN = /^[a-z0-9][a-z0-9-]*[a-z0-9]$|^[a-z0-9]$/;

  // Auto-generate ID for operators when agent + source type are set
  $effect(() => {
    if (!isBuilder && !idCustomized) {
      if (selectedAgent && sourceType) {
        const slug = selectedAgent.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
        triggerId = `${slug}-${sourceType.replace("_", "-")}`;
      }
    }
  });

  const idError = $derived.by(() => {
    if (!touched) return null;
    if (!triggerId.trim()) return $t("triggers.field_id_required");
    if (!TRIGGER_ID_PATTERN.test(triggerId)) return $t("triggers.field_id_invalid");
    return null;
  });

  const targetError = $derived.by(() => {
    if (!touched) return null;
    if (!selectedAgent) return $t("triggers.field_target_required");
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
    !!selectedAgent &&
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
        agent: selectedAgent,
      };
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
    idCustomized = false;
    selectedAgent = "";
    sourceType = "cron";
    enabled = true;
    onBusy = "queue";
    inputTemplate = "";
    advancedOpen = false;
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
  }

  $effect(() => {
    if (open) {
      resetForm();
      void loadOptions();

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
          idCustomized = true;
        }
      }
    }
  });
</script>

<Dialog open={open} onclose={onclose} size="md" title={$t("triggers.create_trigger")} data-testid="trigger-create-dialog">
  <div class="space-y-5">

    <!-- Target: Agent -->
    <div>
      <p class="mb-1.5 block text-[11px] font-medium text-muted-foreground">{$t("triggers.field_target")}</p>
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
      {#if targetError}
        <p class="mt-0.5 text-xs text-destructive" data-testid="trigger-target-error">{targetError}</p>
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
            data-testid="trigger-type-card-{typeConfig.value}"
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
        <div data-testid="trigger-input-cron">
          <CronBuilder
            value={cronSchedule}
            onchange={(expr) => { cronSchedule = expr; }}
          />
        </div>
        {#if sourceError && sourceType === "cron"}
          <p class="mt-1 text-xs text-destructive" data-testid="trigger-source-error">{sourceError}</p>
        {/if}
      </div>

    {:else if sourceType === "interval"}
      <div>
        <p class="mb-2 block text-[11px] font-medium text-muted-foreground">{$t("triggers.field_every")}</p>
        <div data-testid="trigger-input-interval">
          <IntervalPicker
            value={intervalEvery}
            onchange={(val) => { intervalEvery = val; }}
          />
        </div>
        {#if sourceError && sourceType === "interval"}
          <p class="mt-1 text-xs text-destructive" data-testid="trigger-source-error">{sourceError}</p>
        {/if}
      </div>

    {:else if sourceType === "oneshot"}
      <div>
        <label class="mb-1.5 block text-[11px] font-medium text-muted-foreground" for="trigger-fire-at">{$t("triggers.field_fire_at")}</label>
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
        {#if sourceError && sourceType === "oneshot"}
          <p class="mt-0.5 text-xs text-destructive" data-testid="trigger-source-error">{sourceError}</p>
        {/if}
      </div>

    {:else if sourceType === "file_watch"}
      <div class="space-y-3">
        <div>
          <label class="mb-1.5 block text-[11px] font-medium text-muted-foreground" for="trigger-watch-path">{$t("triggers.field_path")}</label>
          <Input
            id="trigger-watch-path"
            class={sourceError && sourceType === 'file_watch' && !fileWatchPath.trim() ? 'border-destructive' : ''}
            placeholder={$t("triggers.field_path_placeholder")}
            bind:value={fileWatchPath}
            data-testid="trigger-input-filepath"
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
              data-testid="trigger-event-create"
            >
              <PlusCircle size={11} />
              {$t("triggers.field_events_create")}
            </button>
            <button
              type="button"
              class="flex items-center gap-1.5 rounded-full border px-3 py-1 text-xs font-medium transition-colors
                {fileWatchModify ? 'border-primary bg-primary/10 text-primary' : 'border-border text-muted-foreground hover:border-primary/40'}"
              onclick={() => { fileWatchModify = !fileWatchModify; }}
              data-testid="trigger-event-modify"
            >
              <Pencil size={11} />
              {$t("triggers.field_events_modify")}
            </button>
            <button
              type="button"
              class="flex items-center gap-1.5 rounded-full border px-3 py-1 text-xs font-medium transition-colors
                {fileWatchDelete ? 'border-destructive/80 bg-destructive/10 text-destructive' : 'border-border text-muted-foreground hover:border-destructive/40'}"
              onclick={() => { fileWatchDelete = !fileWatchDelete; }}
              data-testid="trigger-event-delete"
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
          <label class="mb-1.5 block text-[11px] font-medium text-muted-foreground" for="trigger-secret">{$t("triggers.field_secret")}</label>
          <div class="flex gap-2">
            <Input
              id="trigger-secret"
              class="flex-1 font-mono text-xs {sourceError && sourceType === 'webhook' ? 'border-destructive' : ''}"
              placeholder={$t("triggers.field_secret_placeholder")}
              bind:value={webhookSecret}
              data-testid="trigger-input-webhook-path"
            />
            <Button size="sm" variant="outline" onclick={generateSecret} data-testid="trigger-secret-generate-btn">
              {$t("triggers.field_secret_generate")}
            </Button>
          </div>
          {#if sourceError && sourceType === "webhook"}
            <p class="mt-0.5 text-xs text-destructive" data-testid="trigger-source-error">{sourceError}</p>
          {/if}
        </div>
      </div>
    {/if}

    <!-- Enabled toggle -->
    <label class="flex items-center gap-2 text-sm">
      <Toggle bind:checked={enabled} data-testid="trigger-enabled-toggle" />
      {$t("triggers.field_enabled")}
    </label>

    <!-- Advanced settings (collapsible, closed by default for both profiles) -->
    <div class="rounded-md border border-border">
      <button
        type="button"
        class="flex w-full items-center justify-between px-3 py-2 text-[11px] font-medium text-muted-foreground transition-colors hover:bg-muted/30"
        onclick={() => { advancedOpen = !advancedOpen; }}
        data-testid="trigger-advanced-toggle"
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
          <!-- Trigger ID -->
          <div>
            <label class="mb-1 block text-[11px] text-muted-foreground" for="trigger-id">{$t("triggers.field_id")}</label>
            {#if !isBuilder}
              <p class="mb-1 text-[11px] text-muted-foreground/70">{$t("triggers.field_id_auto")}</p>
            {/if}
            <Input
              id="trigger-id"
              class={idError ? 'border-destructive' : ''}
              placeholder={$t("triggers.field_id_placeholder")}
              bind:value={triggerId}
              oninput={() => { idCustomized = true; }}
              data-testid="trigger-input-name"
            />
            <p class="mt-0.5 text-[11px] text-muted-foreground">{$t("triggers.field_id_help")}</p>
            {#if idError}
              <p class="mt-0.5 text-xs text-destructive" data-testid="trigger-id-error">{idError}</p>
            {/if}
          </div>

          <!-- On busy -->
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

          <!-- Input template -->
          <div>
            <label class="mb-1 block text-[11px] text-muted-foreground" for="trigger-input-template">
              {$t("triggers.field_input_template")}
              <span class="font-normal text-muted-foreground">({$t("common.optional")})</span>
            </label>
            <Textarea
              id="trigger-input-template"
              rows={2}
              placeholder={$t("triggers.field_input_template_placeholder")}
              bind:value={inputTemplate}
              data-testid="trigger-input-template-textarea"
            />
          </div>
        </div>
      {/if}
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
