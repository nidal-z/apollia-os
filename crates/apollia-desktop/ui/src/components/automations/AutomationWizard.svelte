<script lang="ts">
  /**
   * Natural-language automation wizard.
   *
   * Four steps :
   *   1. Describe — free-form textarea + templates shortcut.
   *   2. Schedule — human label preview + natural re-ask if confidence is low.
   *   3. Agent    — pre-selected if matched, dropdown otherwise.
   *   4. Preview  — final card + "Activer cette automatisation" CTA.
   *
   * Invokes `meta_parse_automation` (Tauri IPC) to translate the free-form
   * description into a `ParsedAutomation` payload. Never exposes a raw cron
   * expression to the operator ; the builder path (`CreateTriggerDialog`) stays
   * available behind the "Mode avancé" toggle.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { t, locale } from "svelte-i18n";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Select } from "$lib/components/ui/select";
  import { Input } from "$lib/components/ui/input";
  import { Stepper } from "$lib/components/ui/stepper";
  import type { AgentListItem, CreateTriggerRequest, TriggerDefinitionView } from "$lib/types";
  import {
    parseAutomationDescription,
    type ParsedAutomation,
    type ParsedSchedule,
  } from "$lib/automations/wizardClient";

  interface Props {
    open: boolean;
    onclose: () => void;
    oncreated: (triggerId: string) => void;
    /** Opens the advanced builder dialog (operator/builder bridge). */
    onswitchadvanced?: () => void;
  }

  let { open, onclose, oncreated, onswitchadvanced }: Props = $props();

  // ── State ──────────────────────────────────────────────────────────────
  let step = $state(0);
  let description = $state("");
  let parsed = $state<ParsedAutomation | null>(null);
  let parseError = $state<string | null>(null);
  let parsing = $state(false);
  let agents = $state<AgentListItem[]>([]);
  let selectedAgent = $state<string>("");
  let creating = $state(false);
  let createError = $state<string | null>(null);
  let telemetryStepStartedAt = $state<number>(Date.now());
  let usedTemplate = $state(false);

  const PLACEHOLDERS = [
    "automations.wizard.placeholder_1",
    "automations.wizard.placeholder_2",
    "automations.wizard.placeholder_3",
  ];
  const placeholderKey = $derived(PLACEHOLDERS[Date.now() % PLACEHOLDERS.length]);

  const stepLabels = $derived([
    { label: $t("automations.wizard.step_describe") },
    { label: $t("automations.wizard.step_schedule") },
    { label: $t("automations.wizard.step_agent") },
    { label: $t("automations.wizard.step_preview") },
  ]);

  const humanSchedule = $derived.by(() => {
    const s: ParsedSchedule | undefined = parsed?.schedule ?? undefined;
    if (!s) return "";
    return ($locale ?? "en").startsWith("fr") ? s.human_label_fr : s.human_label_en;
  });

  const nextRun = $derived.by(() => {
    const s: ParsedSchedule | undefined = parsed?.schedule ?? undefined;
    if (!s) return null;
    try {
      return new Date(s.next_run_at).toLocaleString($locale ?? undefined);
    } catch {
      return s.next_run_at;
    }
  });

  const canAdvanceFromDescribe = $derived(description.trim().length >= 4);
  const unresolvedAmbiguities = $derived(parsed?.ambiguities ?? []);
  const canAdvanceFromSchedule = $derived(!!parsed?.schedule && unresolvedAmbiguities.length === 0);
  const canAdvanceFromAgent = $derived(!!selectedAgent);

  // ── Lifecycle ──────────────────────────────────────────────────────────
  $effect(() => {
    if (open) {
      step = 0;
      description = "";
      parsed = null;
      parseError = null;
      selectedAgent = "";
      createError = null;
      usedTemplate = false;
      telemetryStepStartedAt = Date.now();
      void loadAgents();
    }
  });

  async function loadAgents(): Promise<void> {
    try {
      agents = await invoke<AgentListItem[]>("list_agents");
    } catch {
      agents = [];
    }
  }

  // ── Telemetry (local, never uploaded) ─────────────────────────────────
  function emitStepCompleted(stepIndex: number) {
    try {
      const payload = {
        step: stepIndex,
        duration_ms: Date.now() - telemetryStepStartedAt,
        used_template: usedTemplate,
        confidence: parsed?.confidence ?? null,
      };
      // Dispatch a window event so the observability store (if mounted) can
      // forward it to the local telemetry buffer. No network IO here.
      window.dispatchEvent(
        new CustomEvent("automation_wizard_step_completed", { detail: payload }),
      );
    } catch {
      /* best effort */
    }
    telemetryStepStartedAt = Date.now();
  }

  // ── Actions ────────────────────────────────────────────────────────────
  async function runParser() {
    parsing = true;
    parseError = null;
    const knownAgents = agents.map((a) => a.name);
    const res = await parseAutomationDescription(description.trim(), knownAgents);
    parsing = false;
    if (!res.ok) {
      parseError = res.error;
      parsed = null;
      return;
    }
    parsed = res.result;
    if (parsed.agent && parsed.confidence !== "low") {
      selectedAgent = parsed.agent.agent_id;
    }
  }

  async function handleDescribeNext() {
    if (!canAdvanceFromDescribe) return;
    await runParser();
    if (parseError) return;
    emitStepCompleted(0);
    step = 1;
  }

  function handleRefine(extraHint: string) {
    if (!extraHint.trim()) return;
    description = `${description}\n${extraHint.trim()}`;
    void runParser();
  }

  function handleScheduleNext() {
    if (!canAdvanceFromSchedule) return;
    emitStepCompleted(1);
    step = 2;
  }

  function handleAgentNext() {
    if (!canAdvanceFromAgent) return;
    emitStepCompleted(2);
    step = 3;
  }

  function handleBack() {
    if (step > 0) step -= 1;
  }

  function buildSourceInput(): CreateTriggerRequest["source"] | null {
    const s = parsed?.schedule;
    if (!s) return null;
    if (s.type === "cron") return { type: "cron", schedule: s.expr };
    if (s.type === "interval") return { type: "interval", every: s.every };
    if (s.type === "one_shot") return { type: "oneshot", fire_at: s.fire_at };
    return null;
  }

  function buildTriggerId(): string {
    // Derive a stable, operator-friendly id from the description.
    return description
      .trim()
      .toLowerCase()
      .normalize("NFD")
      .replace(/[\u0300-\u036f]/g, "")
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 40) || `automation-${Date.now().toString(36)}`;
  }

  async function handleActivate() {
    const source = buildSourceInput();
    if (!source || !selectedAgent) return;
    creating = true;
    createError = null;
    try {
      const request: CreateTriggerRequest = {
        id: buildTriggerId(),
        agent: selectedAgent,
        enabled: true,
        on_busy: "queue",
        source,
      };
      if (parsed?.payload) {
        request.input_template = parsed.payload;
      }
      const created = await invoke<TriggerDefinitionView>("create_trigger", {
        definition: request,
      });
      emitStepCompleted(3);
      oncreated(created.id);
      onclose();
    } catch (err) {
      createError = err instanceof Error ? err.message : String(err);
    } finally {
      creating = false;
    }
  }

  function handleBrowseTemplates() {
    usedTemplate = true;
    // The gallery lives in surface the hand-off via a window
    // event so the parent route (Automations.svelte) can open it without
    // coupling the wizard to route-level state.
    window.dispatchEvent(new CustomEvent("automation_wizard_browse_templates"));
  }

  function handleAdvanced() {
    onclose();
    onswitchadvanced?.();
  }
</script>

<Dialog
  {open}
  {onclose}
  size="lg"
  title={$t("automations.wizard.title")}
  data-testid="automation-wizard-dialog"
>
  <Stepper steps={stepLabels} current={step} />

  <div class="mt-6 space-y-4">
    {#if step === 0}
      <!-- Step 1 — Describe ------------------------------------------------->
      <div>
        <label class="mb-1 block text-sm font-medium" for="automation-describe">
          {$t("automations.wizard.describe_prompt")}
        </label>
        <Textarea
          id="automation-describe"
          class="min-h-32"
          placeholder={$t(placeholderKey)}
          bind:value={description}
          data-testid="automation-wizard-describe"
        />
        <div class="mt-2 flex items-center justify-between">
          <Button variant="ghost" size="sm" onclick={handleBrowseTemplates} data-testid="automation-wizard-templates">
            {$t("automations.wizard.browse_templates")}
          </Button>
          {#if parseError}
            <p class="text-xs text-destructive" data-testid="automation-wizard-parse-error">
              {$t("automations.wizard.fallback_error")}
            </p>
          {/if}
        </div>
      </div>
    {:else if step === 1}
      <!-- Step 2 — Schedule ------------------------------------------------->
      <div class="space-y-3">
        <p class="text-sm text-muted-foreground">{$t("automations.wizard.schedule_intro")}</p>
        {#if parsed?.schedule}
          <div class="rounded-md border border-border bg-muted/30 p-4" data-testid="automation-wizard-schedule-card">
            <p class="text-base font-medium">{humanSchedule}</p>
            {#if nextRun}
              <p class="mt-1 text-xs text-muted-foreground">
                {$t("automations.wizard.next_run", { values: { when: nextRun } })}
              </p>
            {/if}
          </div>
        {/if}

        {#if unresolvedAmbiguities.length > 0}
          <div class="rounded-md border border-amber-500/40 bg-amber-500/10 p-3" data-testid="automation-wizard-ambiguities">
            <p class="mb-1 text-xs font-medium text-amber-700 dark:text-amber-300">
              {$t("automations.wizard.needs_clarification")}
            </p>
            <ul class="list-disc space-y-0.5 pl-5 text-xs text-amber-900 dark:text-amber-100">
              {#each unresolvedAmbiguities as ambiguity}
                <li>{ambiguity}</li>
              {/each}
            </ul>
          </div>
        {/if}

        <div>
          <label class="mb-1 block text-xs text-muted-foreground" for="automation-refine">
            {$t("automations.wizard.refine_label")}
          </label>
          <Input
            id="automation-refine"
            placeholder={$t("automations.wizard.refine_placeholder")}
            onkeydown={(e: KeyboardEvent) => {
              if (e.key === "Enter") {
                handleRefine((e.target as HTMLInputElement).value);
                (e.target as HTMLInputElement).value = "";
              }
            }}
            data-testid="automation-wizard-refine"
          />
        </div>
      </div>
    {:else if step === 2}
      <!-- Step 3 — Agent ---------------------------------------------------->
      <div class="space-y-3">
        <p class="text-sm text-muted-foreground">{$t("automations.wizard.agent_intro")}</p>
        <Select bind:value={selectedAgent} aria-label={$t("automations.wizard.agent_select_aria")} data-testid="automation-wizard-agent-select">
          <option value="" disabled>— {$t("automations.wizard.agent_pick")} —</option>
          {#each agents as agent}
            <option value={agent.name}>{agent.name}</option>
          {/each}
        </Select>
        {#if parsed?.agent}
          <p class="text-xs text-muted-foreground">
            {$t("automations.wizard.agent_inferred", { values: { agent: parsed.agent.agent_id } })}
          </p>
        {/if}
      </div>
    {:else if step === 3}
      <!-- Step 4 — Preview -------------------------------------------------->
      <div class="space-y-3">
        <p class="text-sm text-muted-foreground">{$t("automations.wizard.preview_intro")}</p>
        <div class="rounded-md border border-border bg-background p-4 shadow-sm" data-testid="automation-wizard-preview-card">
          <p class="text-xs uppercase tracking-wide text-muted-foreground">
            {$t("automations.wizard.preview_label")}
          </p>
          <p class="mt-1 text-base font-medium">{humanSchedule}</p>
          <p class="mt-2 text-sm">
            <span class="text-muted-foreground">{$t("automations.target_prefix")}:</span>
            <span class="ml-1 font-medium">{selectedAgent}</span>
          </p>
          {#if parsed?.payload}
            <p class="mt-2 text-xs text-muted-foreground">
              {$t("automations.wizard.preview_payload", { values: { payload: parsed.payload } })}
            </p>
          {/if}
        </div>
        {#if createError}
          <p class="text-sm text-destructive" data-testid="automation-wizard-create-error">{createError}</p>
        {/if}
      </div>
    {/if}
  </div>

  <DialogFooter>
    <Button
      variant="ghost"
      size="sm"
      onclick={handleAdvanced}
      data-testid="automation-wizard-advanced"
    >
      {$t("automations.wizard.switch_advanced")}
    </Button>
    <div class="flex-1"></div>
    {#if step > 0}
      <Button variant="outline" onclick={handleBack} data-testid="automation-wizard-back">
        {$t("common.back")}
      </Button>
    {/if}
    {#if step === 0}
      <Button
        onclick={handleDescribeNext}
        disabled={!canAdvanceFromDescribe || parsing}
        data-testid="automation-wizard-next"
      >
        {parsing ? $t("automations.wizard.parsing") : $t("common.next")}
      </Button>
    {:else if step === 1}
      <Button
        onclick={handleScheduleNext}
        disabled={!canAdvanceFromSchedule}
        data-testid="automation-wizard-next"
      >
        {$t("common.next")}
      </Button>
    {:else if step === 2}
      <Button
        onclick={handleAgentNext}
        disabled={!canAdvanceFromAgent}
        data-testid="automation-wizard-next"
      >
        {$t("common.next")}
      </Button>
    {:else}
      <Button
        onclick={handleActivate}
        disabled={creating}
        data-testid="automation-wizard-activate"
      >
        {creating ? $t("triggers.creating") : $t("automations.wizard.activate")}
      </Button>
    {/if}
  </DialogFooter>
</Dialog>
