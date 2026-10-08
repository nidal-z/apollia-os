<script lang="ts">
  /**
   * AgentSheet - the sheet of an agent, read without code.
   *
   * Drawn from the agent the runtime resolves (package definition, then the
   * user's overrides), in this order: mission and examples; model and where
   * it runs, with the fallback notice; accesses grouped by action family;
   * sources, the profile among them; the standing consents ("always") with
   * their withdrawal; activity and changes; memory. The
   * built-in Apollia agent has the same sheet as any other.
   *
   * When the runtime holds no definition for an agent, the sheet falls back
   * on what the agent's code declares (description, examples, tools), so an
   * agent installed before definitions existed still reads.
   *
   * While the adjustment session the pane hands over is active, the mission,
   * the model, the accesses and the sources become editable on the same page;
   * a section whose value is the user's rather than the package's is marked.
   */
  import { t } from "svelte-i18n";
  import { llmBackends } from "$lib/stores/llm";
  import { describeTool, toolKindTag } from "$lib/ipc/tools";
  import { listTasks } from "$lib/ipc/tasks";
  import { listTools } from "$lib/ipc/tools";
  import { listMcpExposable } from "$lib/ipc/mcpServer";
  import { listConnections } from "$lib/ipc/connections";
  import { sortConnections } from "$lib/connections/connectionList";
  import { listModelPresets } from "$lib/ipc/agentAdjust";
  import {
    getAgentDefinition,
    getAgentModelFallback,
    listAgentChanges,
    type AgentChangeEntry,
    type AgentModelFallback,
    type ResolvedAgent,
  } from "$lib/ipc/agentSheet";
  import { createIdentityGuard } from "$lib/utils/identityGuard";
  import type { AgentListItem, Connection, TaskSummary } from "$lib/types";
  import { TabBar } from "$lib/components/ui/tabs";
  import SheetMission from "./SheetMission.svelte";
  import SheetModel from "./SheetModel.svelte";
  import SheetAccess from "./SheetAccess.svelte";
  import SheetAlways from "./SheetAlways.svelte";
  import SheetSources from "./SheetSources.svelte";
  import SheetActivity from "./SheetActivity.svelte";
  import SheetMemory from "./SheetMemory.svelte";
  import SheetTechnical from "./SheetTechnical.svelte";
  import AdjustMission from "./AdjustMission.svelte";
  import AdjustModel from "./AdjustModel.svelte";
  import AdjustAccess from "./AdjustAccess.svelte";
  import AdjustSources from "./AdjustSources.svelte";
  import type { AgentAdjust } from "./useAgentAdjust.svelte";
  import { createAgentSources } from "./useAgentSources.svelte";
  import { awaitsIndex, sourceRows } from "./agentSources";
  import {
    grantCandidates,
    isOverridden,
    listedAccesses,
    modelOptions,
    type GrantCandidate,
    type MarkedField,
    type PresetOffer,
  } from "./agentAdjust";
  import { connectionToolFamily, remoteAccesses, type ConnectionOffer } from "./connectionGrant";
  import {
    accessView,
    changeViews,
    fallbackNotice,
    groupAccesses,
    missionView,
    modelView,
    readableName,
    type AccessDescriptor,
    type AccessView,
  } from "./agentSheet";

  interface Props {
    /** Identifier of the agent's definition: its name, or `apollia:chat`. */
    definitionId: string;
    /** The agent as the registry lists it, `null` for the built-in Apollia agent. */
    agent: AgentListItem | null;
    /** Memory namespace of the agent, `null` when it keeps none of its own. */
    memoryNamespace: string | null;
    onOpenLogs: (agentId: string) => void;
    /** The adjustment session of the pane, `null` when the sheet is read only. */
    adjust?: AgentAdjust | null;
  }

  let { definitionId, agent, memoryNamespace, onOpenLogs, adjust = null }: Props = $props();

  const RECENT_RUNS = 10;

  let resolved = $state<ResolvedAgent | null>(null);
  let fallback = $state<AgentModelFallback | null>(null);
  let changes = $state<AgentChangeEntry[]>([]);
  let accesses = $state<AccessView[]>([]);
  let accessLoading = $state(true);
  let connections = $state<Connection[]>([]);
  let runs = $state<TaskSummary[]>([]);
  let runsLoading = $state(false);
  let runsError = $state<string | null>(null);

  const runtimeId = $derived(agent?.id ?? null);
  const guard = createIdentityGuard(() => definitionId);
  const runsGuard = createIdentityGuard(() => runtimeId);

  $effect(() => {
    void adjust?.revision;
    void loadDefinition(definitionId);
  });

  $effect(() => {
    void loadRuns(runtimeId);
  });

  // The registry of sources and their indexes, read again while one of the
  // sources shown is still being read.
  const SOURCES_POLL_MS = 4000;
  const registry = createAgentSources();

  $effect(() => {
    void adjust?.revision;
    void adjust?.active;
    void definitionId;
    void registry.refresh();
  });

  $effect(() => {
    if (!sourcesPending) return;
    const timer = setInterval(() => void registry.refresh(), SOURCES_POLL_MS);
    return () => clearInterval(timer);
  });

  async function loadDefinition(id: string): Promise<void> {
    const ticket = guard.begin();
    accessLoading = true;
    const [definition, lastFallback, history, connected] = await Promise.all([
      getAgentDefinition(id).catch(() => null),
      getAgentModelFallback(id).catch(() => null),
      listAgentChanges(id).catch(() => [] as AgentChangeEntry[]),
      listConnections().catch(() => [] as Connection[]),
    ]);
    if (!ticket.current) return;
    resolved = definition;
    fallback = lastFallback;
    changes = history;
    connections = connected;
    const ids = definition ? definition.access : [];
    const views = await Promise.all(ids.map((accessId) => describeAccess(accessId)));
    if (!ticket.current) return;
    accesses = views;
    accessLoading = false;
  }

  async function describeAccess(id: string): Promise<AccessView> {
    if (id.startsWith("agent:") || id.startsWith("workflow:")) return accessView(id, null);
    if (id.startsWith("mcp:")) {
      const family = connectionToolFamily(id, connections);
      return accessView(id, family ? { description: "", family, family_source: "server", kind: "mcp" } : null);
    }
    const descriptor = await describeTool(id).catch(() => null);
    const read: AccessDescriptor | null = descriptor
      ? {
          description: descriptor.description,
          family: descriptor.family,
          family_source: descriptor.family_source,
          kind: toolKindTag(descriptor.kind),
        }
      : null;
    return accessView(id, read);
  }

  async function loadRuns(id: string | null): Promise<void> {
    const ticket = runsGuard.begin();
    if (!id) {
      runs = [];
      runsLoading = false;
      runsError = null;
      return;
    }
    runsLoading = true;
    runsError = null;
    try {
      const listed = await listTasks({ agent_id: id });
      if (!ticket.current) return;
      runs = [...listed]
        .sort((a, b) => b.created_at.localeCompare(a.created_at))
        .slice(0, RECENT_RUNS);
    } catch (err) {
      if (!ticket.current) return;
      runsError = err instanceof Error ? err.message : String(err);
      runs = [];
    } finally {
      if (ticket.current) runsLoading = false;
    }
  }

  const defaultModel = $derived(
    ($llmBackends.find((b) => b.is_default) ?? $llmBackends[0] ?? null)?.model ?? null,
  );
  const mission = $derived(
    missionView(resolved, agent ? { description: agent.description, examples: agent.examples } : null),
  );
  const model = $derived(modelView(resolved?.model ?? null, defaultModel));
  const notice = $derived(fallbackNotice(fallback));
  const groups = $derived(groupAccesses(accesses));
  const sources = $derived(sourceRows(registry.registry, registry.indexes, resolved?.sources ?? [], false));
  const sourcesPending = $derived(
    awaitsIndex(
      sourceRows(registry.registry, registry.indexes, adjust?.draft?.sources ?? resolved?.sources ?? [], false),
    ),
  );
  const changeRows = $derived(changeViews(changes));

  // ── Adjustment ──────────────────────────────────────────────────────
  const adjusting = $derived(adjust !== null && adjust.active && adjust.draft !== null);

  // The tabs of the sheet. Adjusting brings the first one back: it is the
  // only one that holds something to change.
  type SheetTab = "sheet" | "activity" | "memory" | "technical";
  let asked = $state<SheetTab>("sheet");
  const tab = $derived<SheetTab>(
    adjusting || (asked === "technical" && agent === null) ? "sheet" : asked,
  );
  const tabs = $derived(
    (agent ? (["sheet", "activity", "memory", "technical"] as const) : (["sheet", "activity", "memory"] as const)).map(
      (key) => ({ key, label: $t(TAB_KEYS[key]) }),
    ),
  );
  let presets = $state<PresetOffer[]>([]);
  let extraAccesses = $state<AccessView[]>([]);

  $effect(() => {
    if (!adjusting) return;
    let live = true;
    void listModelPresets()
      .then((list) => {
        if (live) presets = list;
      })
      .catch(() => {
        if (live) presets = [];
      });
    return () => {
      live = false;
    };
  });

  $effect(() => {
    const draftAccess = adjust?.draft?.access ?? [];
    const known = new Set([...accesses, ...extraAccesses].map((a) => a.id));
    const missing = draftAccess.filter((id) => !known.has(id));
    if (missing.length === 0) return;
    let live = true;
    void Promise.all(missing.map((id) => describeAccess(id))).then((views) => {
      if (live) extraAccesses = [...extraAccesses, ...views];
    });
    return () => {
      live = false;
    };
  });

  const adjustGroups = $derived.by(() => {
    if (!adjust?.draft || !adjust.base) return [];
    const byId = new Map([...accesses, ...extraAccesses].map((a) => [a.id, a]));
    const ids = listedAccesses(adjust.base.access, adjust.draft.access);
    return groupAccesses(ids.map((id) => byId.get(id) ?? accessView(id, null)));
  });

  function marked(field: MarkedField): boolean {
    if (adjusting && adjust) return isOverridden(adjust.base, field) || adjust.changed.has(field);
    return isOverridden(resolved, field);
  }

  async function loadCandidates(): Promise<GrantCandidate[]> {
    const [tools, targets] = await Promise.all([
      listTools().catch(() => []),
      listMcpExposable().catch(() => []),
    ]);
    return grantCandidates(tools, targets, adjust?.draft?.access ?? [], definitionId);
  }

  async function loadConnectionOffer(): Promise<ConnectionOffer> {
    const [listed, tools] = await Promise.all([
      listConnections().catch(() => [] as Connection[]),
      listTools().catch(() => []),
    ]);
    connections = listed;
    return {
      connections: sortConnections(listed.filter((c) => c.tools.length > 0)),
      descriptions: new Map(tools.map((tool) => [tool.name, tool.description])),
    };
  }

  const remote = $derived(
    remoteAccesses([...accesses, ...extraAccesses].map((a) => a.id), connections),
  );

  function accessLabel(accessId: string): string {
    const known = accesses.find((a) => a.id === accessId);
    return known ? (known.permits ?? known.label) : readableName(accessId);
  }

  /** The label of each tab, named in full so the catalogue checks see them. */
  const TAB_KEYS = {
    sheet: "agent_sheet.tab.sheet",
    activity: "agent_sheet.tab.activity",
    memory: "agent_sheet.tab.memory",
    technical: "agent_sheet.tab.technical",
  } as const;
</script>

<!-- Three tabs: what the agent is, what it did, what it keeps. While the
     sheet is being adjusted, only the first one holds something to change. -->
<div
  class="flex max-w-5xl flex-col gap-5"
  data-testid="agent-sheet"
  data-agent={definitionId}
  aria-label={$t("agent_sheet.label")}
>
  <TabBar
    items={tabs}
    activeTab={tab}
    ontabchange={(key) => (asked = key as SheetTab)}
    testidPrefix="agent-sheet"
    variant="underline"
  />

  {#if tab === "sheet"}
    <div class="grid grid-cols-1 gap-5 xl:grid-cols-2">
      {#if adjusting && adjust?.draft && adjust.base}
        <AdjustMission draft={adjust.draft} modified={marked("mission") || marked("examples")} />
        <AdjustModel draft={adjust.draft} options={modelOptions($llmBackends)} {presets} modified={marked("model")} />
        <AdjustAccess
          draft={adjust.draft}
          groups={adjustGroups}
          inForce={adjust.base.access}
          modified={marked("access")}
          {loadCandidates}
          agentName={resolved?.name ?? agent?.name ?? definitionId}
          {loadConnectionOffer}
          {remote}
        />
        <AdjustSources draft={adjust.draft} sources={registry} modified={marked("sources")} />
      {:else}
        <div class="min-w-0 space-y-5">
          <SheetMission view={mission} modified={marked("mission") || marked("examples")} />
          <SheetModel view={model} fallback={notice} modified={marked("model")} />
        </div>
        <div class="min-w-0 space-y-5">
          <SheetAccess {groups} loading={accessLoading} modified={marked("access")} {remote} />
          <SheetSources rows={sources} modified={marked("sources")} />
        </div>
      {/if}
    </div>
  {:else if tab === "activity"}
    <div class="grid grid-cols-1 gap-5 xl:grid-cols-2">
      <SheetActivity
        {runs}
        {runsLoading}
        {runsError}
        changes={changeRows}
        triggersFor={agent?.name ?? null}
        logsFor={runtimeId}
        {accessLabel}
        {onOpenLogs}
      />
      <SheetAlways {definitionId} />
    </div>
  {:else if tab === "memory"}
    <SheetMemory namespace={memoryNamespace} />
  {:else if agent}
    <SheetTechnical {agent} definition={resolved} />
  {/if}
</div>
