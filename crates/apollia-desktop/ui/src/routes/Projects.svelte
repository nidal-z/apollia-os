<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import {
    Folder,
    Plus,
    Settings,
    MessageCircle,
    Sparkles,
    Search,
    Trash2,
  } from "lucide-svelte";
  import { addToast } from "$lib/components/ui/toast/store";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { projects } from "$lib/stores/projects";
  import type { ProjectDetail, ProjectSummary } from "$lib/types";
  import {
    PageHeader,
    SectionTitle,
    BtnPrimary,
    BtnSecondary,
    Chip,
    StatusDot,
    EmptyState as OperatorEmptyState,
    ProjectCard,
    ConversationRow,
    TaskRow,
    NewProjectDialog,
    type ProjectTemplate as DialogTemplate,
  } from "$lib/components/operator";

  // ─── State ────────────────────────────────────────────────────────────────

  let loading = $state(false);
  let mode = $state<"grid" | "split">("grid");

  // Dialog state (NewProjectDialog DS)
  let showCreateDialog = $state(false);
  let dialogStep = $state<0 | 1>(0);
  let dialogSelectedId = $state<string | undefined>(undefined);
  let dialogName = $state("");
  let dialogDescription = $state("");
  let dialogColor = $state("hsl(var(--primary))");

  // Confirm-delete
  let showDeleteConfirm = $state(false);
  let deleteProjectId = $state("");
  let deleteProjectName = $state("");
  let deleting = $state(false);

  // Selection / detail
  let selectedProjectId = $state<string | null>(null);
  let selectedProject = $state<ProjectDetail | null>(null);
  let detailLoading = $state(false);

  // Split mode tabs
  type Tab = "conversations" | "tasks" | "memory" | "settings";
  let activeTab = $state<Tab>("conversations");

  // Filter (split-list)
  let listFilter = $state("");

  // ─── Lifecycle ────────────────────────────────────────────────────────────

  void loadProjects();

  // ─── Handlers ─────────────────────────────────────────────────────────────

  async function loadProjects(): Promise<void> {
    loading = true;
    try {
      const list = await invoke<ProjectSummary[]>("list_projects");
      projects.set(list);
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      loading = false;
    }
  }

  async function selectProject(id: string): Promise<void> {
    selectedProjectId = id;
    detailLoading = true;
    mode = "split";
    activeTab = "conversations";
    try {
      selectedProject = await invoke<ProjectDetail>("get_project", { id });
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
      mode = "grid";
      selectedProjectId = null;
    } finally {
      detailLoading = false;
    }
  }

  function backToGrid() {
    mode = "grid";
    selectedProjectId = null;
    selectedProject = null;
  }

  function openCreateDialog() {
    dialogStep = 0;
    dialogSelectedId = "blank";
    dialogName = "";
    dialogDescription = "";
    dialogColor = "hsl(var(--primary))";
    showCreateDialog = true;
  }

  async function handleDialogCreate(data: {
    templateId: string;
    name: string;
    description: string;
    color: string;
  }) {
    if (!data.name.trim()) {
      addToast($t("projects.name_required") || "Nom requis", "error");
      return;
    }
    try {
      let workspacePath: string | undefined;
      try {
        workspacePath = await invoke<string>("suggest_workspace_path", {
          projectName: data.name.trim(),
        });
      } catch {
        // suggestion non critique
      }
      const created = await invoke<ProjectSummary>("create_project", {
        request: {
          name: data.name.trim(),
          description: data.description.trim() || undefined,
          workspace_path: workspacePath,
        },
      });
      addToast(
        $t("projects.created_toast", { values: { name: created.name } }),
        "success",
      );
      showCreateDialog = false;
      await loadProjects();
      void selectProject(created.id);
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }

  function requestDelete(id: string, name: string) {
    deleteProjectId = id;
    deleteProjectName = name;
    showDeleteConfirm = true;
  }

  async function confirmDelete(): Promise<void> {
    deleting = true;
    try {
      await invoke("delete_project", { id: deleteProjectId });
      addToast(
        $t("projects.deleted_toast", { values: { name: deleteProjectName } }),
        "success",
      );
      showDeleteConfirm = false;
      if (selectedProjectId === deleteProjectId) {
        backToGrid();
      }
      await loadProjects();
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      deleting = false;
    }
  }

  // ─── Derived ──────────────────────────────────────────────────────────────

  const sortedProjects = $derived.by<ProjectSummary[]>(() =>
    [...$projects].sort(
      (a, b) =>
        new Date(b.updated_at || b.created_at).getTime() -
        new Date(a.updated_at || a.created_at).getTime(),
    ),
  );

  const filteredListProjects = $derived.by<ProjectSummary[]>(() => {
    const q = listFilter.trim().toLowerCase();
    if (!q) return sortedProjects;
    return sortedProjects.filter(
      (p) =>
        p.name.toLowerCase().includes(q) ||
        (p.description ?? "").toLowerCase().includes(q),
    );
  });

  const dialogTemplates: DialogTemplate[] = [
    {
      id: "blank",
      name: $t("projects.template_blank") || "Projet vierge",
      description:
        $t("projects.template_blank_desc") ||
        "Démarrer sans configuration prédéfinie.",
      agents: [],
      blank: true,
      color: "hsl(var(--primary))",
    },
    {
      id: "launch",
      name: $t("projects.template_launch") || "Lancement produit",
      description:
        $t("projects.template_launch_desc") ||
        "Coordonner comms, jalons et tâches autour d'un go-live.",
      agents: ["Apollia libre", "Scheduler", "Writer"],
      color: "hsl(var(--primary))",
    },
    {
      id: "research",
      name: $t("projects.template_research") || "Recherche & veille",
      description:
        $t("projects.template_research_desc") ||
        "Surveiller un domaine et synthétiser les signaux.",
      agents: ["Analyst", "Writer"],
      color: "#0ea5e9",
    },
    {
      id: "campaign",
      name: $t("projects.template_campaign") || "Campagne emails",
      description:
        $t("projects.template_campaign_desc") ||
        "Composer, planifier et envoyer une campagne.",
      agents: ["Writer", "Inbox Monitor"],
      color: "#f59e0b",
    },
  ];

  // Utility: format relative time string
  function fmtRelative(iso: string | null | undefined): string {
    if (!iso) return "";
    const t0 = new Date(iso).getTime();
    if (Number.isNaN(t0)) return "";
    const diff = Date.now() - t0;
    const m = Math.round(diff / 60000);
    if (m < 1) return "à l'instant";
    if (m < 60) return `il y a ${m} min`;
    const h = Math.round(m / 60);
    if (h < 24) return `il y a ${h} h`;
    const d = Math.round(h / 24);
    return `il y a ${d} j`;
  }

  // Color rotation for cards
  const ACCENTS = [
    "hsl(var(--primary))",
    "hsl(var(--secondary))",
    "#f59e0b",
    "#10b981",
    "#0ea5e9",
    "#ec4899",
  ];
  function accentFor(id: string): string {
    let h = 0;
    for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) | 0;
    return ACCENTS[Math.abs(h) % ACCENTS.length];
  }

  // Progress ring
  function ringDash(progress: number, circumference: number): string {
    const p = Math.max(0, Math.min(100, progress));
    const filled = (p / 100) * circumference;
    return `${filled} ${circumference - filled}`;
  }

  const detailProgress = $derived(62); // placeholder until backend exposes progress
  const detailAccent = $derived(
    selectedProject ? accentFor(selectedProject.id) : "hsl(var(--primary))",
  );
</script>

<div class="flex flex-col h-full min-h-0" data-testid="projects-page">
  {#if mode === "grid"}
    <!-- ============ GRID (entry view) ============ -->
    <PageHeader
      kicker={`WORKSPACES · ${$projects.length} ${$projects.length > 1 ? "PROJETS" : "PROJET"}`}
      title={$t("projects.title")}
      subtitle={$t("projects.subtitle")}
    >
      {#snippet actions()}
        <BtnPrimary onclick={openCreateDialog}>
          {#snippet icon()}<Plus size={12} />{/snippet}
          {#snippet kbd()}<span class="font-mono text-[10px] opacity-80">⌘N</span>{/snippet}
          {$t("projects.new_project")}
        </BtnPrimary>
      {/snippet}
    </PageHeader>

    <div class="flex-1 overflow-auto px-8 pt-5 pb-8">
      {#if loading}
        <div
          class="flex items-center justify-center py-16 text-muted-foreground text-sm"
        >
          {$t("common.loading")}
        </div>
      {:else if $projects.length === 0}
        <OperatorEmptyState
          title={$t("projects.empty_title")}
          desc={$t("projects.empty_subtitle")}
        >
          {#snippet icon()}<Folder size={22} />{/snippet}
          {#snippet action()}
            <BtnPrimary onclick={openCreateDialog}>
              {#snippet icon()}<Plus size={12} />{/snippet}
              {$t("projects.new_project")}
            </BtnPrimary>
          {/snippet}
        </OperatorEmptyState>
      {:else}
        <div
          class="grid gap-3.5 grid-cols-1 md:grid-cols-2 lg:grid-cols-3"
          data-testid="projects-grid"
        >
          {#each sortedProjects as project (project.id)}
            <div
              class="relative group"
              data-testid="project-card-{project.id}"
            >
              <ProjectCard
                title={project.name}
                description={project.description ?? undefined}
                status="active"
                lastActivity={fmtRelative(project.updated_at || project.created_at)}
                color={accentFor(project.id)}
                hover
                onclick={() => selectProject(project.id)}
              />
              <button
                type="button"
                class="absolute top-2.5 right-2.5 h-6 w-6 rounded-md inline-flex items-center justify-center bg-card/70 text-muted-foreground border border-border opacity-0 group-hover:opacity-100 hover:bg-destructive/10 hover:text-danger-a11y transition-all"
                title={$t("common.delete")}
                onclick={(e) => {
                  e.stopPropagation();
                  requestDelete(project.id, project.name);
                }}
                data-testid="delete-project-{project.id}"
              >
                <Trash2 size={12} strokeWidth={1.75} />
              </button>
            </div>
          {/each}

          <!-- Create card -->
          <button
            type="button"
            onclick={openCreateDialog}
            class="rounded-xl border border-dashed border-border px-4 py-5 flex flex-col items-center justify-center cursor-pointer text-muted-foreground bg-transparent min-h-[200px] hover:border-primary/50 hover:text-primary transition-colors"
            data-testid="projects-create-card"
          >
            <Plus size={18} />
            <span class="text-[12.5px] font-medium mt-1.5">
              {$t("projects.new_project")}
            </span>
            <span class="text-[10.5px] text-muted-foreground/70 mt-0.5 font-mono">
              ⌘N
            </span>
          </button>
        </div>
      {/if}
    </div>
  {:else if selectedProject}
    <!-- ============ SPLIT (detail view) ============ -->
    <div class="flex-1 flex min-h-0">
      <!-- LEFT: project list -->
      <aside
        class="w-[240px] shrink-0 border-r border-border flex flex-col bg-background"
      >
        <div class="px-4 pt-4 pb-2.5">
          <div class="flex items-center justify-between mb-2.5">
            <div
              class="font-mono text-[10.5px] font-semibold tracking-[1.2px] uppercase text-muted-foreground"
            >
              {$t("projects.title")} · {$projects.length}
            </div>
            <button
              type="button"
              onclick={openCreateDialog}
              class="text-[11px] text-primary bg-transparent border-0 cursor-pointer inline-flex items-center gap-1 font-medium hover:underline"
            >
              <Plus size={11} />
              {$t("projects.new_project_short") || "Nouveau"}
            </button>
          </div>
          <div
            class="flex items-center gap-2 px-2 py-1.5 rounded-md bg-surface-1 border border-border"
          >
            <Search size={11} class="text-muted-foreground" />
            <input
              type="search"
              bind:value={listFilter}
              placeholder={$t("projects.search_placeholder")}
              class="flex-1 bg-transparent text-[11.5px] text-foreground placeholder:text-muted-foreground border-0 outline-none"
            />
          </div>
        </div>
        <div class="flex-1 overflow-auto px-2.5 pb-3">
          {#each filteredListProjects as p (p.id)}
            {@const accent = accentFor(p.id)}
            {@const isActive = p.id === selectedProjectId}
            <button
              type="button"
              onclick={() => selectProject(p.id)}
              class="w-full text-left flex items-start gap-2.5 px-2.5 py-2.5 rounded-lg cursor-pointer mb-0.5 border-0 transition-colors {isActive
                ? 'bg-primary/10'
                : 'bg-transparent hover:bg-muted/40'}"
            >
              <div
                class="w-1 self-stretch rounded-sm shrink-0 my-0.5"
                style="background: {accent};"
              ></div>
              <div class="flex-1 min-w-0">
                <div
                  class="text-[12.5px] truncate text-foreground"
                  style:font-weight={isActive ? 600 : 500}
                >
                  {p.name}
                </div>
                <div
                  class="text-[10.5px] text-muted-foreground mt-0.5 truncate"
                >
                  {fmtRelative(p.updated_at || p.created_at)}
                </div>
              </div>
            </button>
          {/each}
        </div>
        <div class="px-3 py-2 border-t border-border">
          <BtnSecondary onclick={backToGrid}>
            ← {$t("common.back") || "Retour"}
          </BtnSecondary>
        </div>
      </aside>

      <!-- RIGHT: detail -->
      <section class="flex-1 flex flex-col min-w-0 overflow-hidden bg-background">
        {#if detailLoading}
          <div
            class="flex-1 flex items-center justify-center text-muted-foreground text-sm"
          >
            {$t("common.loading")}
          </div>
        {:else}
          <!-- Header -->
          <div class="px-8 pt-6 pb-4 border-b border-border/60">
            <div class="flex items-start gap-3.5">
              <!-- Progress ring + folder icon -->
              <div class="relative w-12 h-12 shrink-0">
                <svg width="48" height="48" viewBox="0 0 48 48">
                  <circle
                    cx="24"
                    cy="24"
                    r="21"
                    fill="none"
                    stroke="hsl(var(--muted))"
                    stroke-width="3"
                  />
                  <circle
                    cx="24"
                    cy="24"
                    r="21"
                    fill="none"
                    stroke={detailAccent}
                    stroke-width="3"
                    stroke-linecap="round"
                    stroke-dasharray={ringDash(detailProgress, 2 * Math.PI * 21)}
                    transform="rotate(-90 24 24)"
                  />
                </svg>
                <div
                  class="absolute inset-0 flex items-center justify-center"
                >
                  <div
                    class="w-7 h-7 rounded-lg inline-flex items-center justify-center"
                    style="background: linear-gradient(135deg, hsl(var(--primary)), hsl(var(--secondary)));"
                  >
                    <Folder size={13} color="white" />
                  </div>
                </div>
              </div>

              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2 mb-1">
                  <Chip tone="primary" size="sm">
                    {#snippet icon()}
                      <StatusDot color="hsl(var(--primary))" glow />
                    {/snippet}
                    {$t("projects.status_active") || "actif"}
                  </Chip>
                  <span
                    class="text-[10.5px] text-muted-foreground font-mono"
                  >
                    {detailProgress}%
                  </span>
                  <span class="text-[10.5px] text-muted-foreground">
                    · {fmtRelative(selectedProject.updated_at)}
                  </span>
                </div>
                <h2
                  class="m-0 text-foreground"
                  style="font-size: 24px; font-weight: 600; letter-spacing: -0.4px; line-height: 1.15;"
                >
                  {selectedProject.name}
                </h2>
                {#if selectedProject.description}
                  <p
                    class="mt-1.5 mb-0 text-[12.5px] text-muted-foreground leading-[1.5] max-w-[640px]"
                  >
                    {selectedProject.description}
                  </p>
                {/if}
              </div>

              <!-- Agent stack + actions -->
              <div class="flex flex-col items-end gap-2.5">
                <div class="flex items-center gap-1.5">
                  <div class="flex">
                    {#each selectedProject.agents.slice(0, 3) as _, i}
                      <div
                        class="w-[22px] h-[22px] rounded-full inline-flex items-center justify-center border-2 border-background"
                        style="background: linear-gradient(135deg, hsl(var(--primary)), hsl(var(--secondary))); margin-left: {i ===
                        0
                          ? 0
                          : -7}px;"
                      >
                        <Sparkles size={10} color="white" />
                      </div>
                    {/each}
                  </div>
                  {#if selectedProject.agents.length > 0}
                    <span class="text-[10.5px] text-muted-foreground">
                      {selectedProject.agents.length} agent{selectedProject
                        .agents.length > 1
                        ? "s"
                        : ""}
                    </span>
                  {/if}
                </div>
                <div class="flex gap-1.5">
                  <BtnSecondary
                    onclick={() => (activeTab = "settings")}
                  >
                    {#snippet icon()}<Settings size={12} />{/snippet}
                    {$t("projects.tab_settings") || "Paramètres"}
                  </BtnSecondary>
                  <BtnPrimary>
                    {#snippet icon()}<MessageCircle size={12} />{/snippet}
                    {$t("projects.new_chat") || "Nouveau chat"}
                  </BtnPrimary>
                </div>
              </div>
            </div>
          </div>

          <!-- Tabs -->
          <div class="px-8 pt-3.5">
            <div class="flex gap-1 border-b border-border">
              {#each [
                { id: "conversations", label: $t("projects.tab_conversations") || "Conversations" },
                { id: "tasks", label: $t("projects.tab_tasks") || "Tâches" },
                { id: "memory", label: $t("projects.tab_memory") || "Mémoire" },
                { id: "settings", label: $t("projects.tab_settings") || "Paramètres" },
              ] as tab (tab.id)}
                {@const active = activeTab === tab.id}
                <button
                  type="button"
                  onclick={() => (activeTab = tab.id as Tab)}
                  class="px-3 py-2 text-[12.5px] cursor-pointer bg-transparent border-0 -mb-px transition-colors {active
                    ? 'text-foreground font-semibold border-b-2 border-primary'
                    : 'text-muted-foreground font-medium border-b-2 border-transparent hover:text-foreground'}"
                >
                  {tab.label}
                </button>
              {/each}
            </div>
          </div>

          <!-- Tab content -->
          <div class="flex-1 overflow-auto">
            {#if activeTab === "conversations"}
              <SectionTitle count={0}>
                {$t("projects.tab_conversations") || "Conversations"}
              </SectionTitle>
              <div class="px-8">
                <div class="border border-border rounded-xl overflow-hidden bg-card">
                  <ConversationRow
                    title={$t("projects.no_conversations") ||
                      "Aucune conversation pour ce projet"}
                    lastMessage={$t("projects.start_conversation_hint") ||
                      "Démarrez un nouveau chat pour cadrer le travail."}
                    timestamp="—"
                    state="default"
                  />
                </div>
              </div>
            {:else if activeTab === "tasks"}
              <SectionTitle count={0}>
                {$t("projects.tab_tasks") || "Tâches"}
              </SectionTitle>
              <div class="px-8">
                <div class="border border-border rounded-xl overflow-hidden bg-card">
                  <TaskRow
                    task={{
                      title:
                        $t("projects.no_tasks") ||
                        "Aucune tâche planifiée",
                      agent: "—",
                      status: "queued",
                    }}
                  />
                </div>
              </div>
            {:else if activeTab === "memory"}
              <SectionTitle count={selectedProject.documents.length}>
                {$t("projects.tab_memory") || "Mémoire"}
              </SectionTitle>
              <div class="px-8 pb-6">
                {#if selectedProject.documents.length === 0}
                  <OperatorEmptyState
                    title={$t("projects.empty_memory_title") ||
                      "Aucun document attaché"}
                    desc={$t("projects.empty_memory_desc") ||
                      "Glissez-déposez des fichiers ou utilisez les paramètres."}
                  >
                    {#snippet icon()}<Folder size={20} />{/snippet}
                  </OperatorEmptyState>
                {:else}
                  <ul class="divide-y divide-border border border-border rounded-xl bg-card overflow-hidden">
                    {#each selectedProject.documents as doc (doc.id)}
                      <li class="px-4 py-2.5 flex items-center gap-2.5">
                        <Folder size={13} class="text-muted-foreground" />
                        <span class="text-[12.5px] text-foreground truncate flex-1">
                          {doc.name}
                        </span>
                        <span class="text-[10.5px] text-muted-foreground font-mono">
                          {(doc.size_bytes / 1024).toFixed(1)} KB
                        </span>
                      </li>
                    {/each}
                  </ul>
                {/if}
              </div>
            {:else}
              <!-- Settings -->
              <SectionTitle>
                {$t("projects.tab_settings") || "Paramètres"}
              </SectionTitle>
              <div class="px-8 pb-6 space-y-3 max-w-[640px]">
                <div>
                  <div class="text-[11px] text-muted-foreground mb-1 font-semibold">
                    {$t("projects.field_id") || "Identifiant"}
                  </div>
                  <div class="text-[12px] font-mono text-foreground">
                    {selectedProject.id}
                  </div>
                </div>
                <div>
                  <div class="text-[11px] text-muted-foreground mb-1 font-semibold">
                    {$t("projects.field_workspace") || "Workspace"}
                  </div>
                  <div class="text-[12px] font-mono text-foreground">
                    {selectedProject.workspace_path ?? "—"}
                  </div>
                </div>
                {#if selectedProject.instructions}
                  <div>
                    <div class="text-[11px] text-muted-foreground mb-1 font-semibold">
                      {$t("projects.field_instructions") || "Instructions"}
                    </div>
                    <p
                      class="text-[12.5px] text-foreground leading-[1.5] whitespace-pre-wrap m-0"
                    >
                      {selectedProject.instructions}
                    </p>
                  </div>
                {/if}
                <div class="pt-3 border-t border-border">
                  <button
                    type="button"
                    onclick={() =>
                      requestDelete(selectedProject!.id, selectedProject!.name)}
                    class="inline-flex items-center gap-1.5 text-[12px] text-danger-a11y hover:underline bg-transparent border-0 cursor-pointer"
                    data-testid="delete-project-{selectedProject.id}"
                  >
                    <Trash2 size={12} />
                    {$t("common.delete")}
                  </button>
                </div>
              </div>
            {/if}
          </div>
        {/if}
      </section>
    </div>
  {/if}
</div>

<!-- Dialogs -->
<NewProjectDialog
  bind:open={showCreateDialog}
  bind:step={dialogStep}
  bind:selectedId={dialogSelectedId}
  bind:name={dialogName}
  bind:description={dialogDescription}
  bind:color={dialogColor}
  templates={dialogTemplates}
  onCancel={() => (showCreateDialog = false)}
  onCreate={handleDialogCreate}
/>

<ConfirmDialog
  open={showDeleteConfirm}
  title={$t("projects.delete_confirm_title")}
  message={$t("projects.delete_confirm_message", {
    values: { name: deleteProjectName },
  })}
  confirmLabel={$t("projects.delete_confirm_yes")}
  loading={deleting}
  onconfirm={confirmDelete}
  onclose={() => (showDeleteConfirm = false)}
/>
