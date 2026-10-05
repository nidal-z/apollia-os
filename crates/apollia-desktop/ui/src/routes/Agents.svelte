<script lang="ts">
  /**
   * Agents route - thin orchestrator.
   *
   * The screen opens on its list, the agents and the workflows side by side
   * (AgentsOverview). A row opens a sheet in place of the list: the sheet of
   * an agent (AgentDetailPane) or of a workflow (WorkflowSheet), each with a
   * way back. "Create an agent" opens, in the same place, the conversation
   * that drafts one (CreateAgent). All rendering lives in `components/agents/*` and
   * `components/workflows/*`; all IPC goes through `$lib/ipc/*`.
   */
  import { t } from "svelte-i18n";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { agents } from "$lib/stores/agents";
  import { connectionStatus } from "$lib/stores/sse";
  import { navigateTo } from "$lib/stores/navigation";
  import { declareGuideSubscreen } from "$lib/stores/guideContext";
  import { pendingChatSessionId } from "$lib/stores/chat";
  import { PACKAGE_INSTALLED_EVENT, openPackagePreview } from "$lib/stores/packagePreview";
  import { pendingSheet, takePendingSheet } from "$lib/stores/agentOpen";
  import { ErrorBanner } from "$lib/components/operator";
  import { reportError } from "$lib/errors/reportError";
  import { createChatSession } from "$lib/ipc/agents";
  import {
    listWorkflowRuns,
    listWorkflows,
    type WorkflowListItem,
    type WorkflowRun,
  } from "$lib/ipc/workflows";
  import { startAgentChat } from "$lib/agents/startAgentChat";
  import { takeRequestedAgent } from "$lib/stores/agentFocus";
  import { APOLLIA_AGENT_ID } from "../components/agents/sheet/agentSheet";
  import MacSandboxBanner from "../components/common/MacSandboxBanner.svelte";
  import AgentLogs from "../components/agents/AgentLogs.svelte";
  import InstallPackageDialog from "../components/agents/InstallPackageDialog.svelte";
  import AgentsOverview from "../components/agents/AgentsOverview.svelte";
  import AgentDetailPane from "../components/agents/AgentDetailPane.svelte";
  import WorkflowSheet from "../components/workflows/WorkflowSheet.svelte";
  import CreateAgent from "../components/agents/create/CreateAgent.svelte";
  import { filterWorkflows, splitWorkflowId } from "../components/workflows/workflowRuns";
  import { createAgentActions } from "../components/agents/useAgentActions.svelte";
  import type {
    AgentListItem,
    ChatSessionSummary,
    CreateSessionRequest,
  } from "$lib/types";

  /** What the screen shows: its list, or one sheet in its place. */
  type View =
    | { kind: "list" }
    | { kind: "apollia" }
    | { kind: "create" }
    | { kind: "agent"; name: string }
    | { kind: "workflow"; id: string; name: string; launch: boolean };

  // The update failure lives on the controller (`actions.updateError`): the
  // button that triggers it sits in the detail header, two components down,
  // while the banner belongs here.
  const actions = createAgentActions();

  // ── Selection + local UI state ───────────────────────────────────────
  let query = $state("");
  let view = $state<View>({ kind: "list" });
  let workflows = $state<WorkflowListItem[]>([]);
  let workflowsLoading = $state(true);
  let lastRuns = $state<Record<string, WorkflowRun | null>>({});

  let uninstallConfirmName = $state<string | null>(null);
  let installPackageOpen = $state(false);
  let logsAgentId = $state<string | null>(null);
  let logsOpen = $state(false);

  // The installed workflows, with the most recent run of each. A workflow
  // whose runs cannot be read is listed without one.
  async function refreshWorkflows(): Promise<void> {
    try {
      const listed = await listWorkflows();
      workflows = listed;
      const runs = await Promise.all(
        listed.map((workflow) =>
          listWorkflowRuns(workflow.workflowId, 1)
            .then((found) => found[0] ?? null)
            .catch(() => null),
        ),
      );
      lastRuns = Object.fromEntries(listed.map((workflow, i) => [workflow.workflowId, runs[i]]));
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      workflowsLoading = false;
    }
  }

  // Read when the list is shown: at mount, and on the way back from a sheet,
  // where a run may have been launched or stopped.
  $effect(() => {
    if (view.kind === "list") void refreshWorkflows();
  });

  // The command palette reaches this route by event rather than by import, so
  // it can name an action without depending on the screen that performs it. An
  // emission with no listener here is an entry that looks alive and does
  // nothing, which is why it is mounted with the route.
  $effect(() => {
    const openInstall = () => {
      installPackageOpen = true;
    };
    window.addEventListener("apollia:agents:create", openInstall);
    return () => {
      window.removeEventListener("apollia:agents:create", openInstall);
    };
  });

  $effect(() => {
    const refresh = () => {
      void refreshWorkflows();
    };
    window.addEventListener(PACKAGE_INSTALLED_EVENT, refresh);
    return () => {
      window.removeEventListener(PACKAGE_INSTALLED_EVENT, refresh);
    };
  });

  // Agents that take a message; the others stay reachable from their package.
  const allAgents = $derived($agents.filter((a) => a.agent_type !== "worker"));

  const filteredAgents = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (q.length === 0) return allAgents;
    return allAgents.filter(
      (a) =>
        a.name.toLowerCase().includes(q) ||
        (a.description ?? "").toLowerCase().includes(q),
    );
  });

  const filteredWorkflows = $derived(filterWorkflows(workflows, query));

  const selectedName = $derived(view.kind === "agent" ? view.name : null);
  const selected = $derived(
    selectedName === null ? null : ($agents.find((a) => a.name === selectedName) ?? null),
  );

  // The open agent is the sub-screen the guide explains, and it names it.
  $effect(() => {
    if (!selected) return;
    return declareGuideSubscreen("agents.fiche", {
      id: selected.id ?? selected.name,
      name: selected.name,
    });
  });

  // The sheet of an agent that was listed and no longer is (uninstalled)
  // gives way to the list. An agent asked for before the list is read is
  // waited for.
  let listedName: string | null = null;
  $effect(() => {
    if (view.kind !== "agent") {
      listedName = null;
    } else if (selected !== null) {
      listedName = selected.name;
    } else if (listedName === view.name) {
      backToList();
    }
  });

  // ── Selection handlers ───────────────────────────────────────────────
  function selectAgent(name: string): void {
    view = { kind: "agent", name };
    if (uninstallConfirmName !== name) uninstallConfirmName = null;
    // A rejected module belongs to the agent it was picked for. Moving away
    // from that agent leaves a banner naming nothing on screen.
    actions.clearUpdateError();
  }

  /**
   * Arm the uninstall confirm for an agent, opening its sheet on the way.
   *
   * The context menu of a row calls this from the list. Opening the sheet
   * first means the confirm always appears over the agent it names, rather
   * than deleting from a menu with nothing on screen.
   */
  function requestUninstallAgent(agent: AgentListItem): void {
    selectAgent(agent.name);
    uninstallConfirmName = agent.name;
  }

  function selectApolliaChat(): void {
    view = { kind: "apollia" };
    uninstallConfirmName = null;
    actions.clearUpdateError();
  }

  function openCreation(): void {
    view = { kind: "create" };
    uninstallConfirmName = null;
    actions.clearUpdateError();
  }

  function openWorkflow(id: string, name: string, launch: boolean): void {
    view = { kind: "workflow", id, name, launch };
  }

  function backToList(): void {
    view = { kind: "list" };
    uninstallConfirmName = null;
    actions.clearUpdateError();
  }

  // Another screen asked for a sheet (the content of a package in the
  // Library, for one): the sheet of an agent opens once the agent is listed,
  // the sheet of a workflow at once.
  $effect(() => {
    const request = $pendingSheet;
    if (request === null) return;
    if (request.kind === "agent") {
      if (!$agents.some((a) => a.name === request.id)) return;
      query = "";
      selectAgent(request.id);
    } else {
      const known = workflows.find((w) => w.workflowId === request.id);
      openWorkflow(request.id, known?.name ?? splitWorkflowId(request.id).name, false);
    }
    takePendingSheet();
  });

  // Another screen may ask for an agent's sheet before navigating here.
  $effect(() => {
    const asked = takeRequestedAgent();
    if (asked === null) return;
    if (asked === APOLLIA_AGENT_ID) selectApolliaChat();
    else selectAgent(asked);
  });

  // ── Actions ──────────────────────────────────────────────────────────
  // A package file opens its preview, mounted above every screen; the package
  // is installed from there, and the list refreshes on its installed event.
  async function pickPackageFile(): Promise<void> {
    try {
      const path = await openDialog({
        filters: [{ name: "Apollia", extensions: ["apo"] }],
        multiple: false,
      });
      if (typeof path === "string") openPackagePreview(path);
    } catch (err) {
      reportError(err, { surface: "toast" });
    }
  }

  // Routing on a failed creation dropped the operator onto an empty
  // conversation with no agent behind it: the id the chat route waits for was
  // never produced, and nothing said so.
  async function startChatWithAgent(agentName: string): Promise<void> {
    const request: CreateSessionRequest = { mode: "agent", agent_name: agentName };
    await startAgentChat({
      createSession: (): Promise<ChatSessionSummary> => createChatSession(request),
      rememberSession: (sessionId) => pendingChatSessionId.set(sessionId),
      report: (err) => {
        reportError(err, { surface: "toast", testid: "agent-chat-start-error" });
      },
      navigate: () => navigateTo("chat"),
    });
  }

  function openLogs(agentId: string): void {
    logsAgentId = agentId;
    logsOpen = true;
  }
</script>

<div class="flex h-full min-h-0 w-full flex-col" data-testid="agents-page" data-view={view.kind}>
  <div class="px-8 pt-3">
    <MacSandboxBanner />
    {#if actions.updateError}
      <ErrorBanner class="mt-3" data-testid="agent-update-error">
        <p class="font-medium">{actions.updateError.friendly_message}</p>
        <p class="text-caption opacity-90">{actions.updateError.suggested_action}</p>
        {#if actions.updateError.detail}
          <details class="mt-2">
            <summary
              class="cursor-pointer text-caption opacity-70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
              data-testid="agent-update-error-details"
            >
              {$t("errors.show_details")}
            </summary>
            <p class="mt-1.5 break-all font-mono text-code-sm opacity-80">
              {actions.updateError.detail}
            </p>
          </details>
        {/if}
      </ErrorBanner>
    {/if}
  </div>

  <div class="min-h-0 flex-1 overflow-y-auto">
    {#if view.kind === "list"}
      <AgentsOverview
        agents={filteredAgents}
        workflows={filteredWorkflows}
        {lastRuns}
        bind:query
        connecting={$connectionStatus === "connecting"}
        {workflowsLoading}
        onSelectAgent={selectAgent}
        onSelectApollia={selectApolliaChat}
        onOpenWorkflow={(workflow, launch) => openWorkflow(workflow.workflowId, workflow.name, launch)}
        onInstallPackageFile={pickPackageFile}
        onCreateAgent={openCreation}
        onOpenLogs={openLogs}
        onRequestUninstallAgent={requestUninstallAgent}
        onStartChat={startChatWithAgent}
      />
    {:else if view.kind === "workflow"}
      {#key view.id}
        <WorkflowSheet
          workflowId={view.id}
          name={view.name}
          launchOnOpen={view.launch}
          onback={backToList}
        />
      {/key}
    {:else if view.kind === "create"}
      <CreateAgent onback={backToList} onStartChat={startChatWithAgent} onOpenLogs={openLogs} />
    {:else if view.kind === "apollia"}
      <AgentDetailPane
        selected={null}
        agentActions={actions}
        onStartChat={startChatWithAgent}
        onOpenLogs={openLogs}
        confirmingUninstall={false}
        onArmUninstall={() => undefined}
        onDisarmUninstall={() => undefined}
        onback={backToList}
      />
    {:else if selected}
      <AgentDetailPane
        {selected}
        agentActions={actions}
        onStartChat={startChatWithAgent}
        onOpenLogs={openLogs}
        confirmingUninstall={uninstallConfirmName === selected.name}
        onArmUninstall={() => (uninstallConfirmName = selected?.name ?? null)}
        onDisarmUninstall={() => (uninstallConfirmName = null)}
        onback={backToList}
      />
    {:else}
      <div class="px-8 py-8 text-body-xs text-muted-foreground">{$t("common.loading")}</div>
    {/if}
  </div>
</div>

{#if logsAgentId}
  <AgentLogs agentId={logsAgentId} open={logsOpen} onclose={() => (logsOpen = false)} />
{/if}

<InstallPackageDialog
  open={installPackageOpen}
  onclose={() => (installPackageOpen = false)}
  oninstalled={() => void refreshWorkflows()}
/>
