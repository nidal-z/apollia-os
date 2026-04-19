<script lang="ts">
  /**
   * US-SP42-044 — A2A skill panel with tabs (Overview, Telemetry, Steps, Logs).
   *
   * Refactors the former worker panel into a tabbed drill-down surface:
   *   - Overview: worker/skill identity + advertised version
   *   - Telemetry: rolling-window aggregates (invocations, avg latency, success rate, tokens)
   *   - Steps: [`A2AStepProvenance`] entries, clickable to jump to the matching step in
   *     the TimelineGlobal (step_id is the shared correlation key with US-SP42-048).
   *   - Logs: raw A2A runtime events, filtered by skill.
   *
   * If `requiredVersion` is provided, the component triggers a semver compatibility
   * check and displays an `A2ACompatibilityBanner` on mismatch. Telemetry and steps
   * are refreshed on mount, when the tab changes, and on each `a2a` runtime event.
   */
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { Activity, ListTree, Info, ScrollText } from "lucide-svelte";
  import { TabBar } from "$lib/components/ui/tabs";
  import type {
    A2ASkillListing,
    A2ASkillTelemetry,
    A2AStepProvenance,
    A2ACompatibilityWarning,
  } from "$lib/types";
  import A2AStepTree from "./A2AStepTree.svelte";
  import A2ACompatibilityBanner from "./A2ACompatibilityBanner.svelte";

  interface Props {
    skill: A2ASkillListing;
    requiredVersion?: string;
    onUseAlternative?: (agentName: string) => void;
  }

  let { skill, requiredVersion, onUseAlternative }: Props = $props();

  type TabKey = "overview" | "telemetry" | "steps" | "logs";
  let activeTab = $state<TabKey>("overview");

  let telemetry = $state<A2ASkillTelemetry | null>(null);
  let steps = $state<A2AStepProvenance[]>([]);
  let warning = $state<A2ACompatibilityWarning | null>(null);
  let logs = $state<string[]>([]);
  let loading = $state(false);
  let unlistenFns: UnlistenFn[] = [];

  const tabs = $derived([
    { key: "overview", label: "Overview" },
    {
      key: "telemetry",
      label: "Telemetry",
      count: telemetry?.invocations,
    },
    { key: "steps", label: "Steps", count: steps.length },
    { key: "logs", label: "Logs", count: logs.length },
  ]);

  async function refreshTelemetry(): Promise<void> {
    try {
      const all = await invoke<A2ASkillTelemetry[]>(
        "list_a2a_skill_telemetry",
      ).catch(() => [] as A2ASkillTelemetry[]);
      telemetry = all.find((t) => t.skill_name === skill.skill_id) ?? null;
    } catch {
      telemetry = null;
    }
  }

  async function refreshSteps(): Promise<void> {
    try {
      steps = await invoke<A2AStepProvenance[]>("list_a2a_step_provenance", {
        skillId: skill.skill_id,
      }).catch(() => [] as A2AStepProvenance[]);
    } catch {
      steps = [];
    }
  }

  async function refreshCompat(): Promise<void> {
    if (!requiredVersion) {
      warning = null;
      return;
    }
    try {
      warning = await invoke<A2ACompatibilityWarning | null>(
        "check_a2a_compatibility",
        { skillId: skill.skill_id, requiredVersion },
      );
    } catch {
      warning = null;
    }
  }

  async function refreshAll(): Promise<void> {
    loading = true;
    try {
      await Promise.all([refreshTelemetry(), refreshSteps(), refreshCompat()]);
    } finally {
      loading = false;
    }
  }

  function appendLog(message: string): void {
    const ts = new Date().toISOString().split("T")[1]?.slice(0, 12) ?? "";
    logs = [`[${ts}] ${message}`, ...logs].slice(0, 200);
  }

  onMount(async () => {
    await refreshAll();
    try {
      const u = await listen<{ variant?: string; payload?: unknown }>(
        "a2a",
        (evt) => {
          const payload = evt.payload as
            | { variant?: string; event?: Record<string, unknown> }
            | undefined;
          const variant = payload?.variant ?? "";
          const data = (payload?.event ?? {}) as Record<string, unknown>;
          if (data["skill_id"] && data["skill_id"] !== skill.skill_id) return;
          appendLog(`${variant} ${JSON.stringify(data)}`);
          void refreshTelemetry();
          void refreshSteps();
          if (variant === "A2ACompatibilityWarning") void refreshCompat();
        },
      );
      unlistenFns.push(u);
    } catch {
      // Event bridge unavailable — manual refresh only.
    }
  });

  onDestroy(() => {
    for (const u of unlistenFns) u();
  });

  function handleTabChange(key: string): void {
    activeTab = key as TabKey;
    if (key === "telemetry") void refreshTelemetry();
    if (key === "steps") void refreshSteps();
  }

  function formatPercent(v: number): string {
    return `${Math.round(v * 1000) / 10}%`;
  }
</script>

<div
  class="flex flex-col gap-2 rounded-lg border border-border/40 bg-background p-3"
  data-testid="a2a-skill-view"
  data-skill-id={skill.skill_id}
>
  <div class="flex items-baseline justify-between gap-2">
    <div class="min-w-0">
      <h3 class="truncate text-sm font-semibold text-foreground">
        {skill.skill_name || skill.skill_id}
      </h3>
      <p class="truncate text-[11px] text-muted-foreground">
        {skill.agent_name}
        {#if telemetry?.version}
          <span class="ml-1 rounded bg-muted px-1 text-[10px] text-muted-foreground/80">
            v{telemetry.version}
          </span>
        {/if}
      </p>
    </div>
  </div>

  {#if warning}
    <A2ACompatibilityBanner {warning} {onUseAlternative} />
  {/if}

  <TabBar
    items={tabs}
    activeTab={activeTab}
    ontabchange={handleTabChange}
    testidPrefix="a2a-skill-view"
  />

  {#if activeTab === "overview"}
    <div class="flex flex-col gap-2 text-[11px]">
      <div class="flex items-center gap-1.5 text-muted-foreground">
        <Info size={12} />
        <span>Skill ID: <code class="text-foreground">{skill.skill_id}</code></span>
      </div>
      {#if skill.description}
        <p class="text-muted-foreground">{skill.description}</p>
      {/if}
      {#if requiredVersion}
        <p class="text-muted-foreground">
          Required version: <code class="text-foreground">{requiredVersion}</code>
        </p>
      {/if}
    </div>
  {:else if activeTab === "telemetry"}
    <div
      class="grid grid-cols-2 gap-2 text-[11px]"
      data-testid="a2a-skill-view-telemetry"
    >
      {#if telemetry}
        <div class="rounded-md border border-border/30 p-2">
          <p class="text-muted-foreground/60">Invocations</p>
          <p class="text-sm font-semibold text-foreground">{telemetry.invocations}</p>
        </div>
        <div class="rounded-md border border-border/30 p-2">
          <p class="text-muted-foreground/60">Avg latency</p>
          <p class="text-sm font-semibold text-foreground">{telemetry.avg_latency_ms} ms</p>
        </div>
        <div class="rounded-md border border-border/30 p-2">
          <p class="text-muted-foreground/60">Success rate</p>
          <p class="text-sm font-semibold text-foreground">
            {formatPercent(telemetry.success_rate)}
          </p>
        </div>
        <div class="rounded-md border border-border/30 p-2">
          <p class="text-muted-foreground/60">Tokens</p>
          <p class="text-sm font-semibold text-foreground">{telemetry.tokens_consumed}</p>
        </div>
      {:else if loading}
        <p class="col-span-2 text-muted-foreground/70">
          <Activity size={12} class="inline" /> Loading telemetry…
        </p>
      {:else}
        <p class="col-span-2 text-muted-foreground/70">No invocations recorded yet.</p>
      {/if}
    </div>
  {:else if activeTab === "steps"}
    <div data-testid="a2a-skill-view-steps">
      <A2AStepTree {steps} />
    </div>
  {:else if activeTab === "logs"}
    <div
      class="flex max-h-48 flex-col gap-0.5 overflow-y-auto rounded-md border border-border/30 bg-muted/20 p-1 font-mono text-[10px]"
      data-testid="a2a-skill-view-logs"
    >
      {#if logs.length === 0}
        <p class="px-2 py-2 text-muted-foreground/70">
          <ScrollText size={11} class="inline" /> No runtime events yet.
        </p>
      {:else}
        {#each logs as line, i (i)}
          <p class="whitespace-pre-wrap break-all text-muted-foreground">{line}</p>
        {/each}
      {/if}
    </div>
  {/if}
  <ListTree size={0} class="hidden" aria-hidden="true" />
</div>
