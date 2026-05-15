<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { t } from "svelte-i18n";
  import {
    Folder,
    Plus,
    Settings,
    MessageCircle,
    Sparkles,
    Search,
    Trash2,
    FolderOpen,
  } from "lucide-svelte";
  import { addToast } from "$lib/components/ui/toast/store";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { Input } from "$lib/components/ui/input";
  import { Textarea } from "$lib/components/ui/textarea";
  import { projects } from "$lib/stores/projects";
  import { pendingChatSessionId } from "$lib/stores/chat";
  import { navigateTo } from "$lib/stores/navigation";
  import type {
    ChatSessionSummary,
    CreateSessionRequest,
    MemoryEntry,
    ProjectDetail,
    ProjectSummary,
    TaskSummary,
  } from "$lib/types";
  import {
    PageHeader,
    SectionTitle,

    StatusDot,
    EmptyState as OperatorEmptyState,
    ConversationRow,
    TaskRow,
    NewProjectDialog,
    type Task as RowTask,
    type TaskStatus as RowTaskStatus,
    type ProjectTemplate as DialogTemplate,
  } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { TabBar } from "$lib/components/ui/tabs";
  import ContextProvidersTab from "../components/project/ContextProvidersTab.svelte";

  // ─── State ────────────────────────────────────────────────────────────────

  let loading = $state(false);
  // No more "grid" mode — the page always shows the sidebar + detail layout
  // (mirror Assistants), with the first project auto-selected. Grid + cards
  // were removed in the 2026-05-15 refonte UX.

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
  type Tab = "conversations" | "tasks" | "memory" | "context" | "settings";
  let activeTab = $state<Tab>("conversations");

  // Settings editable form state
  let editName = $state("");
  let editDescription = $state("");
  let editInstructions = $state("");
  let editWorkspacePath = $state("");
  let savingMetadata = $state(false);

  // Filter (split-list)
  let listFilter = $state("");

  // ─── Project-scoped lists (Conversations / Tasks / Memory tabs) ──────────

  let projectChats = $state<ChatSessionSummary[]>([]);
  let projectChatsLoading = $state(false);
  let projectTasks = $state<TaskSummary[]>([]);
  let projectTasksLoading = $state(false);
  /** Aggregated count of memory entries across `{project_id}:*` namespaces. */
  let projectMemoryNamespaces = $state<
    Array<{ namespace: string; subname: string; count: number }>
  >([]);
  let projectMemoryLoading = $state(false);

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
    activeTab = "conversations";
    try {
      selectedProject = await invoke<ProjectDetail>("get_project", { id });
      resetEditForm();
      void loadProjectChats();
      void loadProjectTasks();
      void loadProjectMemorySummary();
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
      selectedProjectId = null;
      selectedProject = null;
    } finally {
      detailLoading = false;
    }
  }

  // Auto-select the first project once the list loads. Mirrors the Agents
  // page behaviour: the sidebar is never "empty + nothing on the right" when
  // there's at least one project to show.
  $effect(() => {
    if (loading) return;
    if (selectedProjectId !== null) {
      // Drop selection if the chosen project no longer exists (e.g. deleted).
      if (!$projects.some((p) => p.id === selectedProjectId)) {
        selectedProjectId = null;
        selectedProject = null;
      } else {
        return;
      }
    }
    if ($projects.length > 0) {
      void selectProject($projects[0].id);
    }
  });

  async function loadProjectChats(): Promise<void> {
    if (!selectedProjectId) return;
    projectChatsLoading = true;
    try {
      projectChats = await invoke<ChatSessionSummary[]>("list_chats_by_project", {
        projectId: selectedProjectId,
      });
    } catch (err) {
      projectChats = [];
      console.warn("list_chats_by_project failed", err);
    } finally {
      projectChatsLoading = false;
    }
  }

  async function loadProjectTasks(): Promise<void> {
    if (!selectedProject) return;
    projectTasksLoading = true;
    try {
      // No project_id on tasks (yet) — narrow by agents attached to the project.
      // This stays consistent with how operator/agent linkage is modelled today.
      const allowedAgents = new Set(selectedProject.agents);
      const all = await invoke<TaskSummary[]>("list_tasks", { filter: null });
      projectTasks =
        allowedAgents.size === 0
          ? []
          : all.filter((t) => allowedAgents.has(t.agent_name));
    } catch (err) {
      projectTasks = [];
      console.warn("list_tasks failed", err);
    } finally {
      projectTasksLoading = false;
    }
  }

  async function loadProjectMemorySummary(): Promise<void> {
    if (!selectedProjectId) return;
    projectMemoryLoading = true;
    try {
      const all = await invoke<string[]>("list_memory_namespaces");
      const prefix = `${selectedProjectId}:`;
      const matching = all.filter((n) => n.startsWith(prefix));
      const entries = await Promise.all(
        matching.map(async (namespace) => {
          try {
            const list = await invoke<MemoryEntry[]>("list_memory_entries", {
              namespace,
            });
            return {
              namespace,
              subname: namespace.slice(prefix.length) || "—",
              count: list.length,
            };
          } catch {
            return { namespace, subname: namespace.slice(prefix.length) || "—", count: 0 };
          }
        }),
      );
      projectMemoryNamespaces = entries.sort((a, b) =>
        a.subname.localeCompare(b.subname),
      );
    } catch (err) {
      projectMemoryNamespaces = [];
      console.warn("project memory summary failed", err);
    } finally {
      projectMemoryLoading = false;
    }
  }

  function openChat(sessionId: string): void {
    pendingChatSessionId.set(sessionId);
    navigateTo("chat");
  }

  function mapTaskStatus(s: TaskSummary["status"]): RowTaskStatus {
    switch (s) {
      case "submitted":
        return "queued";
      case "working":
        return "running";
      case "completed":
        return "completed";
      case "failed":
        return "failed";
      case "input_required":
        return "awaiting_approval";
      case "canceled":
        return "cancelled";
      default:
        return "queued";
    }
  }

  function taskToRow(t: TaskSummary): RowTask {
    return {
      id: t.id,
      title: t.input_preview || "(sans titre)",
      agent: t.agent_name || t.agent_id,
      status: mapTaskStatus(t.status),
      started: fmtRelative(t.created_at),
    };
  }

  /** Reload the currently-selected project's detail. Used after mutations. */
  async function reloadDetail(): Promise<void> {
    if (!selectedProjectId) return;
    try {
      selectedProject = await invoke<ProjectDetail>("get_project", {
        id: selectedProjectId,
      });
      resetEditForm();
      void loadProjectChats();
      void loadProjectTasks();
      void loadProjectMemorySummary();
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }

  function resetEditForm(): void {
    editName = selectedProject?.name ?? "";
    editDescription = selectedProject?.description ?? "";
    editInstructions = selectedProject?.instructions ?? "";
    editWorkspacePath = selectedProject?.workspace_path ?? "";
  }

  async function pickWorkspaceDir(): Promise<void> {
    try {
      const selected = await openDialog({ multiple: false, directory: true });
      if (typeof selected === "string") {
        editWorkspacePath = selected;
      }
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }

  async function saveMetadata(): Promise<void> {
    if (!selectedProject) return;
    savingMetadata = true;
    try {
      const trimmedName = editName.trim();
      const trimmedDesc = editDescription.trim();
      const trimmedInstr = editInstructions.trim();
      const trimmedWs = editWorkspacePath.trim();
      // Build a partial patch. Use `null` to explicitly clear an optional
      // field, omit it entirely if unchanged. The Rust side accepts
      // `Option<Option<String>>` so missing = no-op, null = clear.
      const request: Record<string, unknown> = {};
      if (trimmedName && trimmedName !== selectedProject.name) {
        request.name = trimmedName;
      }
      const currentDesc = selectedProject.description ?? "";
      if (trimmedDesc !== currentDesc) {
        request.description = trimmedDesc || null;
      }
      const currentInstr = selectedProject.instructions ?? "";
      if (trimmedInstr !== currentInstr) {
        request.instructions = trimmedInstr || null;
      }
      const currentWs = selectedProject.workspace_path ?? "";
      if (trimmedWs !== currentWs) {
        request.workspace_path = trimmedWs || null;
      }
      if (Object.keys(request).length === 0) {
        savingMetadata = false;
        return;
      }
      selectedProject = await invoke<ProjectDetail>("update_project", {
        id: selectedProject.id,
        request,
      });
      resetEditForm();
      addToast($t("projects.settings_saved_toast"), "success");
      await loadProjects();
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      savingMetadata = false;
    }
  }

  async function startNewChat(): Promise<void> {
    if (!selectedProject) return;
    const request: CreateSessionRequest = {
      mode: "libre",
      project_id: selectedProject.id,
    };
    try {
      const session = await invoke<ChatSessionSummary>(
        "create_chat_session",
        { request },
      );
      pendingChatSessionId.set(session.id);
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      navigateTo("chat");
    }
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
      if (data.templateId === "developer") {
        await applyDeveloperTemplate(created.id);
      }
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

  async function applyDeveloperTemplate(projectId: string): Promise<void> {
    // Pre-configure context providers oriented around code workspaces:
    // git status/diff, file tree snapshot, and APOLLIA.md project rules.
    const seeds: Array<{
      provider_type: string;
      name: string;
      config_json: string;
      priority: number;
    }> = [
      {
        provider_type: "git",
        name: $t("projects.template_developer_provider_git"),
        config_json: "{}",
        priority: 10,
      },
      {
        provider_type: "tree",
        name: $t("projects.template_developer_provider_tree"),
        config_json: "{}",
        priority: 20,
      },
      {
        provider_type: "rules",
        name: $t("projects.template_developer_provider_rules"),
        config_json: "{}",
        priority: 30,
      },
    ];
    for (const s of seeds) {
      try {
        await invoke("set_project_provider", {
          providerId: null,
          projectId,
          providerType: s.provider_type,
          name: s.name,
          configJson: s.config_json,
          path: null,
          enabled: true,
          priority: s.priority,
        });
      } catch (err: unknown) {
        // Non-fatal: project is already created, surface a soft warning.
        addToast(
          $t("projects.template_developer_provider_failed", {
            values: { name: s.name, err: err instanceof Error ? err.message : String(err) },
          }),
          "info",
        );
      }
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
        // Drop the deleted project from the right pane — the auto-select
        // effect will pick the next one if any remain.
        selectedProjectId = null;
        selectedProject = null;
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

  const settingsDirty = $derived.by<boolean>(() => {
    if (!selectedProject) return false;
    return (
      editName.trim() !== selectedProject.name ||
      editDescription.trim() !== (selectedProject.description ?? "") ||
      editInstructions.trim() !== (selectedProject.instructions ?? "") ||
      editWorkspacePath.trim() !== (selectedProject.workspace_path ?? "")
    );
  });

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
      name: $t("projects.template_blank"),
      description: $t("projects.template_blank_desc"),
      agents: [],
      blank: true,
      color: "hsl(var(--primary))",
    },
    {
      id: "developer",
      name: $t("projects.template_developer"),
      description: $t("projects.template_developer_desc"),
      agents: [],
      color: "#0ea5e9",
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

</script>

<div class="flex flex-col h-full min-h-0" data-testid="projects-page">
  {#if !loading && $projects.length === 0}
    <!-- Empty state — no projects yet, full-width invitation. -->
    <PageHeader
      kicker={$t("projects.kicker", {
        values: {
          count: 0,
          label: $t("projects.kicker_singular"),
        },
      })}
      title={$t("projects.title")}
      subtitle={$t("projects.subtitle")}
    />
    <div class="flex-1 overflow-auto px-8 pt-5 pb-8">
      <OperatorEmptyState
        title={$t("projects.empty_title")}
        desc={$t("projects.empty_subtitle")}
      >
        {#snippet icon()}<Folder size={22} />{/snippet}
        {#snippet action()}
          <Button variant="primary-solid" size="sm" onclick={openCreateDialog}>
            {#snippet icon()}<Plus size={12} />{/snippet}
            {$t("projects.new_project")}
          </Button>
        {/snippet}
      </OperatorEmptyState>
    </div>
  {:else}
    <!-- ============ SIDEBAR + DETAIL (mirror Assistants) ============ -->
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
            <Button variant="ghost" size="sm"
              type="button"
              onclick={openCreateDialog}
              class="text-[11px] text-primary bg-transparent border-0 cursor-pointer inline-flex items-center gap-1 font-medium hover:underline"
            >
              <Plus size={11} />
              {$t("projects.new_project_short") || "Nouveau"}
            </Button>
          </div>
          <div
            class="flex items-center gap-2 px-2 py-1.5 rounded-md bg-surface-1 border border-border"
          >
            <Search size={11} class="text-muted-foreground" />
            <Input
              type="search"
              unstyled
              bind:value={listFilter}
              placeholder={$t("projects.search_placeholder")}
              class="flex-1 text-[11.5px] text-foreground"
             />
          </div>
        </div>
        <div class="flex-1 overflow-auto px-2.5 pb-3">
          {#each filteredListProjects as p (p.id)}
            {@const accent = accentFor(p.id)}
            {@const isActive = p.id === selectedProjectId}
            <Button variant="ghost" size="auto"
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
            </Button>
          {/each}
        </div>
      </aside>

      <!-- RIGHT: detail -->
      <section class="flex-1 flex flex-col min-w-0 overflow-hidden bg-background">
        {#if detailLoading || !selectedProject}
          <div
            class="flex-1 flex items-center justify-center text-muted-foreground text-sm"
          >
            {$t("common.loading")}
          </div>
        {:else}
          <!-- Header -->
          <div class="px-8 pt-6 pb-4 border-b border-border/60">
            <div class="flex items-start gap-3.5">
              <!-- Folder icon -->
              <div
                class="w-12 h-12 shrink-0 rounded-lg inline-flex items-center justify-center"
                style="background: linear-gradient(135deg, hsl(var(--primary)), hsl(var(--secondary)));"
              >
                <Folder size={18} color="white" />
              </div>

              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2 mb-1">
                  <Badge variant="primary" size="sm">
                    {#snippet icon()}
                      <StatusDot color="hsl(var(--primary))" glow />
                    {/snippet}
                    {$t("projects.status_active") || "actif"}
                  </Badge>
                  <span class="text-[10.5px] text-muted-foreground">
                    {fmtRelative(selectedProject.updated_at)}
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
                  <Button variant="outline" size="sm"
                    onclick={() => (activeTab = "settings")}
                  >
                    {#snippet icon()}<Settings size={12} />{/snippet}
                    {$t("projects.tab_settings") || "Paramètres"}
                  </Button>
                  <Button variant="primary-solid" size="sm" onclick={startNewChat}>
                    {#snippet icon()}<MessageCircle size={12} />{/snippet}
                    {$t("projects.new_chat") || "Nouveau chat"}
                  </Button>
                </div>
              </div>
            </div>
          </div>

          <!-- Tabs -->
          <div class="px-8 pt-3.5">
            <TabBar
              variant="underline"
              testidPrefix="project"
              activeTab={activeTab}
              items={[
                { key: "conversations", label: $t("projects.tab_conversations") || "Conversations" },
                { key: "tasks", label: $t("projects.tab_tasks") || "Tâches" },
                { key: "memory", label: $t("projects.tab_memory") || "Mémoire" },
                { key: "context", label: $t("projects.tab_context") || "Contexte" },
                { key: "settings", label: $t("projects.tab_settings") || "Paramètres" },
              ]}
              ontabchange={(key) => (activeTab = key as Tab)}
            />
          </div>

          <!-- Tab content -->
          <div class="flex-1 overflow-auto">
            {#if activeTab === "conversations"}
              <SectionTitle count={projectChats.length}>
                {$t("projects.tab_conversations") || "Conversations"}
              </SectionTitle>
              <div class="px-8 pb-6">
                {#if projectChatsLoading}
                  <div class="border border-border rounded-xl bg-card px-4 py-3 text-[12px] text-muted-foreground">
                    {$t("projects.loading") || "Chargement…"}
                  </div>
                {:else if projectChats.length === 0}
                  <OperatorEmptyState
                    title={$t("projects.no_conversations") ||
                      "Aucune conversation liée"}
                    desc={$t("projects.start_conversation_hint") ||
                      "Démarrez un nouveau chat pour cadrer le travail."}
                  >
                    {#snippet icon()}<FolderOpen size={20} />{/snippet}
                  </OperatorEmptyState>
                {:else}
                  <div class="border border-border rounded-xl overflow-hidden bg-card">
                    {#each projectChats as chat (chat.id)}
                      <ConversationRow
                        title={chat.title ??
                          chat.agent_name ??
                          ($t("chat.untitled_session") || "Conversation")}
                        lastMessage={chat.last_message_preview ?? undefined}
                        timestamp={fmtRelative(chat.created_at)}
                        state={chat.status === "closed" ? "closed" : "default"}
                        live={chat.status === "processing"}
                        onclick={() => openChat(chat.id)}
                      />
                    {/each}
                  </div>
                {/if}
              </div>
            {:else if activeTab === "tasks"}
              <SectionTitle count={projectTasks.length}>
                {$t("projects.tab_tasks") || "Tâches"}
              </SectionTitle>
              <div class="px-8 pb-6">
                {#if projectTasksLoading}
                  <div class="border border-border rounded-xl bg-card px-4 py-3 text-[12px] text-muted-foreground">
                    {$t("projects.loading") || "Chargement…"}
                  </div>
                {:else if selectedProject.agents.length === 0}
                  <OperatorEmptyState
                    title={$t("projects.no_tasks") ||
                      "Aucune tâche pour ce projet"}
                    desc={$t("projects.tasks_need_agents") ||
                      "Attachez au moins un agent au projet dans Paramètres pour voir ses tâches ici."}
                  >
                    {#snippet icon()}<Sparkles size={20} />{/snippet}
                  </OperatorEmptyState>
                {:else if projectTasks.length === 0}
                  <OperatorEmptyState
                    title={$t("projects.no_tasks") ||
                      "Aucune tâche pour ce projet"}
                    desc={$t("projects.no_tasks_desc") ||
                      "Les tâches déclenchées par les agents de ce projet apparaîtront ici."}
                  >
                    {#snippet icon()}<Sparkles size={20} />{/snippet}
                  </OperatorEmptyState>
                {:else}
                  <div class="border border-border rounded-xl overflow-hidden bg-card divide-y divide-border">
                    {#each projectTasks as task (task.id)}
                      <TaskRow task={taskToRow(task)} />
                    {/each}
                  </div>
                {/if}
              </div>
            {:else if activeTab === "memory"}
              <!-- 1) Documents joints (source statique) -->
              <SectionTitle count={selectedProject.documents.length}>
                {$t("projects.memory_documents") || "Documents joints"}
              </SectionTitle>
              <div class="px-8 pb-4">
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

              <!-- 2) Mémoire scopée projet (namespaces `{project_id}:*`) -->
              <SectionTitle count={projectMemoryNamespaces.length}>
                {$t("projects.memory_namespaces") || "Mémoire scopée"}
              </SectionTitle>
              <div class="px-8 pb-6">
                {#if projectMemoryLoading}
                  <div class="border border-border rounded-xl bg-card px-4 py-3 text-[12px] text-muted-foreground">
                    {$t("projects.loading") || "Chargement…"}
                  </div>
                {:else if projectMemoryNamespaces.length === 0}
                  <OperatorEmptyState
                    title={$t("projects.no_project_memory") ||
                      "Aucune entrée mémoire pour ce projet"}
                    desc={$t("projects.no_project_memory_desc") ||
                      "Les agents écriront leurs entrées dans des namespaces préfixés par l'ID du projet."}
                  >
                    {#snippet icon()}<FolderOpen size={20} />{/snippet}
                  </OperatorEmptyState>
                {:else}
                  <ul class="divide-y divide-border border border-border rounded-xl bg-card overflow-hidden">
                    {#each projectMemoryNamespaces as ns (ns.namespace)}
                      <li class="px-4 py-2.5 flex items-center gap-2.5">
                        <FolderOpen size={13} class="text-muted-foreground" />
                        <span class="text-[12.5px] text-foreground truncate flex-1">
                          {ns.subname}
                        </span>
                        <span class="text-[10.5px] text-muted-foreground font-mono">
                          {ns.namespace}
                        </span>
                        <span class="text-[10.5px] text-muted-foreground font-medium">
                          {ns.count} {ns.count > 1 ? "entrées" : "entrée"}
                        </span>
                      </li>
                    {/each}
                  </ul>
                {/if}
              </div>
            {:else if activeTab === "context"}
              <ContextProvidersTab
                project={selectedProject}
                onUpdated={reloadDetail}
              />
            {:else}
              <!-- Settings — editable metadata form -->
              <SectionTitle>
                {$t("projects.tab_settings") || "Paramètres"}
              </SectionTitle>
              <div class="px-8 pb-6 space-y-4 max-w-[640px]">
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
                    {$t("projects.settings_field_name")}
                  </div>
                  <Input
                    bind:value={editName}
                    placeholder={$t("projects.field_name")}
                    class="bg-card"
                    data-testid="settings-name-input"
                  />
                </div>

                <div>
                  <div class="text-[11px] text-muted-foreground mb-1 font-semibold">
                    {$t("projects.settings_field_description")}
                  </div>
                  <Textarea
                    bind:value={editDescription}
                    rows={3}
                    class="bg-card"
                    data-testid="settings-description-input"
                  />
                </div>

                <div>
                  <div class="text-[11px] text-muted-foreground mb-1 font-semibold">
                    {$t("projects.settings_field_instructions")}
                  </div>
                  <Textarea
                    bind:value={editInstructions}
                    rows={5}
                    placeholder={$t("projects.settings_field_instructions_placeholder")}
                    class="bg-card"
                    data-testid="settings-instructions-input"
                  />
                </div>

                <div>
                  <div class="text-[11px] text-muted-foreground mb-1 font-semibold">
                    {$t("projects.field_workspace") || "Workspace"}
                  </div>
                  <div class="flex gap-2">
                    <Input
                      bind:value={editWorkspacePath}
                      placeholder="/path/to/workspace"
                      class="flex-1 bg-card"
                      data-testid="settings-workspace-input"
                    />
                    <Button variant="outline" size="sm" onclick={pickWorkspaceDir}>
                      {#snippet icon()}<FolderOpen size={12} />{/snippet}
                      {$t("projects.settings_field_workspace_pick")}
                    </Button>
                  </div>
                </div>

                <div class="flex items-center gap-2 pt-1">
                  <Button variant="primary-solid" size="sm"
                    onclick={saveMetadata}
                    disabled={!settingsDirty || savingMetadata}
                  >
                    {savingMetadata ? $t("common.loading") : $t("common.save")}
                  </Button>
                  <Button variant="outline" size="sm"
                    onclick={resetEditForm}
                    disabled={!settingsDirty || savingMetadata}
                  >
                    {$t("common.cancel")}
                  </Button>
                </div>

                <div class="pt-3 border-t border-border">
                  <Button variant="ghost" size="sm"
                    type="button"
                    onclick={() =>
                      requestDelete(selectedProject!.id, selectedProject!.name)}
                    class="inline-flex items-center gap-1.5 text-[12px] text-danger-a11y hover:underline bg-transparent border-0 cursor-pointer"
                    data-testid="delete-project-{selectedProject.id}"
                  >
                    <Trash2 size={12} />
                    {$t("common.delete")}
                  </Button>
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
