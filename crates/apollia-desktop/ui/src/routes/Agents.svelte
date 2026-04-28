<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { get } from "svelte/store";
  import { t } from "svelte-i18n";
  import type { AgentListItem } from "$lib/types";
  import { agents } from "$lib/stores/agents";
  import { connectionStatus } from "$lib/stores/sse";
  import { navigateTo } from "$lib/stores/navigation";
  import { pendingChatSessionId } from "$lib/stores/chat";
  import { tourOpenAgentDetail } from "$lib/stores/tour";
  import {
    Bot,
    Download,
    Package,
    Plus,
    Search,
    Sparkles,
    MessageSquare,
    Settings,
    Zap,
  } from "lucide-svelte";
  import AgentLogs from "../components/agents/AgentLogs.svelte";
  import AgentDetail from "../components/agents/AgentDetail.svelte";
  import AgentPackageCard from "../components/agents/AgentPackageCard.svelte";
  import AgentPackageDetail from "../components/agents/AgentPackageDetail.svelte";
  import InstallPackageDialog from "../components/agents/InstallPackageDialog.svelte";
  import MacSandboxBanner from "../components/common/MacSandboxBanner.svelte";
  import {
    PageHeader,
    SectionTitle,
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
  let pkgDetail = $state<AgentPackageDetailView | null>(null);
  let pkgDetailOpen = $state(false);

  // ── New V3 state ─────────────────────────────────────────────────────
  let query = $state("");
  let selectedName = $state<string | null>(null);

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

  // Auto-select the first assistant when the list refreshes.
  $effect(() => {
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

  const selected = $derived(
    selectedName === null
      ? null
      : ($agents.find((a) => a.name === selectedName) ?? null),
  );

  // ── Helpers ──────────────────────────────────────────────────────────
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

  async function openPkgDetail(pkg: AgentPackageListItem) {
    try {
      pkgDetail = await getPackageDetail(pkg.name);
      pkgDetailOpen = true;
    } catch {
      pkgDetailOpen = false;
    }
  }

  async function handleUninstallPkg(pkg: AgentPackageListItem) {
    try {
      await uninstallPackage(pkg.name);
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }

  async function handlePkgInstalled(_result: InstallPackageResponse) {
    await refreshPackages();
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
          class="font-mono mb-2.5 text-[10.5px] font-semibold uppercase tracking-[1.5px] text-muted-foreground/80"
        >
          Mes assistants · {filteredAssistants.length}
        </div>
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
            {@const active = agent.name === selectedName}
            <button
              type="button"
              onclick={() => (selectedName = agent.name)}
              class="mb-0.5 flex w-full items-center gap-2.5 rounded-lg px-2.5 py-2 text-left transition-colors {active
                ? 'bg-primary/10'
                : 'hover:bg-surface-1'}"
              data-testid="agent-list-row"
              data-agent-name={agent.name}
            >
              <div
                class="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-lg"
                style="background: {isActive(agent)
                  ? 'linear-gradient(135deg, hsl(var(--primary)), hsl(var(--secondary)))'
                  : 'hsl(var(--surface-1))'}; color: {isActive(agent)
                  ? 'white'
                  : 'hsl(var(--primary))'}; border: {isActive(agent)
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
                  {#if isActive(agent)}
                    <StatusDot color={statusColor(agent)} glow size={5} />
                  {:else if isIdle(agent)}
                    <StatusDot color={statusColor(agent)} size={5} />
                  {/if}
                </div>
                <div
                  class="truncate text-[10.5px] text-muted-foreground"
                >
                  {agent.description ?? statusLabel(agent)}
                </div>
              </div>
              <Chip size="sm" tone="neutral">{kindLabel(agent)}</Chip>
            </button>
          {/each}
        {/if}
      </div>
    </aside>

    <!-- RIGHT — detail -->
    <section class="flex min-w-0 flex-1 flex-col overflow-y-auto">
      {#if selected}
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

          <!-- Packages section -->
          {#if $agentPackages.length > 0}
            <div class="mt-8">
              <SectionTitle>Packages installés · {$agentPackages.length}</SectionTitle>
              <div
                class="mt-3 grid gap-3 sm:grid-cols-1 md:grid-cols-2 lg:grid-cols-3"
              >
                {#each $agentPackages as pkg (pkg.name)}
                  <AgentPackageCard
                    {pkg}
                    ondetail={openPkgDetail}
                    onuninstall={handleUninstallPkg}
                  />
                {/each}
              </div>
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

<AgentPackageDetail
  pkg={pkgDetail}
  open={pkgDetailOpen}
  onclose={() => (pkgDetailOpen = false)}
  onuninstall={async (name) => {
    try {
      await uninstallPackage(name);
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }}
/>
