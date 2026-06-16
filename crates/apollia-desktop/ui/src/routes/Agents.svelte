<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { get } from "svelte/store";
  import { t } from "svelte-i18n";
  import type { AgentListItem, MemoryEntry } from "$lib/types";
  import { agents } from "$lib/stores/agents";
  import { connectionStatus, triggers } from "$lib/stores/sse";
  import { navigateTo } from "$lib/stores/navigation";
  import { pendingChatSessionId } from "$lib/stores/chat";
  import { tourOpenAgentDetail } from "$lib/stores/tour";
  import {
    AlertTriangle,
    Bot,
    Clock,
    Cpu,
    Download,
    ExternalLink,
    FolderOpen,
    Package,
    Play,
    Plus,
    Search,
    Sparkles,
    Square,
    MessageSquare,
    Terminal,
    Trash2,
    Users,
    Zap,
  } from "lucide-svelte";
  import AgentLogs from "../components/agents/AgentLogs.svelte";
  import AgentActivity from "../components/agents/AgentActivity.svelte";
  // AgentDetail.svelte (Sheet) was the previous "Configurer" target - replaced
  // by inline tabs (Aperçu / Outils / Mémoire / Activité / Paramètres) in this
  // route. The file is kept on disk for archival but no longer imported here.
  import AgentTriggers from "../components/agents/AgentTriggers.svelte";
  import AgentLlmInfo from "../components/agents/AgentLlmInfo.svelte";
  import AgentMessagesPanel from "../components/agents/AgentMessagesPanel.svelte";
  import ApolliaChatConfigPanel from "../components/agents/ApolliaChatConfigPanel.svelte";
  import InstallPackageDialog from "../components/agents/InstallPackageDialog.svelte";
  import MacSandboxBanner from "../components/common/MacSandboxBanner.svelte";
  import {
    DetailHeader,
    SidebarHeader,
    StatusDot,
    Card,
    EmptyState,
  } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Toggle } from "$lib/components/ui/toggle";
  import { TabBar } from "$lib/components/ui/tabs";
  import { uiMode } from "$lib/stores/mode";
  import type {
    AgentPackageListItem,
    AgentPackageDetailView,
    ChatSessionSummary,
    CreateSessionRequest,
    InstallPackageResponse,
  } from "$lib/types";
  import {
    agentPackages,
    refreshPackages,
    uninstallPackage,
    getPackageDetail,
    startPackage,
    stopPackage,
    packageRuntimeState,
  } from "$lib/stores/agentPackages";
  import { addToast } from "$lib/components/ui/toast/store";
  import { Spinner } from "$lib/components/ui/progress";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { Input } from "$lib/components/ui/input";

  // ── Existing state (preserved from previous implementation) ──────────
  let installingAgent = $state(false);
  let installError = $state<string | null>(null);
  let logsAgentId = $state<string | null>(null);
  let logsOpen = $state(false);

  let installPackageOpen = $state(false);

  // ── Detail tabs (sidebar + tabs pattern, mirror Projets) ─────────────
  type AgentTab = "overview" | "tools" | "memory" | "activity" | "settings";
  type PackageTab = "overview" | "agents" | "triggers";
  let agentDetailTab = $state<AgentTab>("overview");
  let packageDetailTab = $state<PackageTab>("overview");

  // Inline agent action state (formerly inside AgentDetail.svelte) ──────
  let stoppingAgent = $state(false);
  let confirmStopVisible = $state(false);
  let stopError = $state<string | null>(null);
  let startLoading = $state(false);
  let toggleLoading = $state(false);
  let actionError = $state<string | null>(null);

  async function handleStopSelected(a: AgentListItem): Promise<void> {
    if (!a.id) return;
    confirmStopVisible = false;
    stoppingAgent = true;
    stopError = null;
    try {
      await invoke("stop_agent", { agentId: a.id });
    } catch (err: unknown) {
      stopError = err instanceof Error ? err.message : String(err);
    } finally {
      stoppingAgent = false;
    }
  }

  async function handleStartSelected(a: AgentListItem): Promise<void> {
    if (!a.install_path) return;
    startLoading = true;
    actionError = null;
    try {
      await invoke("start_agent", { path: a.install_path });
    } catch (err: unknown) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      startLoading = false;
    }
  }

  async function handleToggleEnabled(a: AgentListItem): Promise<void> {
    toggleLoading = true;
    actionError = null;
    try {
      if (a.enabled) {
        await invoke("disable_agent", { name: a.name });
      } else {
        await invoke("enable_agent", { name: a.name });
      }
    } catch (err: unknown) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      toggleLoading = false;
    }
  }

  function executionModeLabel(mode: string | null): string {
    switch (mode) {
      case "direct":
        return $t("agent_detail.execution_mode_direct");
      case "orchestrated":
        return $t("agent_detail.execution_mode_orchestrated");
      default:
        return $t("agent_detail.execution_mode_auto");
    }
  }

  // ── Memory tab - load entries from the agent's declared namespace ─────
  // Important: `memory_namespace` comes from the Python manifest, not from
  // `agent.name`. Multiple agents in a package often share a single namespace
  // (e.g. all agents in the `veille-ia` package declare
  // `memory_namespace = "veille-ia"`), so we must NEVER fall back to `name`
  // when fetching entries - that would silently miss the package's memory.
  let memoryEntries = $state<MemoryEntry[]>([]);
  let memoryLoading = $state(false);
  let memoryError = $state<string | null>(null);

  async function loadAgentMemory(namespace: string): Promise<void> {
    memoryLoading = true;
    memoryError = null;
    try {
      memoryEntries = await invoke<MemoryEntry[]>("list_memory_entries", {
        namespace,
      });
    } catch (err: unknown) {
      memoryError = err instanceof Error ? err.message : String(err);
      memoryEntries = [];
    } finally {
      memoryLoading = false;
    }
  }

  $effect(() => {
    if (agentDetailTab !== "memory") return;
    const ns = selected?.memory_namespace;
    if (!ns) {
      memoryEntries = [];
      memoryLoading = false;
      memoryError = null;
      return;
    }
    void loadAgentMemory(ns);
  });

  // ── New V3 state ─────────────────────────────────────────────────────
  let query = $state("");
  let selectedName = $state<string | null>(null);
  /** Nom du package sélectionné, ou null. Mutuellement exclusif avec selectedName. */
  let selectedPackageName = $state<string | null>(null);
  /** Détail du package sélectionné, chargé à la volée. */
  let pkgDetail = $state<AgentPackageDetailView | null>(null);
  /** Pinned synthetic system agent - when true, the right column shows the
   * Apollia Chat config panel instead of an `AgentListItem` detail. */
  let apolliaChatSelected = $state(false);
  /** Action start/stop en cours, par identifiant ("agent:NAME" ou "pkg:NAME"). */
  let busyKeys = $state<Record<string, boolean>>({});
  /** Confirmation désinstallation inline. */
  let confirmUninstallPkg = $state<string | null>(null);

  $effect(() => {
    refreshPackages();
  });

  // Assistants only on this view (workers stay accessible via the package
  // section / dedicated configuration screens).
  const allAssistants = $derived(
    $agents.filter((a) => a.agent_type !== "worker"),
  );

  const filteredAssistants = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (q.length === 0) return allAssistants;
    return allAssistants.filter(
      (a) =>
        a.name.toLowerCase().includes(q) ||
        (a.description ?? "").toLowerCase().includes(q),
    );
  });

  // Auto-select the first assistant when the list refreshes (sauf si un package ou
  // l'agent système Apollia Chat est déjà sélectionné).
  $effect(() => {
    if (selectedPackageName !== null || apolliaChatSelected) return;
    if (
      filteredAssistants.length > 0 &&
      (selectedName === null ||
        !filteredAssistants.some((a) => a.name === selectedName))
    ) {
      selectedName = filteredAssistants[0].name;
    }
    if (filteredAssistants.length === 0) {
      selectedName = null;
    }
  });

  // Reset agent detail tab when the agent selection changes.
  $effect(() => {
    void selectedName;
    agentDetailTab = "overview";
    confirmStopVisible = false;
    stopError = null;
    actionError = null;
  });

  // Reset package detail tab when the package selection changes.
  $effect(() => {
    void selectedPackageName;
    packageDetailTab = "overview";
  });

  // Filtered packages (same query as assistants).
  const filteredPackages = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (q.length === 0) return $agentPackages;
    return $agentPackages.filter(
      (p) =>
        p.name.toLowerCase().includes(q) ||
        (p.description ?? "").toLowerCase().includes(q),
    );
  });

  const selectedPackage = $derived(
    selectedPackageName === null
      ? null
      : ($agentPackages.find((p) => p.name === selectedPackageName) ?? null),
  );

  const selectedPackageState = $derived(
    selectedPackage
      ? packageRuntimeState(selectedPackage, $agents, $triggers)
      : null,
  );

  // If the selected package gets uninstalled, clear selection.
  $effect(() => {
    if (selectedPackageName !== null && selectedPackage === null) {
      selectedPackageName = null;
      pkgDetail = null;
    }
  });

  const selected = $derived(
    selectedName === null
      ? null
      : ($agents.find((a) => a.name === selectedName) ?? null),
  );

  // ── Helpers ──────────────────────────────────────────────────────────
  // Maps the Python class name (decision D2) onto a short user-facing label.
  function agentClassLabel(a: AgentListItem): string | null {
    switch (a.agent_class) {
      case "ReActAgent":
        return "Direct";
      case "ConversationalAgent":
        return "Conversational";
      case "OrchestratedAgent":
        return "Orchestrated";
      case "WorkerAgent":
        return "Worker";
      default:
        return a.agent_class ?? null;
    }
  }


  function isActive(a: AgentListItem): boolean {
    return a.runtime_status === "active" || a.runtime_status === "degraded";
  }

  function isIdle(a: AgentListItem): boolean {
    return a.runtime_status === "stopped" || a.runtime_status === null;
  }

  function statusLabel(a: AgentListItem): string {
    switch (a.runtime_status) {
      case "active":
        return $t("agents.status_active");
      case "degraded":
        return $t("agents.status_degraded");
      case "initializing":
        return $t("agents.status_initializing");
      case "stopping":
        return $t("agents.status_stopping");
      case "stopped":
        return $t("agents.status_stopped");
      default:
        return $t("agents.status_not_loaded");
    }
  }

  function statusTone(
    a: AgentListItem,
  ): "success" | "warning" | "neutral" {
    if (a.runtime_status === "active") return "success";
    if (a.runtime_status === "degraded") return "warning";
    return "neutral";
  }

  function statusColor(a: AgentListItem): string {
    if (a.runtime_status === "active") return "hsl(var(--success))";
    if (a.runtime_status === "degraded") return "hsl(var(--warning))";
    return "hsl(var(--muted-foreground))";
  }

  // ── Actions (preserved from previous implementation) ─────────────────
  async function pickAndInstallAgent() {
    installError = null;
    try {
      const path = await openDialog({
        filters: [{ name: "Python Agent", extensions: ["py"] }],
        multiple: false,
      });
      if (!path) return;
      installingAgent = true;
      await invoke("install_agent", { path });
    } catch (err: unknown) {
      installError = err instanceof Error ? err.message : String(err);
    } finally {
      installingAgent = false;
    }
  }

  function openLogs(agentId: string) {
    logsAgentId = agentId;
    logsOpen = true;
  }
  function closeLogs() {
    logsOpen = false;
  }
  /**
   * Select an agent and jump to its settings tab (replaces the old
   * `AgentDetail` Sheet - the tour entry point still calls this).
   */
  function openDetail(agent: AgentListItem) {
    selectedName = agent.name;
    apolliaChatSelected = false;
    selectedPackageName = null;
    agentDetailTab = "settings";
  }
  function handleTaskClick(_taskId: string) {
    navigateTo("tasks");
  }
  function handleMemoryLink() {
    navigateTo("memory");
  }

  function formatMemoryRelative(iso: string): string {
    const diffMs = Date.now() - new Date(iso).getTime();
    if (diffMs < 0) return $t("agents.memory_relative_future");
    const s = Math.floor(diffMs / 1000);
    if (s < 60) return `${s}s`;
    const m = Math.floor(s / 60);
    if (m < 60) return `${m}min`;
    const h = Math.floor(m / 60);
    if (h < 24) return `${h}h`;
    const d = Math.floor(h / 24);
    if (d < 7) return `${d}j`;
    return `${Math.floor(d / 7)}sem`;
  }

  async function startChatWithAgent(agentName: string): Promise<void> {
    try {
      const request: CreateSessionRequest = {
        mode: "agent",
        agent_name: agentName,
      };
      const session = await invoke<ChatSessionSummary>(
        "create_chat_session",
        { request },
      );
      pendingChatSessionId.set(session.id);
      navigateTo("chat");
    } catch {
      navigateTo("chat");
    }
  }

  async function selectPackage(pkg: AgentPackageListItem) {
    selectedPackageName = pkg.name;
    selectedName = null;
    apolliaChatSelected = false;
    pkgDetail = null;
    confirmUninstallPkg = null;
    try {
      pkgDetail = await getPackageDetail(pkg.name);
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }

  async function handleUninstallPkg(name: string) {
    try {
      await uninstallPackage(name);
      if (selectedPackageName === name) {
        selectedPackageName = null;
        pkgDetail = null;
      }
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      confirmUninstallPkg = null;
    }
  }

  async function handlePkgInstalled(_result: InstallPackageResponse) {
    await refreshPackages();
  }

  // ── Inline start/stop actions ────────────────────────────────────────
  function setBusy(key: string, value: boolean) {
    busyKeys = { ...busyKeys, [key]: value };
  }

  async function toggleAgentRuntime(a: AgentListItem) {
    const key = `agent:${a.name}`;
    if (busyKeys[key]) return;
    setBusy(key, true);
    try {
      if (isActive(a)) {
        if (a.id) await invoke("stop_agent", { agentId: a.id });
      } else {
        if (a.install_path) {
          await invoke("start_agent", { path: a.install_path });
        } else {
          addToast(
            $t("agents.start_missing_path", { values: { name: a.name } }),
            "error",
          );
        }
      }
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      setBusy(key, false);
    }
  }

  async function togglePackageRuntime(pkg: AgentPackageListItem) {
    const key = `pkg:${pkg.name}`;
    if (busyKeys[key]) return;
    const state = packageRuntimeState(pkg, $agents, $triggers);
    setBusy(key, true);
    try {
      const result =
        state.status === "running" || state.status === "partial"
          ? await stopPackage(pkg, $agents, $triggers)
          : await startPackage(pkg, $agents, $triggers);
      if (result.errors.length > 0) {
        addToast(
          $t("agents.package_errors", {
            values: {
              name: pkg.name,
              count: result.errors.length,
              first: result.errors[0],
            },
          }),
          "error",
        );
      }
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      setBusy(key, false);
    }
  }

  function packageStatusTone(
    s: ReturnType<typeof packageRuntimeState>,
  ): "success" | "warning" | "neutral" {
    if (s.status === "running") return "success";
    if (s.status === "partial") return "warning";
    return "neutral";
  }

  function packageStatusLabel(
    s: ReturnType<typeof packageRuntimeState>,
  ): string {
    if (s.status === "running") return $t("agents.status_active");
    if (s.status === "partial") return $t("agents.pkg_status_partial");
    return $t("agents.status_stopped");
  }

  function packageStatusColor(
    s: ReturnType<typeof packageRuntimeState>,
  ): string {
    if (s.status === "running") return "hsl(var(--success))";
    if (s.status === "partial") return "hsl(var(--warning))";
    return "hsl(var(--muted-foreground))";
  }

  // Open the agent detail panel when the tour requests it programmatically.
  $effect(() => {
    return tourOpenAgentDetail.subscribe((agentName) => {
      if (agentName === null) return;
      const found = get(agents).find((a) => a.name === agentName);
      if (found !== undefined) {
        selectedName = found.name;
        openDetail(found);
      }
    });
  });
</script>

<div
  class="flex h-full min-h-0 w-full flex-col"
  data-testid="agents-page"
>
  <div class="px-8 pt-3">
    <MacSandboxBanner />
    {#if installError}
      <div
        class="mt-3 rounded-lg border border-destructive/20 bg-destructive/5 px-3 py-2 text-xs text-destructive"
      >
        {installError}
      </div>
    {/if}
  </div>

  <!-- ── Split layout: list (320px) + detail ─────────────────────────── -->
  <div class="flex min-h-0 flex-1">
    <!-- LEFT - list -->
    <aside
      class="flex w-[320px] shrink-0 flex-col border-r border-border/60"
      data-testid="agents-list"
    >
      <!-- Page-level actions live in the sidebar header (split screen, identity is the breadcrumb) -->
      <SidebarHeader title={$t("agents.page_title")}>
        {#snippet actions()}
          <Button
            variant="outline"
            size="icon-sm"
            onclick={() => (installPackageOpen = true)}
            title={$t("agents.install_package_button")}
            aria-label={$t("agents.install_package_button")}
          >
            <Package size={13} />
          </Button>
          <Button
            variant="primary-solid"
            size="icon-sm"
            onclick={pickAndInstallAgent}
            disabled={installingAgent}
            title={$t("agents.new_assistant")}
            aria-label={$t("agents.new_assistant")}
          >
            {#if installingAgent}<Spinner size={13} />{:else}<Download size={13} />{/if}
          </Button>
        {/snippet}
        {#snippet search()}
          <div
            class="flex items-center gap-[7px] rounded-md border border-border bg-surface-1 px-2.5 py-[7px]"
          >
            <Search size={11} class="text-muted-foreground" />
            <Input
              type="text"
              unstyled
              bind:value={query}
              placeholder={$t("agents.filter_placeholder")}
              class="flex-1 text-[11.5px] text-foreground"
              data-testid="agents-search"
             />
          </div>
        {/snippet}
      </SidebarHeader>

      <div class="flex-1 overflow-y-auto px-2.5 pb-2">
        <!-- Section header: assistants -->
        <div
          class="section-meta mb-1.5 mt-1 px-2 text-[10px] tracking-[1.4px] /80"
        >
          {$t("agents.section_my_assistants")} · {filteredAssistants.length}
        </div>
        <!-- Pinned system agent: Apollia Chat -->
        <Button variant="ghost" size="auto"
          type="button"
          onclick={() => {
            apolliaChatSelected = true;
            selectedName = null;
            selectedPackageName = null;
          }}
          class="mb-1 flex w-full items-center gap-2.5 rounded-lg px-2.5 py-2 text-left transition-colors {apolliaChatSelected
            ? 'bg-primary/10'
            : 'hover:bg-surface-1'}"
          data-testid="apollia-chat-pinned"
        >
          <div
            class="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-lg"
            style="background: linear-gradient(135deg, hsl(var(--primary)), hsl(var(--secondary))); color: white;"
          >
            <Zap size={13} />
          </div>
          <div class="min-w-0 flex-1">
            <div
              class="flex items-center gap-1.5 text-[12.5px] {apolliaChatSelected
                ? 'font-semibold text-foreground'
                : 'font-medium text-foreground'}"
            >
              <span class="truncate">Apollia Chat</span>
            </div>
            <div class="truncate text-[10.5px] text-muted-foreground">
              {$t("agents.apollia_chat_subtitle")}
            </div>
          </div>
        </Button>

        {#if $connectionStatus === "connecting" && allAssistants.length === 0}
          <div class="space-y-1">
            {#each Array(4) as _, i (i)}
              <div
                class="flex items-center gap-2.5 rounded-lg px-2.5 py-2"
              >
                <Skeleton class="h-7 w-7 rounded-lg bg-surface-2" />
                <div class="flex-1 space-y-1.5">
                  <Skeleton class="h-3 w-3/5 rounded bg-surface-2" />
                  <Skeleton class="h-2.5 w-2/5 rounded bg-surface-2" />
                </div>
              </div>
            {/each}
          </div>
        {:else if filteredAssistants.length === 0}
          <div class="px-2 pt-6">
            <EmptyState
              tone="primary"
              title={query.length > 0
                ? $t("agents.list_empty_no_result")
                : $t("agents.list_empty_no_assistant")}
              desc={query.length > 0
                ? $t("agents.list_empty_try_other")
                : $t("agents.list_empty_install_first")}
            >
              {#snippet icon()}
                <Bot size={22} />
              {/snippet}
              {#snippet action()}
                {#if query.length === 0}
                  <Button variant="primary-solid" size="sm" onclick={pickAndInstallAgent}>
                    {#snippet icon()}
                      <Plus size={12} />
                    {/snippet}
                    {$t("agents.new_assistant")}
                  </Button>
                {/if}
              {/snippet}
            </EmptyState>
          </div>
        {:else}
          {#each filteredAssistants as agent (agent.name)}
            {@const active = agent.name === selectedName && !apolliaChatSelected && selectedPackageName === null}
            {@const running = isActive(agent)}
            {@const transitioning = agent.runtime_status === "initializing" || agent.runtime_status === "stopping"}
            {@const busy = busyKeys[`agent:${agent.name}`] === true || transitioning}
            <div
              class="group mb-0.5 flex w-full items-center gap-1 rounded-lg pr-1 transition-colors {active
                ? 'bg-primary/10'
                : 'hover:bg-surface-1'}"
            >
              <Button variant="ghost" size="auto"
                type="button"
                onclick={() => {
                  selectedName = agent.name;
                  apolliaChatSelected = false;
                  selectedPackageName = null;
                }}
                class="flex min-w-0 flex-1 items-center gap-2.5 rounded-lg px-2.5 py-2 text-left"
                data-testid="agent-list-row"
                data-agent-name={agent.name}
              >
                <div
                  class="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-lg"
                  style="background: {running
                    ? 'linear-gradient(135deg, hsl(var(--primary)), hsl(var(--secondary)))'
                    : 'hsl(var(--surface-1))'}; color: {running
                    ? 'white'
                    : 'hsl(var(--primary))'}; border: {running
                    ? 'none'
                    : '1px solid hsl(var(--border))'};"
                >
                  <Sparkles size={13} />
                </div>
                <div class="min-w-0 flex-1">
                  <div
                    class="flex items-center gap-1.5 text-[12.5px] {active
                      ? 'font-semibold text-foreground'
                      : 'font-medium text-foreground'}"
                  >
                    <span class="truncate">{agent.name}</span>
                    {#if running}
                      <StatusDot color={statusColor(agent)} glow size={5} />
                    {:else if isIdle(agent)}
                      <StatusDot color={statusColor(agent)} size={5} />
                    {/if}
                  </div>
                  <div class="truncate text-[10.5px] text-muted-foreground">
                    {agent.description ?? statusLabel(agent)}
                  </div>
                </div>
              </Button>
              <Button variant="ghost" size="icon-sm"
                type="button"
                onclick={() => toggleAgentRuntime(agent)}
                disabled={busy || (!running && !agent.install_path)}
                class="-ml-1 text-muted-foreground transition-opacity hover:text-foreground hover:bg-transparent disabled:opacity-40 {running || busy
                  ? 'opacity-100'
                  : 'opacity-0 group-hover:opacity-100 focus-visible:opacity-100'}"
                aria-label={running
                  ? $t("agents.stop_named", { values: { name: agent.name } })
                  : $t("agents.start_named", { values: { name: agent.name } })}
                title={running ? $t("agents.stop") : $t("agents.start")}
                data-testid="agent-list-toggle"
                data-agent-name={agent.name}
              >
                {#if busy}
                  <Spinner size={13} />
                {:else if running}
                  <Square size={12} fill="currentColor" />
                {:else}
                  <Play size={13} fill="currentColor" />
                {/if}
              </Button>
            </div>
          {/each}
        {/if}

        <!-- Section header: packages -->
        <div
          class="section-meta mb-1.5 mt-4 px-2 text-[10px] tracking-[1.4px] /80"
        >
          {$t("agents.section_my_packages")} · {filteredPackages.length}
        </div>
        {#if filteredPackages.length === 0}
          <div class="px-2 py-3 text-[11px] text-muted-foreground/70">
            {query.length > 0
              ? $t("agents.packages_empty_no_result")
              : $t("agents.packages_empty_none_installed")}
          </div>
        {:else}
          {#each filteredPackages as pkg (pkg.name)}
            {@const pkgActive = pkg.name === selectedPackageName}
            {@const pkgState = packageRuntimeState(pkg, $agents, $triggers)}
            {@const pkgRunning = pkgState.status === "running" || pkgState.status === "partial"}
            {@const pkgBusy = busyKeys[`pkg:${pkg.name}`] === true}
            <div
              class="group mb-0.5 flex w-full items-center gap-1 rounded-lg pr-1 transition-colors {pkgActive
                ? 'bg-primary/10'
                : 'hover:bg-surface-1'}"
            >
              <Button variant="ghost" size="auto"
                type="button"
                onclick={() => selectPackage(pkg)}
                class="flex min-w-0 flex-1 items-center gap-2.5 rounded-lg px-2.5 py-2 text-left"
                data-testid="package-list-row"
                data-package-name={pkg.name}
              >
                <div
                  class="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-lg"
                  style="background: {pkgState.status === 'running'
                    ? 'linear-gradient(135deg, hsl(var(--primary)), hsl(var(--secondary)))'
                    : 'hsl(var(--surface-1))'}; color: {pkgState.status === 'running'
                    ? 'white'
                    : 'hsl(var(--primary))'}; border: {pkgState.status === 'running'
                    ? 'none'
                    : '1px solid hsl(var(--border))'};"
                >
                  <Package size={13} />
                </div>
                <div class="min-w-0 flex-1">
                  <div
                    class="flex items-center gap-1.5 text-[12.5px] {pkgActive
                      ? 'font-semibold text-foreground'
                      : 'font-medium text-foreground'}"
                  >
                    <span class="truncate">{pkg.name}</span>
                    {#if pkg.root_missing}
                      <AlertTriangle size={10} class="shrink-0 text-destructive/70" />
                    {:else}
                      <StatusDot
                        color={packageStatusColor(pkgState)}
                        glow={pkgState.status === "running"}
                        size={5}
                      />
                    {/if}
                  </div>
                  <div class="truncate text-[10.5px] text-muted-foreground">
                    {pkgState.runningAgents}/{pkgState.totalAgents} {$t("agents.agents_word")} · {pkgState.enabledTriggers}/{pkgState.totalTriggers} {$t("agents.triggers_word")}
                  </div>
                </div>
                <Badge size="sm" variant={packageStatusTone(pkgState)}>
                  {packageStatusLabel(pkgState)}
                </Badge>
              </Button>
              <Button variant="ghost" size="icon-sm"
                type="button"
                onclick={() => togglePackageRuntime(pkg)}
                disabled={pkgBusy || pkg.root_missing || pkg.agents.length === 0}
                class="-ml-1 text-muted-foreground transition-opacity hover:text-foreground hover:bg-transparent disabled:opacity-40 {pkgRunning || pkgBusy
                  ? 'opacity-100'
                  : 'opacity-0 group-hover:opacity-100 focus-visible:opacity-100'}"
                aria-label={pkgRunning
                  ? $t("agents.stop_named", { values: { name: pkg.name } })
                  : $t("agents.start_named", { values: { name: pkg.name } })}
                title={pkgRunning ? $t("agents.stop_all") : $t("agents.start_all")}
                data-testid="package-list-toggle"
                data-package-name={pkg.name}
              >
                {#if pkgBusy}
                  <Spinner size={13} />
                {:else if pkgRunning}
                  <Square size={12} fill="currentColor" />
                {:else}
                  <Play size={13} fill="currentColor" />
                {/if}
              </Button>
            </div>
          {/each}
        {/if}
      </div>
    </aside>

    <!-- RIGHT - detail -->
    <section class="flex min-w-0 flex-1 flex-col overflow-y-auto">
      {#if apolliaChatSelected}
        <DetailHeader title="Apollia Chat" meta={$t("agents.apollia_chat_detail_subtitle")}>
          {#snippet leading()}
            <div
              class="inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-lg"
              style="background: linear-gradient(135deg, hsl(var(--primary)), hsl(var(--secondary)));"
            >
              <Zap size={18} color="white" />
            </div>
          {/snippet}
        </DetailHeader>

        <div class="px-8 pt-[18px] pb-8">
          <ApolliaChatConfigPanel />
        </div>
      {:else if selectedPackage}
        {@const pkg = selectedPackage}
        {@const pkgState = selectedPackageState!}
        {@const pkgRunning = pkgState.status === "running" || pkgState.status === "partial"}
        {@const pkgBusy = busyKeys[`pkg:${pkg.name}`] === true}
        <!-- Detail header (screen identity is the breadcrumb) -->
        <DetailHeader
          title={pkg.name}
          titleTestid="package-detail-title"
          meta={pkg.description || $t("agents.package_default_description")}
        >
          {#snippet leading()}
            <div
              class="inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-lg"
              style="background: {pkgState.status === 'running'
                ? 'linear-gradient(135deg, hsl(var(--primary)), hsl(var(--secondary)))'
                : 'hsl(var(--surface-1))'}; color: {pkgState.status === 'running'
                ? 'white'
                : 'hsl(var(--primary))'}; border: {pkgState.status === 'running'
                ? 'none'
                : '1px solid hsl(var(--border))'};"
            >
              <Package size={18} />
            </div>
          {/snippet}
          {#snippet actions()}
            {#if confirmUninstallPkg === pkg.name}
              <Button variant="outline" size="sm" onclick={() => (confirmUninstallPkg = null)}>
                {$t("common.cancel")}
              </Button>
              <Button variant="primary-solid" size="sm" onclick={() => handleUninstallPkg(pkg.name)}>
                {#snippet icon()}
                  <Trash2 size={12} />
                {/snippet}
                {$t("common.confirm")}
              </Button>
            {:else}
              <Button variant="outline" size="sm"
                onclick={() => (confirmUninstallPkg = pkg.name)}
              >
                {#snippet icon()}
                  <Trash2 size={12} />
                {/snippet}
                {$t("agents.uninstall")}
              </Button>
              <Button variant="primary-solid" size="sm"
                onclick={() => togglePackageRuntime(pkg)}
                disabled={pkgBusy || pkg.root_missing || pkg.agents.length === 0}
              >
                {#snippet icon()}
                  {#if pkgBusy}
                    <Spinner size={12} />
                  {:else if pkgRunning}
                    <Square size={12} fill="currentColor" />
                  {:else}
                    <Play size={12} fill="currentColor" />
                  {/if}
                {/snippet}
                {pkgRunning ? $t("agents.stop_all") : $t("agents.start_all")}
              </Button>
            {/if}
          {/snippet}
          {#snippet footer()}
            <Badge size="sm" variant={packageStatusTone(pkgState)}>
              {#snippet icon()}
                <StatusDot
                  color={packageStatusColor(pkgState)}
                  glow={pkgState.status === "running"}
                  size={5}
                />
              {/snippet}
              {packageStatusLabel(pkgState)}
            </Badge>
            <Badge size="sm" variant="neutral">v{pkg.version}</Badge>
            <Badge size="sm" variant="neutral">
              {pkgState.runningAgents}/{pkgState.totalAgents} {$t("agents.agents_word")}
            </Badge>
            <Badge size="sm" variant="neutral">
              {pkgState.enabledTriggers}/{pkgState.totalTriggers} {$t("agents.triggers_word")}
            </Badge>
            {#if pkg.root_missing}
              <Badge size="sm" variant="warning">
                {#snippet icon()}
                  <AlertTriangle size={10} />
                {/snippet}
                {$t("agents.source_missing")}
              </Badge>
            {/if}
          {/snippet}
        </DetailHeader>

        {@const pkgNamesSet = new Set(pkg.agents.map((x) => x.name))}
        {@const pkgTriggers = $triggers.filter((tr) => pkgNamesSet.has(tr.agent))}

        <!-- Tabs (underline, aligné Projets) -->
        <div class="px-8 pt-3.5">
          <TabBar
            variant="underline"
            testidPrefix="package-detail"
            activeTab={packageDetailTab}
            items={[
              { key: "overview", label: $t("agents.tab_overview") },
              { key: "agents", label: $t("agents.tab_agents"), count: pkg.agents.length },
              { key: "triggers", label: $t("agents.tab_triggers"), count: pkgTriggers.length },
            ]}
            ontabchange={(key) => (packageDetailTab = key as PackageTab)}
          />
        </div>

        <!-- Tab content -->
        <div class="px-8 pt-5 pb-8" data-testid="package-detail-tab-content">
          {#if packageDetailTab === "overview"}
            <Card class="p-[14px_16px] max-w-3xl">
              <div class="mb-2 text-[12.5px] font-semibold text-foreground">{$t("agents.info_title")}</div>
              <div class="flex flex-col gap-2 text-[11.5px] text-muted-foreground">
                <div class="flex items-center gap-1.5">
                  <Clock size={11} />
                  <span>{$t("agents.installed_on", { values: { date: new Date(pkg.installed_at).toLocaleDateString() } })}</span>
                </div>
                <div class="flex items-center gap-1.5">
                  <FolderOpen size={11} class="shrink-0" />
                  <span class="truncate font-mono text-[11px]" title={pkg.root_path}>
                    {pkg.root_path}
                  </span>
                </div>
                {#if pkgDetail?.author}
                  <div class="flex items-center gap-1.5">
                    <Users size={11} />
                    <span>{pkgDetail.author}</span>
                  </div>
                {/if}
              </div>
            </Card>
          {:else if packageDetailTab === "agents"}
            <Card class="p-[14px_16px] max-w-3xl">
              <div class="mb-2.5 flex items-center justify-between">
                <span class="text-[12.5px] font-semibold text-foreground">
                  {$t("agents.tab_agents")} · {pkg.agents.length}
                </span>
              </div>
              {#if pkg.agents.length === 0}
                <div class="py-4 text-center text-[11.5px] text-muted-foreground">
                  {$t("agents.package_no_agent")}
                </div>
              {:else}
                {#each pkg.agents as pa, i (pa.name)}
                  {@const linked = $agents.find((x) => x.name === pa.name) ?? null}
                  {@const linkedRunning = linked ? isActive(linked) : false}
                  <div
                    class="flex items-center gap-2.5 py-2 {i === pkg.agents.length - 1
                      ? ''
                      : 'border-b border-border/40'}"
                  >
                    <div class="min-w-0 flex-1">
                      <Button variant="ghost" size="auto"
                        type="button"
                        onclick={() => {
                          if (linked) {
                            selectedName = linked.name;
                            selectedPackageName = null;
                          }
                        }}
                        disabled={!linked}
                        class="block w-full truncate text-left p-0 text-[12.5px] font-medium text-foreground hover:underline disabled:no-underline disabled:opacity-60"
                      >
                        {pa.name}
                      </Button>
                      <div class="mt-0.5 truncate text-[10.5px] text-muted-foreground">
                        {pa.entry}
                      </div>
                    </div>
                    <Badge size="sm" variant={pa.role === "director" ? "info" : "neutral"}>
                      {pa.role}
                    </Badge>
                    {#if linked}
                      <StatusDot color={statusColor(linked)} glow={linkedRunning} size={5} />
                    {/if}
                  </div>
                {/each}
              {/if}
            </Card>
          {:else if packageDetailTab === "triggers"}
            <Card class="p-[14px_16px] max-w-3xl">
              <div class="mb-2.5 flex items-center justify-between">
                <span class="text-[12.5px] font-semibold text-foreground">
                  {$t("agents.tab_triggers")} · {pkgTriggers.length}
                </span>
                <span class="font-mono text-[10.5px] text-muted-foreground">
                  {$t("agents.triggers_active_count", { values: { count: pkgState.enabledTriggers } })}
                </span>
              </div>
              {#if pkgTriggers.length === 0}
                <div class="py-4 text-center text-[11.5px] text-muted-foreground">
                  {$t("agents.no_trigger_configured")}
                </div>
              {:else}
                {#each pkgTriggers as tr, i (tr.id)}
                  <div
                    class="flex items-center gap-2.5 py-2 {i === pkgTriggers.length - 1
                      ? ''
                      : 'border-b border-border/40'}"
                  >
                    <div class="inline-flex h-[22px] w-[22px] items-center justify-center rounded-md bg-primary/10 text-primary">
                      <Zap size={11} />
                    </div>
                    <div class="min-w-0 flex-1">
                      <div class="font-mono truncate text-[12px] font-medium text-foreground">
                        {tr.id}
                      </div>
                      <div class="truncate text-[10.5px] text-muted-foreground">
                        {tr.agent} · {tr.source_kind}
                        {tr.source_config ? ` · ${tr.source_config}` : ""}
                      </div>
                    </div>
                    <Badge size="sm" variant={tr.enabled ? "success" : "neutral"}>
                      {tr.enabled ? $t("agents.trigger_active") : $t("agents.trigger_inactive")}
                    </Badge>
                  </div>
                {/each}
              {/if}
            </Card>
          {/if}
        </div>
      {:else if selected}
        {@const a = selected}
        <!-- Detail header (screen identity is the breadcrumb) -->
        <DetailHeader
          title={a.name}
          titleTestid="agent-detail-title"
          meta={a.description ?? $t("agents.no_description")}
        >
          {#snippet leading()}
            <div
              class="inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-lg"
              style="background: linear-gradient(135deg, hsl(var(--primary)), hsl(var(--secondary)));"
            >
              <Sparkles size={18} color="white" />
            </div>
          {/snippet}
          {#snippet actions()}
            <Button variant="primary-solid" size="sm"
              onclick={() => startChatWithAgent(a.name)}
              disabled={!isActive(a)}
            >
              {#snippet icon()}
                <MessageSquare size={12} />
              {/snippet}
              {$t("agents.new_chat")}
            </Button>
          {/snippet}
          {#snippet footer()}
            <Badge size="sm" variant={statusTone(a)}>
              {#snippet icon()}
                <StatusDot
                  color={statusColor(a)}
                  glow={a.runtime_status === "active"}
                  size={5}
                />
              {/snippet}
              {statusLabel(a)}
            </Badge>
            <Badge size="sm" variant="neutral">v{a.version}</Badge>
            <Badge size="sm" variant="neutral">
              {a.tools_required.length + a.tools_optional.length} {$t("agents.tools_word")}
            </Badge>
            {#if agentClassLabel(a)}
              <Badge size="sm" variant="info">{agentClassLabel(a)}</Badge>
            {/if}
            {#if a.execution_mode}
              <Badge size="sm" variant="neutral">{a.execution_mode}</Badge>
            {/if}
            {#if a.supports_a2a}
              <Badge size="sm" variant="info">A2A</Badge>
            {/if}
          {/snippet}
        </DetailHeader>

        {@const isInstalledNow = a.installed_at !== null}
        {@const runtimeStatus = a.runtime_status}
        {@const isRunningNow =
          runtimeStatus === "active" || runtimeStatus === "degraded"}
        {@const isLoadedNow = runtimeStatus !== null}
        {@const showA2AMessages =
          a.supports_a2a === true && $uiMode === "builder"}
        {@const allToolsAgent = [
          ...a.tools_required.map((id) => ({ id, required: true })),
          ...a.tools_optional.map((id) => ({ id, required: false })),
        ]}

        <!-- Tabs (underline pattern, aligné Projets) -->
        <div class="px-8 pt-3.5">
          <TabBar
            variant="underline"
            testidPrefix="agent-detail"
            activeTab={agentDetailTab}
            items={[
              { key: "overview", label: $t("agents.tab_overview") },
              { key: "tools", label: $t("agents.tab_tools"), count: allToolsAgent.length },
              { key: "memory", label: $t("agents.tab_memory") },
              { key: "activity", label: $t("agents.tab_activity") },
              { key: "settings", label: $t("agents.tab_settings") },
            ]}
            ontabchange={(key) => (agentDetailTab = key as AgentTab)}
          />
        </div>

        <!-- Tab content -->
        <div class="px-8 pt-5 pb-8" data-testid="agent-detail-tab-content">
          {#if agentDetailTab === "overview"}
            <div class="grid grid-cols-1 gap-4 lg:grid-cols-2">
              <!-- Stat: tools count -->
              <Card class="p-[16px_18px]">
                <div class="font-mono text-[10.5px] font-semibold uppercase tracking-[1.2px] text-muted-foreground">
                  {$t("agents.tab_tools")}
                </div>
                <div class="mt-1 tabular-nums" style="font-size: 24px; font-weight: 600; letter-spacing: -0.3px;">
                  {a.tools_required.length + a.tools_optional.length}
                </div>
                <div class="mt-0.5 text-[11px] text-muted-foreground">
                  {$t("agents.tools_required_optional", { values: { required: a.tools_required.length, optional: a.tools_optional.length } })}
                </div>
              </Card>

              <!-- Stat: triggers count -->
              <Card class="p-[16px_18px]">
                <div class="font-mono text-[10.5px] font-semibold uppercase tracking-[1.2px] text-muted-foreground">
                  {$t("agents.triggers_stat_title")}
                </div>
                <div class="mt-1 tabular-nums" style="font-size: 24px; font-weight: 600; letter-spacing: -0.3px;">
                  {a.tags.filter((tag) => tag.startsWith("trigger:")).length}
                </div>
                <div class="mt-0.5 text-[11px] text-muted-foreground">
                  {a.tags.filter((tag) => tag.startsWith("trigger:")).length === 0
                    ? $t("agents.triggers_stat_none")
                    : $t("agents.triggers_stat_active")}
                </div>
              </Card>

              <!-- Info grid: execution_mode + install_path -->
              {#if a.execution_mode}
                <Card class="p-[16px_18px]">
                  <div class="flex items-center gap-2 mb-1.5">
                    <Cpu size={12} class="text-muted-foreground/50" />
                    <span class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50">
                      {$t('agent_detail.execution_mode_title')}
                    </span>
                  </div>
                  <p class="text-sm text-foreground/80">{executionModeLabel(a.execution_mode)}</p>
                </Card>
              {/if}
              {#if a.install_path}
                <Card class="p-[16px_18px]">
                  <div class="flex items-center gap-2 mb-1.5">
                    <Terminal size={12} class="text-muted-foreground/50" />
                    <span class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50">
                      {$t('agent_detail.install_path_title')}
                    </span>
                  </div>
                  <p class="text-xs text-muted-foreground font-mono truncate" title={a.install_path}>{a.install_path}</p>
                </Card>
              {/if}

              <!-- Examples -->
              {#if a.examples.length > 0}
                <Card class="p-[14px_16px] lg:col-span-2">
                  <div class="font-mono mb-2 text-[10px] font-semibold uppercase tracking-[1.2px] text-muted-foreground">
                    {$t("agents.examples_title")}
                  </div>
                  <ul class="m-0 list-none space-y-1 p-0">
                    {#each a.examples.slice(0, 5) as ex (ex)}
                      <li class="text-[12px] text-foreground/85">- {ex}</li>
                    {/each}
                  </ul>
                </Card>
              {/if}
            </div>
          {:else if agentDetailTab === "tools"}
            <Card class="p-[14px_16px]">
              <div class="mb-2.5 flex items-center justify-between">
                <span class="text-[12.5px] font-semibold text-foreground">{$t("agents.available_tools")}</span>
                <span class="font-mono text-[10.5px] text-muted-foreground">{allToolsAgent.length}</span>
              </div>
              {#if allToolsAgent.length === 0}
                <div class="py-6 text-center text-[11.5px] text-muted-foreground">
                  {$t("agents.no_tool_declared")}
                </div>
              {:else}
                {#each allToolsAgent as tool, i (tool.id)}
                  {@const sensitive =
                    tool.id.startsWith("fs.write") ||
                    tool.id.includes("send") ||
                    tool.id.includes("delete")}
                  {@const scope = tool.id.split(/[._]/)[0] || "tool"}
                  <div
                    class="flex items-center gap-2.5 py-2 {i === allToolsAgent.length - 1 ? '' : 'border-b border-border/40'}"
                  >
                    <div class="inline-flex h-[22px] w-[22px] items-center justify-center rounded-md bg-primary/10 text-primary">
                      <Zap size={11} />
                    </div>
                    <div class="min-w-0 flex-1">
                      <div class="font-mono truncate text-[12px] font-medium text-foreground">{tool.id}</div>
                      <div class="text-[10.5px] text-muted-foreground">
                        {tool.required ? $t("agents.tool_required") : $t("agents.tool_optional")}
                      </div>
                    </div>
                    <Badge size="sm" variant="neutral">{scope}</Badge>
                    {#if sensitive}
                      <Badge size="sm" variant="warning">{$t("agents.tool_ask_each_time")}</Badge>
                    {/if}
                  </div>
                {/each}
              {/if}
            </Card>
          {:else if agentDetailTab === "memory"}
            {@const agentNamespace = a.memory_namespace}
            <div class="space-y-4 max-w-3xl">
              {#if !agentNamespace}
                <Card class="p-[14px_16px]">
                  <p class="text-[12px] text-muted-foreground italic">
                    {$t("agents.memory_none_declared")}
                  </p>
                </Card>
              {:else}
                <!-- Namespace breadcrumb -->
                <div class="flex items-center gap-3 text-[11.5px] text-muted-foreground">
                  <div class="inline-flex items-center gap-1.5">
                    <span class="font-mono text-foreground">{agentNamespace}</span>
                    <span class="text-muted-foreground/60">·</span>
                    <span class="tabular-nums">
                      {memoryEntries.length > 1
                        ? $t("agents.memory_entries_plural", { values: { count: memoryEntries.length } })
                        : $t("agents.memory_entries_singular", { values: { count: memoryEntries.length } })}
                    </span>
                  </div>
                  {#if a.shared_memory_namespaces.length > 0}
                    <span class="text-muted-foreground/60">·</span>
                    <span class="text-[10.5px]">
                      {a.shared_memory_namespaces.length > 1
                        ? $t("agents.memory_shared_namespaces_plural", { values: { count: a.shared_memory_namespaces.length } })
                        : $t("agents.memory_shared_namespaces_singular", { values: { count: a.shared_memory_namespaces.length } })}
                    </span>
                  {/if}
                </div>

                <!-- Entries -->
                <Card class="overflow-hidden">
                  {#if memoryLoading}
                    <div class="px-4 py-6 text-center text-[12px] text-muted-foreground">
                      {$t("agents.memory_loading")}
                    </div>
                  {:else if memoryError}
                    <div class="px-4 py-3 text-[12px] text-destructive" data-testid="agent-memory-error">
                      {memoryError}
                    </div>
                  {:else if memoryEntries.length === 0}
                    <div class="px-4 py-8 text-center">
                      <p class="text-[12.5px] text-muted-foreground mb-1">
                        {$t("agents.memory_empty_namespace")}
                      </p>
                      <p class="text-[10.5px] text-muted-foreground/70">
                        {$t("agents.memory_empty_hint")}
                      </p>
                    </div>
                  {:else}
                    {#each memoryEntries.slice(0, 12) as entry, i (entry.id)}
                      {@const isLast = i === Math.min(memoryEntries.length, 12) - 1}
                      <div
                        class="flex items-start gap-3 px-4 py-2.5 {isLast ? '' : 'border-b border-border/40'}"
                        data-testid="agent-memory-row"
                      >
                        <div class="w-[22px] h-[22px] rounded-md shrink-0 mt-0.5 inline-flex items-center justify-center bg-muted text-muted-foreground">
                          {#if entry.entry_type === "episodic"}
                            <Clock size={11} />
                          {:else if entry.entry_type === "procedural"}
                            <Zap size={11} />
                          {:else}
                            <Sparkles size={11} />
                          {/if}
                        </div>
                        <div class="flex-1 min-w-0">
                          <div class="flex items-center gap-2 min-w-0">
                            <code class="text-[12px] truncate text-foreground" title={entry.key}>{entry.key}</code>
                            <span class="inline-flex shrink-0 items-center px-1.5 py-px rounded text-[9.5px] font-medium tabular-nums tracking-tight border border-border/60 bg-background text-muted-foreground">
                              {entry.entry_type.slice(0, 4).toLowerCase()}
                            </span>
                          </div>
                          {#if entry.value}
                            <div class="text-[11px] text-muted-foreground mt-1 truncate" title={entry.value}>
                              {entry.value.length > 140 ? entry.value.slice(0, 140) + "…" : entry.value}
                            </div>
                          {/if}
                          <div class="text-[10px] text-muted-foreground/70 mt-0.5 tabular-nums">
                            {$t("agents.memory_time_ago", { values: { time: formatMemoryRelative(entry.created_at) } })}
                          </div>
                        </div>
                      </div>
                    {/each}
                    {#if memoryEntries.length > 12}
                      <div class="px-4 py-2 text-[10.5px] text-muted-foreground/70 border-t border-border/40 text-center">
                        {memoryEntries.length - 12 > 1
                          ? $t("agents.memory_more_entries_plural", { values: { count: memoryEntries.length - 12 } })
                          : $t("agents.memory_more_entries_singular", { values: { count: memoryEntries.length - 12 } })}
                        - <Button variant="ghost" size="auto" onclick={handleMemoryLink} class="inline p-0 text-[10.5px] text-primary hover:underline">{$t("agents.memory_view_all")}</Button>
                      </div>
                    {/if}
                  {/if}
                </Card>

                <Button variant="outline" size="sm" onclick={handleMemoryLink} class="gap-2">
                  {#snippet icon()}<ExternalLink size={13} />{/snippet}
                  {$t('agent_detail.memory_link', { values: { namespace: agentNamespace } })}
                </Button>
              {/if}
            </div>
          {:else if agentDetailTab === "activity"}
            <div class="space-y-4 max-w-3xl" data-testid="agent-detail-activity">
              {#if a.id}
                <Card class="p-[14px_16px]">
                  <AgentActivity agentId={a.id} onTaskClick={handleTaskClick} />
                </Card>
              {:else}
                <Card class="p-[14px_16px]">
                  <p class="text-[12px] text-muted-foreground italic">
                    {$t("agents.activity_not_loaded")}
                  </p>
                </Card>
              {/if}

              <Card class="p-[14px_16px]">
                <AgentTriggers agentName={a.name} />
              </Card>
            </div>
          {:else if agentDetailTab === "settings"}
            <div class="space-y-4 max-w-3xl" data-testid="agent-detail-settings">
              {#if stopError || actionError}
                <div class="rounded-lg border border-destructive/20 bg-destructive/5 px-3 py-2 text-xs text-destructive">
                  {stopError || actionError}
                </div>
              {/if}

              <!-- Runtime controls -->
              <Card class="p-[14px_16px]">
                <div class="font-mono text-[10px] font-semibold uppercase tracking-[1.2px] text-muted-foreground mb-3">
                  {$t("agents.execution_section")}
                </div>
                {#if confirmStopVisible}
                  <div class="flex items-center gap-2 flex-wrap">
                    <span class="text-xs text-muted-foreground">{$t('agents.stop_confirm')}</span>
                    <Button size="sm" variant="destructive"
                      onclick={() => handleStopSelected(a)}
                      disabled={stoppingAgent}
                      data-testid="agent-detail-stop-confirm"
                    >
                      {stoppingAgent ? $t('agents.stopping') : $t('common.confirm')}
                    </Button>
                    <Button size="sm" variant="outline" onclick={() => (confirmStopVisible = false)}>
                      {$t('common.cancel')}
                    </Button>
                  </div>
                {:else}
                  <div class="flex items-center gap-2 flex-wrap">
                    {#if !isLoadedNow && isInstalledNow && a.install_path}
                      <Button size="sm" variant="primary-solid"
                        onclick={() => handleStartSelected(a)}
                        disabled={startLoading}
                        data-testid="agent-detail-start-btn"
                      >
                        {#snippet icon()}<Play size={12} />{/snippet}
                        {startLoading ? $t('agents.starting_agent') : $t('agents.start')}
                      </Button>
                    {/if}
                    {#if isRunningNow && a.id}
                      <Button size="sm" variant="outline"
                        onclick={() => (confirmStopVisible = true)}
                        data-testid="agent-detail-stop-btn"
                      >
                        {#snippet icon()}<Square size={12} />{/snippet}
                        {$t('agents.stop')}
                      </Button>
                    {/if}
                    {#if a.id}
                      <Button size="sm" variant="ghost"
                        onclick={() => openLogs(a.id!)}
                        data-testid="agent-detail-logs-btn"
                      >
                        {$t('agents.logs')}
                      </Button>
                    {/if}
                  </div>
                {/if}

                {#if isInstalledNow}
                  <div class="mt-3 pt-3 border-t border-border/40 flex items-center justify-between gap-3">
                    <div>
                      <div class="text-[12px] font-medium text-foreground">{$t("agents.auto_start_enabled")}</div>
                      <div class="text-[10.5px] text-muted-foreground mt-0.5">
                        {$t("agents.auto_start_hint")}
                      </div>
                    </div>
                    <Toggle
                      checked={a.enabled}
                      onchange={() => handleToggleEnabled(a)}
                      disabled={toggleLoading}
                      size="sm"
                      aria-label={$t("agents.auto_start_enabled")}
                      data-testid="agent-detail-enabled-toggle"
                    />
                  </div>
                {/if}
              </Card>

              <!-- LLM info -->
              <Card class="p-[14px_16px]">
                <AgentLlmInfo />
              </Card>

              <!-- Agent messages (A2A + builder only) -->
              {#if showA2AMessages}
                <Card class="p-[14px_16px]">
                  <div class="flex items-center gap-2 mb-3">
                    <MessageSquare size={12} class="text-muted-foreground/50" />
                    <span class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50">
                      {$t('agent_detail.messages_title')}
                    </span>
                  </div>
                  <AgentMessagesPanel agentName={a.name} />
                </Card>
              {/if}
            </div>
          {/if}
        </div>
      {:else if $connectionStatus === "connecting"}
        <div class="flex flex-1 items-center justify-center px-8">
          <div class="text-[12.5px] text-muted-foreground">
            {$t("agents.loading_assistants")}
          </div>
        </div>
      {:else}
        <div class="flex flex-1 items-center justify-center px-8 py-14">
          <EmptyState
            tone="primary"
            title={$t("agents.detail_empty_title")}
            desc={$t("agents.list_empty_install_first")}
          >
            {#snippet icon()}
              <Bot size={22} />
            {/snippet}
            {#snippet action()}
              <Button variant="primary-solid" size="sm" onclick={pickAndInstallAgent}>
                {#snippet icon()}
                  <Plus size={12} />
                {/snippet}
                {$t("agents.new_assistant")}
              </Button>
            {/snippet}
          </EmptyState>
        </div>
      {/if}
    </section>
  </div>
</div>

{#if logsAgentId}
  <AgentLogs agentId={logsAgentId} open={logsOpen} onclose={closeLogs} />
{/if}

<InstallPackageDialog
  open={installPackageOpen}
  onclose={() => (installPackageOpen = false)}
  oninstalled={handlePkgInstalled}
/>
