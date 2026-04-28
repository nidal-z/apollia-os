<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { fly } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { t, locale } from "svelte-i18n";
  import {
    Plus,
    RefreshCw,
    Check,
    X,
    Play,
    Timer as TimerIcon,
    Calendar as CalendarIcon,
    ShieldAlert,
  } from "lucide-svelte";
  import { triggers } from "$lib/stores/triggers";
  import { uiMode } from "$lib/stores/mode";
  import type {
    TriggerReloadResult,
    TriggerStatus,
    TriggerDefinitionView,
    CreateTriggerRequest,
    UpdateTriggerRequest,
    TriggerSourceInput,
    AgentListItem,
    PipelineInfo,
  } from "$lib/types";
  import { humanizeTrigger } from "$lib/utils/humanize-trigger";
  import {
    PageHeader,
    SectionTitle,
    BtnPrimary,
    BtnSecondary,
    Chip,
    StatusDot,
    EmptyState,
    TriggerCard,
    TriggerSourcePicker,
  } from "$lib/components/operator";
  import type {
    Trigger as DsTrigger,
    TriggerSource as DsTriggerSource,
    TriggerStatus as DsTriggerStatus,
  } from "$lib/components/operator";
  import type { TriggerSourceKind } from "$lib/components/operator";
  import { addToast } from "$lib/components/ui/toast/store";
  import TriggerLogs from "../components/triggers/TriggerLogs.svelte";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";

  // ─────────────────────────────────────────────────────────────────────
  // View state
  // ─────────────────────────────────────────────────────────────────────
  type View = "list" | "form";
  let view = $state<View>("list");

  let reloading = $state(false);
  let reloadError = $state<string | null>(null);
  let logsTriggerId = $state<string | null>(null);
  let logsOpen = $derived(logsTriggerId !== null);
  let deleteTriggerId = $state<string | null>(null);
  let deleting = $state(false);

  // ─────────────────────────────────────────────────────────────────────
  // List view — grouping & status mapping
  // ─────────────────────────────────────────────────────────────────────
  function mapKind(k: TriggerStatus["source_kind"]): DsTriggerSource {
    switch (k) {
      case "cron":
        return "cron";
      case "interval":
        return "interval";
      case "oneshot":
        return "once";
      case "file_watch":
        return "file";
      case "webhook":
        return "webhook";
    }
  }

  function deriveStatus(t: TriggerStatus): DsTriggerStatus {
    if (!t.enabled) return "paused";
    if (t.source_kind === "oneshot") return "scheduled";
    return "ok";
  }

  function toDsTrigger(s: TriggerStatus, lang: string): DsTrigger {
    return {
      id: s.id,
      source: mapKind(s.source_kind),
      expression: s.source_config,
      human: humanizeTrigger(s.source_kind, s.source_config, lang),
      agent: s.agent,
      fires: s.fire_count,
      ignored: s.skip_count,
      status: deriveStatus(s),
      active: s.enabled,
      lastFired: s.last_fired ?? undefined,
    };
  }

  const triggersByAgent = $derived.by(() => {
    const groups = new Map<string, TriggerStatus[]>();
    for (const trigger of $triggers) {
      const existing = groups.get(trigger.agent);
      if (existing) existing.push(trigger);
      else groups.set(trigger.agent, [trigger]);
    }
    return groups;
  });

  // ─────────────────────────────────────────────────────────────────────
  // List view — actions (preserve all invoke() calls)
  // ─────────────────────────────────────────────────────────────────────
  async function handleReload() {
    reloading = true;
    reloadError = null;
    try {
      const result: TriggerReloadResult = await invoke("reload_triggers");
      addToast(
        $t("triggers.reload_success", { values: { count: result.reloaded } }),
        "success",
      );
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      reloadError = msg;
      addToast($t("triggers.reload_error", { values: { message: msg } }), "error");
    } finally {
      reloading = false;
    }
  }

  async function handleToggle(trigger: TriggerStatus, next: boolean) {
    try {
      await invoke("set_trigger_enabled", { id: trigger.id, enabled: next });
      addToast(
        $t(next ? "triggers.enabled_toast" : "triggers.disabled_toast", {
          values: { id: trigger.id },
        }),
        "success",
      );
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }

  async function handleFire(trigger: TriggerStatus) {
    try {
      const result: { task_id: string } = await invoke("fire_trigger", {
        id: trigger.id,
      });
      addToast(
        $t("triggers.fired_toast", {
          values: { taskId: result.task_id.slice(0, 8) },
        }),
        "success",
      );
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }

  function handleEdit(trigger: TriggerStatus) {
    void openFormForEdit(trigger.id);
  }

  function handleRequestDelete(triggerId: string) {
    deleteTriggerId = triggerId;
  }

  async function handleConfirmDelete() {
    if (!deleteTriggerId) return;
    const id = deleteTriggerId;
    deleting = true;
    try {
      await invoke("delete_trigger", { id });
      addToast($t("triggers.deleted_toast", { values: { id } }), "success");
      void handleReload();
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      addToast($t("triggers.delete_error", { values: { message: msg } }), "error");
    } finally {
      deleting = false;
      deleteTriggerId = null;
    }
  }

  // ─────────────────────────────────────────────────────────────────────
  // Form state
  // ─────────────────────────────────────────────────────────────────────
  let formMode = $state<"create" | "edit">("create");
  let formTriggerId = $state<string | null>(null); // when editing
  let triggerIdField = $state("");
  let idCustomized = $state(false);
  let targetKind = $state<"agent" | "pipeline">("agent");
  let selectedAgent = $state("");
  let selectedPipeline = $state("");
  let sourceKind = $state<TriggerSourceKind>("cron");
  let cronPreset = $state("day");
  let cronSchedule = $state("0 9 * * *");
  let intervalEvery = $state("3600s");
  let oneshotDate = $state("");
  let oneshotTime = $state("09:00");
  let fileWatchPath = $state("");
  let fileWatchCreate = $state(true);
  let fileWatchModify = $state(true);
  let fileWatchDelete = $state(false);
  let webhookSecret = $state("");
  let inputTemplate = $state("");
  let enabled = $state(true);
  let onBusy = $state<"queue" | "drop">("queue");

  let agents = $state<AgentListItem[]>([]);
  let pipelines = $state<PipelineInfo[]>([]);
  let saving = $state(false);
  let formError = $state<string | null>(null);

  const TRIGGER_ID_PATTERN = /^[a-z0-9][a-z0-9-]*[a-z0-9]$|^[a-z0-9]$/;

  // map UI source kind <-> backend source_type
  function uiToBackend(k: TriggerSourceKind): TriggerStatus["source_kind"] {
    return k === "once" ? "oneshot" : k === "file" ? "file_watch" : k;
  }
  function backendToUi(k: TriggerStatus["source_kind"]): TriggerSourceKind {
    return k === "oneshot" ? "once" : k === "file_watch" ? "file" : k;
  }

  function resetForm() {
    formMode = "create";
    formTriggerId = null;
    triggerIdField = "";
    idCustomized = false;
    targetKind = "agent";
    selectedAgent = "";
    selectedPipeline = "";
    sourceKind = "cron";
    cronPreset = "day";
    cronSchedule = "0 9 * * *";
    intervalEvery = "3600s";
    const today = new Date();
    oneshotDate = today.toISOString().slice(0, 10);
    oneshotTime = "09:00";
    fileWatchPath = "";
    fileWatchCreate = true;
    fileWatchModify = true;
    fileWatchDelete = false;
    webhookSecret = "";
    inputTemplate = "";
    enabled = true;
    onBusy = "queue";
    saving = false;
    formError = null;
  }

  async function loadOptions() {
    try {
      agents = await invoke<AgentListItem[]>("list_agents");
    } catch {
      agents = [];
    }
    try {
      pipelines = await invoke<PipelineInfo[]>("list_pipelines");
    } catch {
      pipelines = [];
    }
  }

  function openFormForCreate() {
    resetForm();
    void loadOptions();
    view = "form";
  }

  async function openFormForEdit(id: string) {
    resetForm();
    formMode = "edit";
    formTriggerId = id;
    await loadOptions();
    try {
      const def: TriggerDefinitionView = await invoke("get_trigger", { id });
      hydrateForm(def);
    } catch {
      // Fallback: hydrate from store
      const s = $triggers.find((tr) => tr.id === id);
      if (s) {
        triggerIdField = s.id;
        selectedAgent = s.agent;
        sourceKind = backendToUi(s.source_kind);
        // best-effort source_config parsing
        if (s.source_kind === "cron") cronSchedule = s.source_config;
        else if (s.source_kind === "interval") intervalEvery = s.source_config;
        else if (s.source_kind === "file_watch") fileWatchPath = s.source_config;
        else if (s.source_kind === "webhook") webhookSecret = s.source_config;
        else if (s.source_kind === "oneshot") {
          const m = /(\d{4}-\d{2}-\d{2})[T ](\d{2}:\d{2})/.exec(s.source_config);
          if (m) {
            oneshotDate = m[1];
            oneshotTime = m[2];
          }
        }
        enabled = s.enabled;
      }
    }
    idCustomized = true;
    view = "form";
  }

  function hydrateForm(def: TriggerDefinitionView) {
    triggerIdField = def.id;
    if (def.agent) {
      targetKind = "agent";
      selectedAgent = def.agent;
    } else if (def.pipeline) {
      targetKind = "pipeline";
      selectedPipeline = def.pipeline;
    }
    sourceKind = backendToUi(def.source_type);
    enabled = def.enabled;
    onBusy = def.on_busy;
    inputTemplate = def.input_template ?? "";
    const cfg = def.source_config ?? {};
    if (def.source_type === "cron") {
      cronSchedule = String(cfg["schedule"] ?? "");
    } else if (def.source_type === "interval") {
      intervalEvery = String(cfg["every"] ?? "");
    } else if (def.source_type === "oneshot") {
      const fa = String(cfg["fire_at"] ?? "");
      const m = /(\d{4}-\d{2}-\d{2})[T ](\d{2}:\d{2})/.exec(fa);
      if (m) {
        oneshotDate = m[1];
        oneshotTime = m[2];
      }
    } else if (def.source_type === "file_watch") {
      fileWatchPath = String(cfg["path"] ?? "");
      const events = (cfg["events"] as string[] | undefined) ?? [];
      fileWatchCreate = events.includes("create");
      fileWatchModify = events.includes("modify");
      fileWatchDelete = events.includes("delete");
    } else if (def.source_type === "webhook") {
      webhookSecret = String(cfg["secret"] ?? "");
    }
  }

  // Auto-generate ID for create flow
  $effect(() => {
    if (formMode === "create" && !idCustomized) {
      const target = targetKind === "agent" ? selectedAgent : selectedPipeline;
      if (target && sourceKind) {
        const slug = target
          .toLowerCase()
          .replace(/[^a-z0-9]+/g, "-")
          .replace(/^-+|-+$/g, "");
        triggerIdField = `${slug}-${uiToBackend(sourceKind).replace("_", "-")}`;
      }
    }
  });

  function buildSource(): TriggerSourceInput | null {
    switch (sourceKind) {
      case "cron":
        if (!cronSchedule.trim()) return null;
        return { type: "cron", schedule: cronSchedule.trim() };
      case "interval":
        if (!intervalEvery.trim()) return null;
        return { type: "interval", every: intervalEvery.trim() };
      case "once": {
        if (!oneshotDate || !oneshotTime) return null;
        return { type: "oneshot", fire_at: `${oneshotDate}T${oneshotTime}` };
      }
      case "file": {
        if (!fileWatchPath.trim()) return null;
        const events: string[] = [];
        if (fileWatchCreate) events.push("create");
        if (fileWatchModify) events.push("modify");
        if (fileWatchDelete) events.push("delete");
        if (events.length === 0) return null;
        return { type: "file_watch", path: fileWatchPath.trim(), events };
      }
      case "webhook":
        if (!webhookSecret.trim()) return null;
        return { type: "webhook", secret: webhookSecret.trim() };
    }
  }

  const idValid = $derived(
    !!triggerIdField.trim() && TRIGGER_ID_PATTERN.test(triggerIdField.trim()),
  );
  const targetValid = $derived(
    targetKind === "agent" ? !!selectedAgent : !!selectedPipeline,
  );
  const sourceValid = $derived(buildSource() !== null);
  const formValid = $derived(idValid && targetValid && sourceValid);

  // ─────────────────────────────────────────────────────────────────────
  // Live preview — next 3 fire times (best-effort, FE-only for cron/interval/once)
  // ─────────────────────────────────────────────────────────────────────
  function parseCronField(field: string, min: number, max: number): number[] {
    if (field === "*") {
      const arr: number[] = [];
      for (let i = min; i <= max; i++) arr.push(i);
      return arr;
    }
    if (field.startsWith("*/")) {
      const step = parseInt(field.slice(2), 10) || 1;
      const arr: number[] = [];
      for (let i = min; i <= max; i += step) arr.push(i);
      return arr;
    }
    return field
      .split(",")
      .flatMap((part) => {
        if (part.includes("-")) {
          const [a, b] = part.split("-").map((n) => parseInt(n, 10));
          const arr: number[] = [];
          for (let i = a; i <= b; i++) arr.push(i);
          return arr;
        }
        return [parseInt(part, 10)];
      })
      .filter((n) => !isNaN(n) && n >= min && n <= max);
  }

  function nextCronFires(expr: string, count: number, from: Date): Date[] {
    const parts = expr.trim().split(/\s+/);
    if (parts.length !== 5) return [];
    try {
      const mins = parseCronField(parts[0], 0, 59);
      const hours = parseCronField(parts[1], 0, 23);
      const doms = parseCronField(parts[2], 1, 31);
      const months = parseCronField(parts[3], 1, 12);
      const dows = parseCronField(parts[4], 0, 6);
      const out: Date[] = [];
      const cursor = new Date(from.getTime() + 60_000);
      cursor.setSeconds(0, 0);
      const limit = 366 * 24 * 60; // 1 year cap
      for (let i = 0; i < limit && out.length < count; i++) {
        if (
          mins.includes(cursor.getMinutes()) &&
          hours.includes(cursor.getHours()) &&
          doms.includes(cursor.getDate()) &&
          months.includes(cursor.getMonth() + 1) &&
          dows.includes(cursor.getDay())
        ) {
          out.push(new Date(cursor));
        }
        cursor.setMinutes(cursor.getMinutes() + 1);
      }
      return out;
    } catch {
      return [];
    }
  }

  function parseIntervalSeconds(s: string): number {
    const m = /^(\d+)\s*(s|m|h|d)?$/i.exec(s.trim());
    if (!m) return 0;
    const n = parseInt(m[1], 10);
    const unit = (m[2] ?? "s").toLowerCase();
    return n * (unit === "h" ? 3600 : unit === "m" ? 60 : unit === "d" ? 86400 : 1);
  }

  const previewFires = $derived.by<Date[]>(() => {
    const now = new Date();
    if (sourceKind === "cron") {
      return nextCronFires(cronSchedule, 3, now);
    }
    if (sourceKind === "interval") {
      const secs = parseIntervalSeconds(intervalEvery);
      if (!secs) return [];
      const out: Date[] = [];
      for (let i = 1; i <= 3; i++) {
        out.push(new Date(now.getTime() + secs * i * 1000));
      }
      return out;
    }
    if (sourceKind === "once") {
      if (!oneshotDate || !oneshotTime) return [];
      const d = new Date(`${oneshotDate}T${oneshotTime}`);
      return isNaN(d.getTime()) ? [] : [d];
    }
    return [];
  });

  function fmtDate(d: Date): string {
    return d.toLocaleString($locale ?? "fr", {
      weekday: "long",
      day: "numeric",
      month: "long",
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  // ─────────────────────────────────────────────────────────────────────
  // Form submit (preserve invoke create_trigger / update_trigger)
  // ─────────────────────────────────────────────────────────────────────
  async function handleSave() {
    formError = null;
    if (!formValid) {
      formError = $t("triggers.field_id_invalid");
      return;
    }
    const source = buildSource();
    if (!source) {
      formError = $t("triggers.field_schedule_required");
      return;
    }
    saving = true;
    try {
      if (formMode === "create") {
        const definition: CreateTriggerRequest = {
          id: triggerIdField.trim(),
          enabled,
          on_busy: onBusy,
          source,
        };
        if (targetKind === "agent") definition.agent = selectedAgent;
        else definition.pipeline = selectedPipeline;
        if (inputTemplate.trim()) definition.input_template = inputTemplate.trim();
        await invoke<TriggerDefinitionView>("create_trigger", { definition });
        addToast(
          $t("triggers.created_toast", { values: { id: definition.id } }),
          "success",
        );
      } else if (formMode === "edit" && formTriggerId) {
        const definition: UpdateTriggerRequest = {
          enabled,
          on_busy: onBusy,
          source,
        };
        if (targetKind === "agent") definition.agent = selectedAgent;
        else definition.pipeline = selectedPipeline;
        if (inputTemplate.trim()) definition.input_template = inputTemplate.trim();
        await invoke<TriggerDefinitionView>("update_trigger", {
          id: formTriggerId,
          definition,
        });
        addToast(
          $t("triggers.updated_toast", { values: { id: formTriggerId } }),
          "success",
        );
      }
      void handleReload();
      view = "list";
    } catch (err: unknown) {
      formError = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  function handleCancel() {
    view = "list";
  }

  // ─────────────────────────────────────────────────────────────────────
  // Stats
  // ─────────────────────────────────────────────────────────────────────
  const stats = $derived.by(() => {
    const list = $triggers;
    return {
      total: list.length,
      active: list.filter((tr) => tr.enabled).length,
      paused: list.filter((tr) => !tr.enabled).length,
    };
  });
</script>

<div
  class="mx-auto w-full max-w-6xl"
  data-testid="triggers-page"
  data-view={view}
>
  {#if view === "list"}
    <PageHeader
      kicker={$t("triggers.title")}
      title="Mes déclencheurs."
      subtitle={$t("triggers.subtitle")}
    >
      {#snippet actions()}
        <BtnSecondary onclick={handleReload} disabled={reloading}>
          {#snippet icon()}<RefreshCw size={12} class={reloading ? "animate-spin" : ""} />{/snippet}
          {reloading ? $t("triggers.reloading") : $t("triggers.reload")}
        </BtnSecondary>
        <BtnPrimary onclick={openFormForCreate}>
          {#snippet icon()}<Plus size={12} />{/snippet}
          {$t("triggers.new_trigger")}
        </BtnPrimary>
      {/snippet}
    </PageHeader>

    <div class="px-8 py-6 space-y-6">
      {#if reloadError}
        <div
          class="rounded-md border border-destructive bg-destructive/10 px-4 py-2 text-sm text-destructive"
        >
          {reloadError}
        </div>
      {/if}

      {#if $triggers.length === 0}
        <div class="bg-card border border-border rounded-xl">
          <EmptyState
            title={$t("triggers.title")}
            desc={$t("triggers.subtitle")}
          >
            {#snippet icon()}<TimerIcon size={22} />{/snippet}
            {#snippet action()}
              <BtnPrimary onclick={openFormForCreate}>
                {#snippet icon()}<Plus size={12} />{/snippet}
                {$t("triggers.new_trigger")}
              </BtnPrimary>
            {/snippet}
          </EmptyState>
        </div>
      {:else}
        <!-- Stats summary chip row -->
        <div class="flex items-center gap-2">
          <Chip tone="primary" size="sm">Tous · {stats.total}</Chip>
          <Chip tone="success" size="sm">Actifs · {stats.active}</Chip>
          {#if stats.paused > 0}
            <Chip tone="neutral" size="sm">En pause · {stats.paused}</Chip>
          {/if}
          {#if $uiMode === "builder"}
            <Chip tone="info" size="sm">{$t("triggers.builder_mode_badge")}</Chip>
          {/if}
        </div>

        <div class="space-y-7" data-testid="triggers-grouped">
          {#each [...triggersByAgent.entries()] as [agentName, agentTriggers] (agentName)}
            <section
              animate:flip={{ duration: 250 }}
              in:fly={{ y: 8, duration: 200 }}
              data-testid="trigger-group-{agentName}"
            >
              <SectionTitle count={agentTriggers.length}>
                {agentName}
              </SectionTitle>

              <div class="px-8 flex flex-col gap-2">
                {#each agentTriggers as trigger (trigger.id)}
                  <div animate:flip={{ duration: 250 }} in:fly={{ y: 6, duration: 180 }}>
                    <TriggerCard
                      trigger={toDsTrigger(trigger, $locale ?? "fr")}
                      onToggle={(next) => handleToggle(trigger, next)}
                      onFire={() => handleFire(trigger)}
                      onLogs={() => (logsTriggerId = trigger.id)}
                      onEdit={() => handleEdit(trigger)}
                      onDelete={() => handleRequestDelete(trigger.id)}
                    />
                  </div>
                {/each}
              </div>
            </section>
          {/each}
        </div>
      {/if}
    </div>
  {:else}
    <!-- ============================ FORM VIEW ============================ -->
    <PageHeader
      kicker={formMode === "create" ? "NOUVEAU DÉCLENCHEUR" : "MODIFIER LE DÉCLENCHEUR"}
      title={formMode === "create" ? "Créer un trigger." : "Modifier le trigger."}
      subtitle="Définissez quand Apollia doit faire travailler un agent à votre place."
    >
      {#snippet actions()}
        <BtnSecondary onclick={handleCancel}>
          {#snippet icon()}<X size={12} />{/snippet}
          {$t("common.cancel")}
        </BtnSecondary>
        <BtnPrimary onclick={handleSave} disabled={saving || !formValid}>
          {#snippet icon()}<Check size={12} />{/snippet}
          {saving
            ? $t("triggers.creating")
            : formMode === "create"
              ? $t("triggers.create_trigger")
              : $t("common.save")}
        </BtnPrimary>
      {/snippet}
    </PageHeader>

    <div class="px-8 py-6 grid grid-cols-1 lg:grid-cols-[1fr_360px] gap-6">
      <!-- Form column -->
      <div class="space-y-6 min-w-0">
        <!-- Cible -->
        <section>
          <div
            class="text-[11px] font-semibold tracking-[1.5px] text-muted-foreground uppercase mb-2.5 font-mono"
          >
            Cible
          </div>
          <div
            class="inline-flex p-0.5 rounded-[9px] bg-surface-2 border border-border mb-2"
          >
            <button
              type="button"
              onclick={() => (targetKind = "agent")}
              class="px-3.5 py-1.5 rounded-md text-[12px] font-medium transition-colors {targetKind ===
              'agent'
                ? 'bg-card text-foreground shadow-elev-1'
                : 'text-muted-foreground'}"
            >
              {$t("triggers.field_target_agent")}
            </button>
            <button
              type="button"
              onclick={() => (targetKind = "pipeline")}
              class="px-3.5 py-1.5 rounded-md text-[12px] font-medium transition-colors {targetKind ===
              'pipeline'
                ? 'bg-card text-foreground shadow-elev-1'
                : 'text-muted-foreground'}"
            >
              {$t("triggers.field_target_pipeline")}
            </button>
          </div>
          {#if targetKind === "agent"}
            <select
              bind:value={selectedAgent}
              class="w-full px-3 py-2 rounded-lg bg-card border border-border text-[12.5px] text-foreground focus:outline-none focus:ring-2 focus:ring-primary/40"
              data-testid="trigger-input-agent"
            >
              <option value="" disabled>— {$t("triggers.field_target_agent")} —</option>
              {#each agents as agent}
                <option value={agent.name}>{agent.name}</option>
              {/each}
            </select>
          {:else}
            <select
              bind:value={selectedPipeline}
              class="w-full px-3 py-2 rounded-lg bg-card border border-border text-[12.5px] text-foreground focus:outline-none focus:ring-2 focus:ring-primary/40"
              data-testid="trigger-pipeline-select"
            >
              <option value="" disabled>— {$t("triggers.field_target_pipeline")} —</option>
              {#each pipelines as pipeline}
                <option value={pipeline.id}>{pipeline.id}</option>
              {/each}
            </select>
          {/if}
          <p class="mt-1 text-[11px] text-muted-foreground">
            L'agent qui recevra le message du trigger.
          </p>
        </section>

        <!-- Source picker -->
        <section>
          <div
            class="text-[11px] font-semibold tracking-[1.5px] text-muted-foreground uppercase mb-2.5 font-mono"
          >
            Type de source
          </div>
          <TriggerSourcePicker
            bind:value={sourceKind}
            bind:cronPreset
            showPresets={true}
            onCronPresetChange={(p) => {
              if (p.expr) cronSchedule = p.expr;
            }}
          />
        </section>

        <!-- Source-specific config -->
        <section>
          <div
            class="text-[11px] font-semibold tracking-[1.5px] text-muted-foreground uppercase mb-2.5 font-mono"
          >
            Configuration
          </div>
          {#if sourceKind === "cron"}
            <label class="block text-[11.5px] font-medium text-foreground mb-1.5">
              Expression cron
            </label>
            <input
              type="text"
              bind:value={cronSchedule}
              placeholder="0 9 * * *"
              class="w-full px-3 py-2 rounded-lg bg-card border border-border text-[12.5px] text-foreground font-mono focus:outline-none focus:ring-2 focus:ring-primary/40"
              data-testid="trigger-input-cron"
            />
            <p class="mt-1 text-[10.5px] text-muted-foreground font-mono">
              min hour day month weekday
            </p>
          {:else if sourceKind === "interval"}
            <label class="block text-[11.5px] font-medium text-foreground mb-1.5">
              Intervalle
            </label>
            <input
              type="text"
              bind:value={intervalEvery}
              placeholder="3600s"
              class="w-full px-3 py-2 rounded-lg bg-card border border-border text-[12.5px] text-foreground font-mono focus:outline-none focus:ring-2 focus:ring-primary/40"
              data-testid="trigger-input-interval"
            />
            <p class="mt-1 text-[10.5px] text-muted-foreground">
              Format : 30s, 15m, 1h, 1d
            </p>
          {:else if sourceKind === "once"}
            <label class="block text-[11.5px] font-medium text-foreground mb-1.5">
              Date et heure
            </label>
            <div class="grid grid-cols-2 gap-2">
              <input
                type="date"
                bind:value={oneshotDate}
                class="px-3 py-2 rounded-lg bg-card border border-border text-[12.5px] text-foreground focus:outline-none focus:ring-2 focus:ring-primary/40"
                data-testid="trigger-fire-at-input"
              />
              <input
                type="time"
                bind:value={oneshotTime}
                class="px-3 py-2 rounded-lg bg-card border border-border text-[12.5px] text-foreground focus:outline-none focus:ring-2 focus:ring-primary/40"
                data-testid="trigger-fire-at-time-input"
              />
            </div>
          {:else if sourceKind === "file"}
            <label class="block text-[11.5px] font-medium text-foreground mb-1.5">
              Dossier surveillé
            </label>
            <input
              type="text"
              bind:value={fileWatchPath}
              placeholder="~/Downloads"
              class="w-full px-3 py-2 rounded-lg bg-card border border-border text-[12.5px] text-foreground font-mono focus:outline-none focus:ring-2 focus:ring-primary/40"
              data-testid="trigger-input-filepath"
            />
            <div class="mt-3 flex gap-2 flex-wrap">
              <button
                type="button"
                onclick={() => (fileWatchCreate = !fileWatchCreate)}
                class="px-3 py-1 rounded-full text-[11px] font-medium border transition-colors {fileWatchCreate
                  ? 'border-primary bg-primary/10 text-primary'
                  : 'border-border text-muted-foreground hover:border-primary/40'}"
              >
                Création
              </button>
              <button
                type="button"
                onclick={() => (fileWatchModify = !fileWatchModify)}
                class="px-3 py-1 rounded-full text-[11px] font-medium border transition-colors {fileWatchModify
                  ? 'border-primary bg-primary/10 text-primary'
                  : 'border-border text-muted-foreground hover:border-primary/40'}"
              >
                Modification
              </button>
              <button
                type="button"
                onclick={() => (fileWatchDelete = !fileWatchDelete)}
                class="px-3 py-1 rounded-full text-[11px] font-medium border transition-colors {fileWatchDelete
                  ? 'border-destructive/80 bg-destructive/10 text-danger-a11y'
                  : 'border-border text-muted-foreground hover:border-destructive/40'}"
              >
                Suppression
              </button>
            </div>
          {:else if sourceKind === "webhook"}
            <label class="block text-[11.5px] font-medium text-foreground mb-1.5">
              Secret du webhook
            </label>
            <input
              type="text"
              bind:value={webhookSecret}
              placeholder="POST /hooks/<id> avec ce secret"
              class="w-full px-3 py-2 rounded-lg bg-card border border-border text-[12.5px] text-foreground font-mono focus:outline-none focus:ring-2 focus:ring-primary/40"
              data-testid="trigger-input-webhook-path"
            />
          {/if}
        </section>

        <!-- Advanced -->
        <section>
          <div
            class="text-[11px] font-semibold tracking-[1.5px] text-muted-foreground uppercase mb-2.5 font-mono"
          >
            Paramètres avancés
          </div>
          <label class="block text-[11.5px] font-medium text-foreground mb-1.5">
            ID du trigger
          </label>
          <input
            type="text"
            bind:value={triggerIdField}
            oninput={() => (idCustomized = true)}
            disabled={formMode === "edit"}
            class="w-full px-3 py-2 rounded-lg bg-card border border-border text-[12.5px] text-foreground font-mono focus:outline-none focus:ring-2 focus:ring-primary/40 disabled:opacity-60"
            data-testid="trigger-input-name"
          />
          {#if !idValid && triggerIdField}
            <p class="mt-1 text-[11px] text-danger-a11y">
              {$t("triggers.field_id_invalid")}
            </p>
          {/if}

          <label
            class="block text-[11.5px] font-medium text-foreground mb-1.5 mt-4"
          >
            Template d'entrée
            <span class="text-muted-foreground font-normal">(optionnel)</span>
          </label>
          <textarea
            bind:value={inputTemplate}
            rows="3"
            placeholder={"Bonjour {{date}} — passe en revue mes priorités du jour."}
            class="w-full px-3 py-2 rounded-lg bg-card border border-border text-[12px] text-foreground font-mono focus:outline-none focus:ring-2 focus:ring-primary/40 resize-y"
          ></textarea>
          <p class="mt-1 text-[10.5px] text-muted-foreground font-mono">
            Variables : {`{{date}}`} · {`{{time}}`} · {`{{trigger.id}}`} · {`{{file.path}}`}
          </p>

          <div class="mt-4 flex items-center gap-3">
            <label
              class="text-[11.5px] font-medium text-foreground"
              for="trigger-on-busy"
            >
              Si occupé :
            </label>
            <select
              id="trigger-on-busy"
              bind:value={onBusy}
              class="px-3 py-1.5 rounded-lg bg-card border border-border text-[12px] text-foreground focus:outline-none focus:ring-2 focus:ring-primary/40"
            >
              <option value="queue">{$t("triggers.field_on_busy_queue")}</option>
              <option value="drop">{$t("triggers.field_on_busy_drop")}</option>
            </select>

            <label class="ml-auto flex items-center gap-2 text-[12px] text-foreground">
              <input
                type="checkbox"
                bind:checked={enabled}
                class="w-4 h-4 rounded border-border accent-primary"
              />
              {$t("triggers.field_enabled")}
            </label>
          </div>
        </section>

        {#if formError}
          <div
            class="rounded-md border border-destructive bg-destructive/10 px-4 py-2 text-sm text-danger-a11y"
            data-testid="create-trigger-error"
          >
            {formError}
          </div>
        {/if}
      </div>

      <!-- Live preview column -->
      <aside class="space-y-4">
        <div
          class="text-[10.5px] tracking-[1.5px] text-muted-foreground font-mono font-semibold uppercase"
        >
          Aperçu en direct
        </div>

        <div class="bg-card border border-border rounded-xl overflow-hidden">
          <div
            class="h-0.5"
            style="background: hsl(var({enabled ? '--success' : '--muted-foreground'}));"
          ></div>
          <div class="px-3.5 py-3">
            <div class="flex items-center gap-2 mb-1.5 flex-wrap">
              <div
                class="w-7 h-7 rounded-lg bg-primary/10 text-primary inline-flex items-center justify-center"
              >
                <CalendarIcon size={13} />
              </div>
              <span class="text-[12px] font-mono font-semibold text-foreground truncate">
                {triggerIdField || "nouveau-trigger"}
              </span>
              <Chip
                size="sm"
                tone={enabled ? "success" : "neutral"}
              >
                {#snippet icon()}<StatusDot
                    color={enabled ? "hsl(var(--success))" : "hsl(var(--muted-foreground))"}
                    glow={enabled}
                  />{/snippet}
                {enabled ? "actif" : "en pause"}
              </Chip>
            </div>
            <div class="text-[11.5px] text-muted-foreground mb-2">
              {humanizeTrigger(
                uiToBackend(sourceKind),
                buildSource()
                  ? sourceKind === "cron"
                    ? cronSchedule
                    : sourceKind === "interval"
                      ? intervalEvery
                      : sourceKind === "once"
                        ? `${oneshotDate}T${oneshotTime}`
                        : sourceKind === "file"
                          ? fileWatchPath
                          : webhookSecret
                  : "",
                $locale ?? "fr",
              )}
            </div>
          </div>
        </div>

        <div
          class="text-[10.5px] tracking-[1.5px] text-muted-foreground font-mono font-semibold uppercase"
        >
          Prochains déclenchements
        </div>
        <div class="bg-card border border-border rounded-xl px-3.5 py-2">
          {#if previewFires.length === 0}
            <div class="py-2 text-[11.5px] text-muted-foreground italic">
              {sourceKind === "webhook" || sourceKind === "file"
                ? "Déclenchement à la demande — pas de planning."
                : "Configurez la source pour voir les prochaines exécutions."}
            </div>
          {:else}
            {#each previewFires.slice(0, 3) as d, i}
              <div
                class="flex items-center gap-2.5 py-1.5 text-[11.5px] text-muted-foreground {i ===
                0
                  ? ''
                  : 'border-t border-border/50'}"
              >
                <StatusDot
                  color={i === 0
                    ? "hsl(var(--primary))"
                    : "hsl(var(--muted-foreground))"}
                  glow={i === 0}
                />
                <span class="capitalize">{fmtDate(d)}</span>
              </div>
            {/each}
          {/if}
        </div>

        <div
          class="rounded-xl bg-warning/10 px-3 py-2.5 text-[11px] text-warning-foreground flex gap-2"
        >
          <ShieldAlert size={13} class="shrink-0 mt-px" />
          <span class="text-warning-foreground/90">
            Ce trigger envoie un message à l'agent. L'agent décide ensuite s'il agit ou
            vous demande votre accord.
          </span>
        </div>

        <div class="flex justify-end gap-2 pt-2">
          <BtnSecondary onclick={handleCancel}>
            {#snippet icon()}<X size={12} />{/snippet}
            {$t("common.cancel")}
          </BtnSecondary>
          <BtnPrimary onclick={handleSave} disabled={saving || !formValid}>
            {#snippet icon()}<Play size={12} />{/snippet}
            {saving
              ? $t("triggers.creating")
              : formMode === "create"
                ? $t("triggers.create_trigger")
                : $t("common.save")}
          </BtnPrimary>
        </div>
      </aside>
    </div>
  {/if}
</div>

<!-- Logs sheet (preserved) -->
{#if logsTriggerId}
  <TriggerLogs
    triggerId={logsTriggerId}
    open={logsOpen}
    onclose={() => (logsTriggerId = null)}
  />
{/if}

<!-- Delete confirmation dialog (preserved) -->
<ConfirmDialog
  open={deleteTriggerId !== null}
  onclose={() => (deleteTriggerId = null)}
  onconfirm={handleConfirmDelete}
  title={$t("triggers.delete_confirm_title")}
  message={$t("triggers.delete_confirm_message", {
    values: { id: deleteTriggerId ?? "" },
  })}
  confirmLabel={$t("triggers.delete_confirm_yes")}
  cancelLabel={$t("common.cancel")}
  loading={deleting}
  data-testid="delete-trigger-confirm"
/>
