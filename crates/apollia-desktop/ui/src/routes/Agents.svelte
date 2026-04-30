<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { get } from "svelte/store";
  import { t } from "svelte-i18n";
  import type { AgentListItem } from "$lib/types";
  import { agents } from "$lib/stores/agents";
  import { connectionStatus, triggers } from "$lib/stores/sse";
  import { navigateTo } from "$lib/stores/navigation";
  import { pendingChatSessionId } from "$lib/stores/chat";
  import { tourOpenAgentDetail } from "$lib/stores/tour";
  import {
    AlertTriangle,
    Bot,
    Clock,
    Download,
    FolderOpen,
    Loader2,
    Package,
    Play,
    Plus,
    Search,
    Sparkles,
    Square,
    MessageSquare,
    Settings,
    Trash2,
    Users,
    Zap,
  } from "lucide-svelte";
  import AgentLogs from "../components/agents/AgentLogs.svelte";
  import AgentDetail from "../components/agents/AgentDetail.svelte";
  import ApolliaChatConfigPanel from "../components/agents/ApolliaChatConfigPanel.svelte";
  import InstallPackageDialog from "../components/agents/InstallPackageDialog.svelte";
  import MacSandboxBanner from "../components/common/MacSandboxBanner.svelte";
  import {
    PageHeader,
    BtnPrimary,
    BtnSecondary,
    Chip,
    StatusDot,
    Card,
    EmptyState,
  } from "$lib/components/operator";
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

  // ── Existing state (preserved from previous implementation) ──────────
  let installingAgent = $state(false);
  let installError = $state<string | null>(null);
  let logsAgentId = $state<string | null>(null);
  let logsOpen = $state(false);
  let detailAgent = $state<AgentListItem | null>(null);
  let detailOpen = $state(false);

  let installPackageOpen = $state(false);

  // ── New V3 state ─────────────────────────────────────────────────────
  let query = $state("");
  let selectedName = $state<string | null>(null);
  /** Nom du package sélectionné, ou null. Mutuellement exclusif avec selectedName. */
  let selectedPackageName = $state<string | null>(null);
  /** Détail du package sélectionné, chargé à la volée. */
  let pkgDetail = $state<AgentPackageDetailView | null>(null);
  /** Pinned synthetic system agent — when true, the right column shows the
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

  function kindLabel(a: AgentListItem): string {
    if (a.tags.includes("trigger")) return "trigger";
    if (a.execution_mode === "orchestrated") return "tâche";
    if (a.agent_type === "system") return "système";
    return "libre";
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
        return "actif";
      case "degraded":
        return "dégradé";
      case "initializing":
        return "démarrage…";
      case "stopping":
        return "arrêt…";
      case "stopped":
        return "arrêté";
      default:
        return "non chargé";
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
  function openDetail(agent: AgentListItem) {
    detailAgent = agent;
    detailOpen = true;
  }
  function closeDetail() {
    detailOpen = false;
  }
  function openLogsFromDetail(agentId: string) {
    closeDetail();
    openLogs(agentId);
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
            `Impossible de démarrer ${a.name} : install_path manquant.`,
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
          `Package ${pkg.name} : ${result.errors.length} erreur(s) — ${result.errors[0]}`,
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
    if (s.status === "running") return "actif";
    if (s.status === "partial") return "partiel";
    return "arrêté";
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
  <!-- ── Header ──────────────────────────────────────────────────────── -->
  <PageHeader
    kicker="MES ASSISTANTS · {allAssistants.length}"
    title="Assistants"
    subtitle="Vos compagnons IA — chacun avec ses outils, sa mémoire et ses déclencheurs."
  >
    {#snippet actions()}
      <BtnSecondary onclick={() => (installPackageOpen = true)}>
        {#snippet icon()}
          <Package size={12} />
        {/snippet}
        Installer un package
      </BtnSecondary>
      <BtnPrimary
        onclick={pickAndInstallAgent}
        disabled={installingAgent}
      >
        {#snippet icon()}
          <Download size={12} />
        {/snippet}
        {installingAgent
          ? $t("agents.installing")
          : "Nouvel assistant"}
      </BtnPrimary>
    {/snippet}
  </PageHeader>

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
    <!-- LEFT — list -->
    <aside
      class="flex w-[320px] shrink-0 flex-col border-r border-border/60"
      data-testid="agents-list"
    >
      <div class="px-[18px] pb-[10px] pt-4">
        <div
          class="flex items-center gap-[7px] rounded-md border border-border bg-surface-1 px-2.5 py-[7px]"
        >
          <Search size={11} class="text-muted-foreground" />
          <input
            type="text"
            bind:value={query}
            placeholder="Filtrer"
            class="flex-1 border-none bg-transparent text-[11.5px] text-foreground placeholder:text-muted-foreground focus:outline-none"
            data-testid="agents-search"
          />
        </div>
      </div>

      <div class="flex-1 overflow-y-auto px-2.5 pb-2">
        <!-- Section header: assistants -->
        <div
          class="font-mono mb-1.5 mt-1 px-2 text-[10px] font-semibold uppercase tracking-[1.4px] text-muted-foreground/80"
        >
          Mes assistants · {filteredAssistants.length}
        </div>
        <!-- Pinned system agent: Apollia Chat -->
        <button
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
              Agent système · chat libre
            </div>
          </div>
          <Chip size="sm" tone="info">Système</Chip>
        </button>

        {#if $connectionStatus === "connecting" && allAssistants.length === 0}
          <div class="space-y-1">
            {#each Array(4) as _, i (i)}
              <div
                class="flex items-center gap-2.5 rounded-lg px-2.5 py-2"
              >
                <div class="h-7 w-7 animate-pulse rounded-lg bg-surface-2"></div>
                <div class="flex-1 space-y-1.5">
                  <div class="h-3 w-3/5 animate-pulse rounded bg-surface-2"></div>
                  <div class="h-2.5 w-2/5 animate-pulse rounded bg-surface-2"></div>
                </div>
              </div>
            {/each}
          </div>
        {:else if filteredAssistants.length === 0}
          <div class="px-2 pt-6">
            <EmptyState
              tone="primary"
              title={query.length > 0
                ? "Aucun résultat"
                : "Aucun assistant"}
              desc={query.length > 0
                ? "Essayez un autre terme."
                : "Installez votre premier assistant pour commencer."}
            >
              {#snippet icon()}
                <Bot size={22} />
              {/snippet}
              {#snippet action()}
                {#if query.length === 0}
                  <BtnPrimary onclick={pickAndInstallAgent}>
                    {#snippet icon()}
                      <Plus size={12} />
                    {/snippet}
                    Nouvel assistant
                  </BtnPrimary>
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
              <button
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
                <Chip size="sm" tone="neutral">{kindLabel(agent)}</Chip>
              </button>
              <button
                type="button"
                onclick={() => toggleAgentRuntime(agent)}
                disabled={busy || (!running && !agent.install_path)}
                class="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-surface-2 hover:text-foreground disabled:opacity-40 disabled:hover:bg-transparent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                aria-label={running ? `Arrêter ${agent.name}` : `Démarrer ${agent.name}`}
                title={running ? "Arrêter" : "Démarrer"}
                data-testid="agent-list-toggle"
                data-agent-name={agent.name}
              >
                {#if busy}
                  <Loader2 size={13} class="animate-spin" />
                {:else if running}
                  <Square size={12} fill="currentColor" />
                {:else}
                  <Play size={13} fill="currentColor" />
                {/if}
              </button>
            </div>
          {/each}
        {/if}

        <!-- Section header: packages -->
        <div
          class="font-mono mb-1.5 mt-4 px-2 text-[10px] font-semibold uppercase tracking-[1.4px] text-muted-foreground/80"
        >
          Mes packages · {filteredPackages.length}
        </div>
        {#if filteredPackages.length === 0}
          <div class="px-2 py-3 text-[11px] text-muted-foreground/70">
            {query.length > 0
              ? "Aucun package."
              : "Aucun package installé."}
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
              <button
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
                    {pkgState.runningAgents}/{pkgState.totalAgents} agents · {pkgState.enabledTriggers}/{pkgState.totalTriggers} triggers
                  </div>
                </div>
                <Chip size="sm" tone={packageStatusTone(pkgState)}>
                  {packageStatusLabel(pkgState)}
                </Chip>
              </button>
              <button
                type="button"
                onclick={() => togglePackageRuntime(pkg)}
                disabled={pkgBusy || pkg.root_missing || pkg.agents.length === 0}
                class="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-surface-2 hover:text-foreground disabled:opacity-40 disabled:hover:bg-transparent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                aria-label={pkgRunning ? `Arrêter ${pkg.name}` : `Démarrer ${pkg.name}`}
                title={pkgRunning ? "Tout arrêter" : "Tout démarrer"}
                data-testid="package-list-toggle"
                data-package-name={pkg.name}
              >
                {#if pkgBusy}
                  <Loader2 size={13} class="animate-spin" />
                {:else if pkgRunning}
                  <Square size={12} fill="currentColor" />
                {:else}
                  <Play size={13} fill="currentColor" />
                {/if}
              </button>
            </div>
          {/each}
        {/if}
      </div>
    </aside>

    <!-- RIGHT — detail -->
    <section class="flex min-w-0 flex-1 flex-col overflow-y-auto">
      {#if apolliaChatSelected}
        <div class="border-b border-border/40 px-8 pb-4 pt-[22px]">
          <div class="flex items-start gap-3.5">
            <div
              class="inline-flex h-[46px] w-[46px] shrink-0 items-center justify-center rounded-xl"
              style="background: linear-gradient(135deg, hsl(var(--primary)), hsl(var(--secondary)));"
            >
              <Zap size={20} color="white" />
            </div>
            <div class="min-w-0 flex-1">
              <h2
                class="m-0 text-foreground"
                style="font-size: 22px; font-weight: 600; letter-spacing: -0.4px; line-height: 1.2;"
              >
                Apollia Chat
              </h2>
              <p class="mt-1 max-w-[540px] text-[12.5px] leading-[1.5] text-muted-foreground">
                Votre assistant intégré — il vous accompagne au quotidien dans
                le chat libre. Personnalisez sa personnalité, ses outils, et
                le modèle qu'il utilise.
              </p>
            </div>
            <div class="flex shrink-0 gap-1.5">
              <Chip size="sm" tone="info">Système</Chip>
            </div>
          </div>
        </div>

        <div class="px-8 pt-[18px] pb-8">
          <ApolliaChatConfigPanel />
        </div>
      {:else if selectedPackage}
        {@const pkg = selectedPackage}
        {@const pkgState = selectedPackageState!}
        {@const pkgRunning = pkgState.status === "running" || pkgState.status === "partial"}
        {@const pkgBusy = busyKeys[`pkg:${pkg.name}`] === true}
        <!-- Header -->
        <div class="border-b border-border/40 px-8 pb-4 pt-[22px]">
          <div class="flex items-start gap-3.5">
            <div
              class="inline-flex h-[46px] w-[46px] shrink-0 items-center justify-center rounded-xl"
              style="background: {pkgState.status === 'running'
                ? 'linear-gradient(135deg, hsl(var(--primary)), hsl(var(--secondary)))'
                : 'hsl(var(--surface-1))'}; color: {pkgState.status === 'running'
                ? 'white'
                : 'hsl(var(--primary))'}; border: {pkgState.status === 'running'
                ? 'none'
                : '1px solid hsl(var(--border))'};"
            >
              <Package size={20} />
            </div>
            <div class="min-w-0 flex-1">
              <h2
                class="m-0 text-foreground"
                style="font-size: 22px; font-weight: 600; letter-spacing: -0.4px; line-height: 1.2;"
                data-testid="package-detail-title"
              >
                {pkg.name}
              </h2>
              <p class="mt-1 max-w-[540px] text-[12.5px] leading-[1.5] text-muted-foreground">
                {pkg.description || "Package d'agents Apollia."}
              </p>
            </div>
            <div class="flex shrink-0 gap-1.5">
              {#if confirmUninstallPkg === pkg.name}
                <BtnSecondary onclick={() => (confirmUninstallPkg = null)}>
                  Annuler
                </BtnSecondary>
                <BtnPrimary onclick={() => handleUninstallPkg(pkg.name)}>
                  {#snippet icon()}
                    <Trash2 size={12} />
                  {/snippet}
                  Confirmer
                </BtnPrimary>
              {:else}
                <BtnSecondary
                  onclick={() => (confirmUninstallPkg = pkg.name)}
                >
                  {#snippet icon()}
                    <Trash2 size={12} />
                  {/snippet}
                  Désinstaller
                </BtnSecondary>
                <BtnPrimary
                  onclick={() => togglePackageRuntime(pkg)}
                  disabled={pkgBusy || pkg.root_missing || pkg.agents.length === 0}
                >
                  {#snippet icon()}
                    {#if pkgBusy}
                      <Loader2 size={12} class="animate-spin" />
                    {:else if pkgRunning}
                      <Square size={12} fill="currentColor" />
                    {:else}
                      <Play size={12} fill="currentColor" />
                    {/if}
                  {/snippet}
                  {pkgRunning ? "Tout arrêter" : "Tout démarrer"}
                </BtnPrimary>
              {/if}
            </div>
          </div>
          <div class="mt-3.5 flex flex-wrap gap-2">
            <Chip size="sm" tone={packageStatusTone(pkgState)}>
              {#snippet icon()}
                <StatusDot
                  color={packageStatusColor(pkgState)}
                  glow={pkgState.status === "running"}
                  size={5}
                />
              {/snippet}
              {packageStatusLabel(pkgState)}
            </Chip>
            <Chip size="sm" tone="neutral">v{pkg.version}</Chip>
            <Chip size="sm" tone="neutral">
              {pkgState.runningAgents}/{pkgState.totalAgents} agents
            </Chip>
            <Chip size="sm" tone="neutral">
              {pkgState.enabledTriggers}/{pkgState.totalTriggers} triggers
            </Chip>
            {#if pkg.root_missing}
              <Chip size="sm" tone="warning">
                {#snippet icon()}
                  <AlertTriangle size={10} />
                {/snippet}
                source manquante
              </Chip>
            {/if}
          </div>
        </div>

        <!-- Content -->
        <div class="px-8 pt-[18px] pb-8">
          <div class="grid grid-cols-1 gap-4 lg:grid-cols-2">
            <!-- Meta -->
            <Card class="p-[14px_16px]">
              <div class="mb-2 text-[12.5px] font-semibold text-foreground">
                Informations
              </div>
              <div class="flex flex-col gap-2 text-[11.5px] text-muted-foreground">
                <div class="flex items-center gap-1.5">
                  <Clock size={11} />
                  <span>
                    Installé le {new Date(pkg.installed_at).toLocaleDateString()}
                  </span>
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

            <!-- Agents -->
            <Card class="p-[14px_16px]">
              <div class="mb-2.5 flex items-center justify-between">
                <span class="text-[12.5px] font-semibold text-foreground">
                  Agents · {pkg.agents.length}
                </span>
              </div>
              {#if pkg.agents.length === 0}
                <div class="py-4 text-center text-[11.5px] text-muted-foreground">
                  Aucun agent.
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
                      <button
                        type="button"
                        onclick={() => {
                          if (linked) {
                            selectedName = linked.name;
                            selectedPackageName = null;
                          }
                        }}
                        disabled={!linked}
                        class="block w-full truncate text-left text-[12.5px] font-medium text-foreground hover:underline disabled:no-underline disabled:opacity-60"
                      >
                        {pa.name}
                      </button>
                      <div class="mt-0.5 truncate text-[10.5px] text-muted-foreground">
                        {pa.entry}
                      </div>
                    </div>
                    <Chip size="sm" tone={pa.role === "director" ? "info" : "neutral"}>
                      {pa.role}
                    </Chip>
                    {#if linked}
                      <StatusDot
                        color={statusColor(linked)}
                        glow={linkedRunning}
                        size={5}
                      />
                    {/if}
                  </div>
                {/each}
              {/if}
            </Card>

            <!-- Triggers -->
            <Card class="p-[14px_16px] lg:col-span-2">
              {@const pkgNamesSet = new Set(pkg.agents.map((x) => x.name))}
              {@const pkgTriggers = $triggers.filter((t) => pkgNamesSet.has(t.agent))}
              <div class="mb-2.5 flex items-center justify-between">
                <span class="text-[12.5px] font-semibold text-foreground">
                  Triggers · {pkgTriggers.length}
                </span>
                <span class="font-mono text-[10.5px] text-muted-foreground">
                  {pkgState.enabledTriggers} actifs
                </span>
              </div>
              {#if pkgTriggers.length === 0}
                <div class="py-4 text-center text-[11.5px] text-muted-foreground">
                  Aucun trigger configuré.
                </div>
              {:else}
                {#each pkgTriggers as tr, i (tr.id)}
                  <div
                    class="flex items-center gap-2.5 py-2 {i === pkgTriggers.length - 1
                      ? ''
                      : 'border-b border-border/40'}"
                  >
                    <div
                      class="inline-flex h-[22px] w-[22px] items-center justify-center rounded-md bg-primary/10 text-primary"
                    >
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
                    <Chip size="sm" tone={tr.enabled ? "success" : "neutral"}>
                      {tr.enabled ? "actif" : "inactif"}
                    </Chip>
                  </div>
                {/each}
              {/if}
            </Card>
          </div>
        </div>
      {:else if selected}
        {@const a = selected}
        <!-- Header -->
        <div class="border-b border-border/40 px-8 pb-4 pt-[22px]">
          <div class="flex items-start gap-3.5">
            <div
              class="inline-flex h-[46px] w-[46px] shrink-0 items-center justify-center rounded-xl"
              style="background: linear-gradient(135deg, hsl(var(--primary)), hsl(var(--secondary)));"
            >
              <Sparkles size={20} color="white" />
            </div>
            <div class="min-w-0 flex-1">
              <h2
                class="m-0 text-foreground"
                style="font-size: 22px; font-weight: 600; letter-spacing: -0.4px; line-height: 1.2;"
                data-testid="agent-detail-title"
              >
                {a.name}
              </h2>
              <p
                class="mt-1 max-w-[540px] text-[12.5px] leading-[1.5] text-muted-foreground"
              >
                {a.description ?? $t("agents.no_description")}
              </p>
            </div>
            <div class="flex shrink-0 gap-1.5">
              <BtnSecondary onclick={() => openDetail(a)}>
                {#snippet icon()}
                  <Settings size={12} />
                {/snippet}
                Configurer
              </BtnSecondary>
              <BtnPrimary
                onclick={() => startChatWithAgent(a.name)}
                disabled={!isActive(a)}
              >
                {#snippet icon()}
                  <MessageSquare size={12} />
                {/snippet}
                Nouveau chat
              </BtnPrimary>
            </div>
          </div>
          <div class="mt-3.5 flex flex-wrap gap-2">
            <Chip size="sm" tone={statusTone(a)}>
              {#snippet icon()}
                <StatusDot
                  color={statusColor(a)}
                  glow={a.runtime_status === "active"}
                  size={5}
                />
              {/snippet}
              {statusLabel(a)}
            </Chip>
            <Chip size="sm" tone="neutral">v{a.version}</Chip>
            <Chip size="sm" tone="neutral">
              {a.tools_required.length + a.tools_optional.length} outils
            </Chip>
            {#if agentClassLabel(a)}
              <Chip size="sm" tone="info">{agentClassLabel(a)}</Chip>
            {/if}
            {#if a.execution_mode}
              <Chip size="sm" tone="neutral">{a.execution_mode}</Chip>
            {/if}
            {#if a.supports_a2a}
              <Chip size="sm" tone="info">A2A</Chip>
            {/if}
          </div>
        </div>

        <!-- Content grid -->
        <div class="px-8 pt-[18px] pb-8">
          <div
            class="grid grid-cols-1 gap-4 lg:grid-cols-2"
            data-testid="agent-detail-grid"
          >
            <!-- Stat: tools count -->
            <Card class="p-[16px_18px]">
              <div
                class="font-mono text-[10.5px] font-semibold uppercase tracking-[1.2px] text-muted-foreground"
              >
                Outils
              </div>
              <div
                class="mt-1 tabular-nums"
                style="font-size: 24px; font-weight: 600; letter-spacing: -0.3px;"
              >
                {a.tools_required.length + a.tools_optional.length}
              </div>
              <div class="mt-0.5 text-[11px] text-muted-foreground">
                {a.tools_required.length} requis · {a.tools_optional.length} optionnels
              </div>
            </Card>

            <!-- Stat: triggers count (placeholder, sourced from tags) -->
            <Card class="p-[16px_18px]">
              <div
                class="font-mono text-[10.5px] font-semibold uppercase tracking-[1.2px] text-muted-foreground"
              >
                Déclencheurs
              </div>
              <div
                class="mt-1 tabular-nums"
                style="font-size: 24px; font-weight: 600; letter-spacing: -0.3px;"
              >
                {a.tags.filter((tag) => tag.startsWith("trigger:")).length}
              </div>
              <div class="mt-0.5 text-[11px] text-muted-foreground">
                {a.tags.filter((tag) => tag.startsWith("trigger:")).length === 0
                  ? "aucun trigger configuré"
                  : "actifs"}
              </div>
            </Card>

            <!-- Tools panel -->
            <div class="lg:row-span-2">
              <Card class="p-[14px_16px]">
                <div class="mb-2.5 flex items-center justify-between">
                  <span class="text-[12.5px] font-semibold text-foreground"
                    >Outils disponibles</span
                  >
                  <button
                    type="button"
                    onclick={() => openDetail(a)}
                    class="cursor-pointer border-none bg-transparent text-[11px] text-primary hover:underline"
                  >
                    Gérer →
                  </button>
                </div>

                {@const allTools = [
                  ...a.tools_required.map((id) => ({
                    id,
                    required: true,
                  })),
                  ...a.tools_optional.map((id) => ({
                    id,
                    required: false,
                  })),
                ]}

                {#if allTools.length === 0}
                  <div class="py-6 text-center text-[11.5px] text-muted-foreground">
                    Aucun outil déclaré.
                  </div>
                {:else}
                  {#each allTools as tool, i (tool.id)}
                    {@const sensitive = tool.id.startsWith("fs.write") ||
                      tool.id.includes("send") ||
                      tool.id.includes("delete")}
                    {@const scope = tool.id.split(/[._]/)[0] || "tool"}
                    <div
                      class="flex items-center gap-2.5 py-2 {i ===
                      allTools.length - 1
                        ? ''
                        : 'border-b border-border/40'}"
                    >
                      <div
                        class="inline-flex h-[22px] w-[22px] items-center justify-center rounded-md bg-primary/10 text-primary"
                      >
                        <Zap size={11} />
                      </div>
                      <div class="min-w-0 flex-1">
                        <div
                          class="font-mono truncate text-[12px] font-medium text-foreground"
                        >
                          {tool.id}
                        </div>
                        <div class="text-[10.5px] text-muted-foreground">
                          {tool.required ? "Requis" : "Optionnel"}
                        </div>
                      </div>
                      <Chip size="sm" tone="neutral">{scope}</Chip>
                      {#if sensitive}
                        <Chip size="sm" tone="warning"
                          >demander à chaque fois</Chip
                        >
                      {/if}
                    </div>
                  {/each}
                {/if}
              </Card>
            </div>

            <!-- Memory panel -->
            <Card class="p-[14px_16px]">
              <div class="mb-2 flex items-center justify-between">
                <span class="text-[12.5px] font-semibold text-foreground"
                  >Mémoire</span
                >
                <span
                  class="font-mono text-[10.5px] text-muted-foreground"
                >
                  {a.tags.length} tag{a.tags.length > 1 ? "s" : ""}
                </span>
              </div>
              <p
                class="m-0 mb-2 text-[11.5px] leading-[1.5] text-muted-foreground"
              >
                Ce que cet assistant retient — préférences, projets, contexte.
              </p>
              <div class="flex flex-wrap gap-1.5">
                {#if a.tags.length === 0}
                  <span class="text-[11px] text-muted-foreground/70">
                    Aucun élément en mémoire.
                  </span>
                {:else}
                  {#each a.tags as tag (tag)}
                    <Chip size="sm" tone="secondary">{tag}</Chip>
                  {/each}
                {/if}
              </div>
            </Card>

            <!-- Usage / examples -->
            <Card class="p-[14px_16px]">
              <div class="mb-2 flex items-center justify-between">
                <span class="text-[12.5px] font-semibold text-foreground"
                  >Activité</span
                >
                <span
                  class="font-mono text-[10.5px] text-muted-foreground"
                >
                  {a.id ? "chargé" : "non chargé"}
                </span>
              </div>
              <!-- Sparkline placeholder -->
              <div class="flex h-12 items-end gap-1">
                {#each [3, 5, 4, 7, 6, 9, 8, 11, 9, 12, 10, 14] as h, i (i)}
                  <div
                    class="flex-1 rounded-sm bg-primary/20"
                    style="height: {(h / 14) * 100}%;"
                  ></div>
                {/each}
              </div>
              <div class="mt-2 text-[10.5px] text-muted-foreground">
                Données d'usage à venir.
              </div>
              {#if a.examples.length > 0}
                <div class="mt-3 border-t border-border/40 pt-2.5">
                  <div
                    class="font-mono mb-1.5 text-[10px] font-semibold uppercase tracking-[1.2px] text-muted-foreground"
                  >
                    Exemples
                  </div>
                  <ul class="m-0 list-none space-y-1 p-0">
                    {#each a.examples.slice(0, 3) as ex (ex)}
                      <li class="text-[11.5px] text-muted-foreground">
                        — {ex}
                      </li>
                    {/each}
                  </ul>
                </div>
              {/if}
            </Card>
          </div>

          <!-- Logs link -->
          {#if a.id}
            <div class="mt-4 flex justify-end">
              <BtnSecondary onclick={() => openLogs(a.id!)}>
                {$t("agents.logs")}
              </BtnSecondary>
            </div>
          {/if}
        </div>
      {:else if $connectionStatus === "connecting"}
        <div class="flex flex-1 items-center justify-center px-8">
          <div class="text-[12.5px] text-muted-foreground">
            Chargement des assistants…
          </div>
        </div>
      {:else}
        <div class="flex flex-1 items-center justify-center px-8 py-14">
          <EmptyState
            tone="primary"
            title="Aucun assistant sélectionné"
            desc="Installez votre premier assistant pour commencer."
          >
            {#snippet icon()}
              <Bot size={22} />
            {/snippet}
            {#snippet action()}
              <BtnPrimary onclick={pickAndInstallAgent}>
                {#snippet icon()}
                  <Plus size={12} />
                {/snippet}
                Nouvel assistant
              </BtnPrimary>
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
{#if detailAgent}
  <AgentDetail
    agent={detailAgent}
    open={detailOpen}
    onclose={closeDetail}
    onlogs={openLogsFromDetail}
  />
{/if}

<InstallPackageDialog
  open={installPackageOpen}
  onclose={() => (installPackageOpen = false)}
  oninstalled={handlePkgInstalled}
/>
