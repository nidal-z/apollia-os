<script lang="ts">
  /**
   * AgentSheet - the sheet of an agent, read without code.
   *
   * Drawn from the agent the runtime resolves (package definition, then the
   * user's overrides), in this order: mission and examples; model and where
   * it runs, with the fallback notice; accesses grouped by action family;
   * sources, the profile among them; activity and changes; memory. The
   * built-in Apollia agent has the same sheet as any other.
   *
   * When the runtime holds no definition for an agent, the sheet falls back
   * on what the agent's code declares (description, examples, tools), so an
   * agent installed before definitions existed still reads.
   */
  import { t } from "svelte-i18n";
  import { llmBackends } from "$lib/stores/llm";
  import { describeTool, toolKindTag } from "$lib/ipc/tools";
  import { listTasks } from "$lib/ipc/tasks";
  import {
    getAgentDefinition,
    getAgentModelFallback,
    listAgentChanges,
    type AgentChangeEntry,
    type AgentModelFallback,
    type ResolvedAgent,
  } from "$lib/ipc/agentSheet";
  import { createIdentityGuard } from "$lib/utils/identityGuard";
  import type { AgentListItem, TaskSummary } from "$lib/types";
  import SheetMission from "./SheetMission.svelte";
  import SheetModel from "./SheetModel.svelte";
  import SheetAccess from "./SheetAccess.svelte";
  import SheetSources from "./SheetSources.svelte";
  import SheetActivity from "./SheetActivity.svelte";
  import SheetMemory from "./SheetMemory.svelte";
  import SheetTechnical from "./SheetTechnical.svelte";
  import {
    accessView,
    changeViews,
    fallbackNotice,
    groupAccesses,
    missionView,
    modelView,
    readableName,
    sourceViews,
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
  }

  let { definitionId, agent, memoryNamespace, onOpenLogs }: Props = $props();

  const RECENT_RUNS = 10;

  let resolved = $state<ResolvedAgent | null>(null);
  let fallback = $state<AgentModelFallback | null>(null);
  let changes = $state<AgentChangeEntry[]>([]);
  let accesses = $state<AccessView[]>([]);
  let accessLoading = $state(true);
  let runs = $state<TaskSummary[]>([]);
  let runsLoading = $state(false);
  let runsError = $state<string | null>(null);

  const runtimeId = $derived(agent?.id ?? null);
  const guard = createIdentityGuard(() => definitionId);
  const runsGuard = createIdentityGuard(() => runtimeId);

  $effect(() => {
    void loadDefinition(definitionId, agent);
  });

  $effect(() => {
    void loadRuns(runtimeId);
  });

  async function loadDefinition(id: string, declared: AgentListItem | null): Promise<void> {
    const ticket = guard.begin();
    accessLoading = true;
    const [definition, lastFallback, history] = await Promise.all([
      getAgentDefinition(id).catch(() => null),
      getAgentModelFallback(id).catch(() => null),
      listAgentChanges(id).catch(() => [] as AgentChangeEntry[]),
    ]);
    if (!ticket.current) return;
    resolved = definition;
    fallback = lastFallback;
    changes = history;
    const ids = definition
      ? definition.access
      : [...(declared?.tools_required ?? []), ...(declared?.tools_optional ?? [])];
    const views = await Promise.all(ids.map((accessId) => describeAccess(accessId)));
    if (!ticket.current) return;
    accesses = views;
    accessLoading = false;
  }

  async function describeAccess(id: string): Promise<AccessView> {
    if (id.startsWith("agent:") || id.startsWith("workflow:") || id.startsWith("mcp:")) {
      return accessView(id, null);
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
  const sources = $derived(sourceViews(resolved?.sources ?? []));
  const changeRows = $derived(changeViews(changes));

  function accessLabel(accessId: string): string {
    const known = accesses.find((a) => a.id === accessId);
    return known ? (known.permits ?? known.label) : readableName(accessId);
  }
</script>

<div class="max-w-3xl space-y-6" data-testid="agent-sheet" data-agent={definitionId} aria-label={$t("agent_sheet.label")}>
  <SheetMission view={mission} />
  <SheetModel view={model} fallback={notice} />
  <SheetAccess {groups} loading={accessLoading} />
  <SheetSources {sources} />
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
  <SheetMemory namespace={memoryNamespace} />
  {#if agent}
    <SheetTechnical {agent} definition={resolved} />
  {/if}
</div>
