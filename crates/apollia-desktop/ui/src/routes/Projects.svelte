<script lang="ts">
  import { flip } from "svelte/animate";
  import { fly } from "svelte/transition";
  import { t } from "svelte-i18n";
  import { get } from "svelte/store";
  import { Folder, Plus, Search } from "lucide-svelte";
  import { addToast } from "$lib/components/ui/toast/store";
  import { reportError } from "$lib/errors/reportError";
  import type { HumanizedError } from "$lib/errors/humanize";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { Input } from "$lib/components/ui/input";
  import { Button } from "$lib/components/ui/button";
  import { projects } from "$lib/stores/projects";
  import { pendingChatSessionId } from "$lib/stores/chat";
  import { navigateTo } from "$lib/stores/navigation";
  import {
    listProjects,
    getProject,
    createProject,
    deleteProject,
    suggestWorkspacePath,
    setProjectProvider,
    listChatsByProject,
    listTasks,
    listMemoryNamespaces,
    listMemoryEntries,
    createChatSession,
  } from "$lib/ipc/projects";
  import { rowIn, rowOut, listFlip } from "$lib/design/listMotion";
  import { contentIn, contentOut } from "$lib/design/routeTransition";
  import { listNavigation } from "$lib/components/operator/listNavigation";
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
    SplitLayout,
    SidebarHeader,
    SkeletonList,
    EmptyState as OperatorEmptyState,
    NewProjectDialog,
    type Task as RowTask,
    type TaskStatus as RowTaskStatus,
    type ProjectTemplate as DialogTemplate,
  } from "$lib/components/operator";
  import ProjectSidebarRow from "../components/projects/ProjectSidebarRow.svelte";
  import ProjectsLoadError from "../components/projects/ProjectsLoadError.svelte";
  import ProjectDetailPane, {
    type ProjectTab,
  } from "../components/projects/ProjectDetailPane.svelte";

  // ─── State ────────────────────────────────────────────────────────────────

  let loading = $state(false);
  let loadError = $state<HumanizedError | null>(null);

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
  let activeTab = $state<ProjectTab>("conversations");

  // Filter (split-list)
  let listFilter = $state("");

  // Project-scoped lists (Conversations / Tasks / Memory tabs)
  let projectChats = $state<ChatSessionSummary[]>([]);
  let projectChatsLoading = $state(false);
  let projectTasks = $state<TaskSummary[]>([]);
  let projectTasksLoading = $state(false);
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
      const list = await listProjects();
      projects.set(list);
      loadError = null;
    } catch (err: unknown) {
      loadError = reportError(err, { surface: "inline" });
    } finally {
      loading = false;
    }
  }

  async function selectProject(id: string): Promise<void> {
    selectedProjectId = id;
    detailLoading = true;
    activeTab = "conversations";
    try {
      selectedProject = await getProject(id);
      void loadProjectChats();
      void loadProjectTasks();
      void loadProjectMemorySummary();
    } catch (err: unknown) {
      reportError(err, { surface: "toast" });
      selectedProjectId = null;
      selectedProject = null;
    } finally {
      detailLoading = false;
    }
  }

  // Auto-select the first project once the list loads (mirror Agents): the
  // right pane is never "empty + nothing selected" while a project exists.
  $effect(() => {
    if (loading) return;
    if (selectedProjectId !== null) {
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
      projectChats = await listChatsByProject(selectedProjectId);
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
      const allowedAgents = new Set(selectedProject.agents);
      const all = await listTasks(null);
      projectTasks =
        allowedAgents.size === 0
          ? []
          : all.filter((task) => allowedAgents.has(task.agent_name));
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
      const all = await listMemoryNamespaces();
      const prefix = `${selectedProjectId}:`;
      const matching = all.filter((n) => n.startsWith(prefix));
      const entries = await Promise.all(
        matching.map(async (namespace) => {
          try {
            const list: MemoryEntry[] = await listMemoryEntries(namespace);
            return {
              namespace,
              subname: namespace.slice(prefix.length) || "-",
              count: list.length,
            };
          } catch {
            return { namespace, subname: namespace.slice(prefix.length) || "-", count: 0 };
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

  function taskToRow(task: TaskSummary): RowTask {
    return {
      id: task.id,
      title: task.input_preview || "(sans titre)",
      agent: task.agent_name || task.agent_id,
      status: mapTaskStatus(task.status),
      started: fmtRelative(task.created_at),
    };
  }

  /** Reload the current project's detail + scoped lists after a context change. */
  async function reloadDetail(): Promise<void> {
    if (!selectedProjectId) return;
    try {
      selectedProject = await getProject(selectedProjectId);
      void loadProjectChats();
      void loadProjectTasks();
      void loadProjectMemorySummary();
    } catch (err) {
      reportError(err, { surface: "toast" });
    }
  }

  /** Settings tab saved: adopt the fresh detail and refresh the sidebar list. */
  function onProjectSaved(updated: ProjectDetail): void {
    selectedProject = updated;
    void loadProjects();
  }

  /** Context-menu / kebab "rename": open the project on its Settings tab. */
  async function renameProject(project: ProjectSummary): Promise<void> {
    await selectProject(project.id);
    activeTab = "settings";
  }

  async function startNewChat(): Promise<void> {
    if (!selectedProject) return;
    const request: CreateSessionRequest = {
      mode: "libre",
      project_id: selectedProject.id,
    };
    try {
      const session = await createChatSession(request);
      pendingChatSessionId.set(session.id);
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      navigateTo("chat");
    }
  }

  function openCreateDialog(): void {
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
  }): Promise<void> {
    if (!data.name.trim()) {
      addToast($t("projects.name_required"), "error");
      return;
    }
    try {
      let workspacePath: string | undefined;
      try {
        workspacePath = await suggestWorkspacePath(data.name.trim());
      } catch {
        // suggestion non critique
      }
      const created = await createProject({
        name: data.name.trim(),
        description: data.description.trim() || undefined,
        workspace_path: workspacePath,
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
      reportError(err, { surface: "toast" });
    }
  }

  async function applyDeveloperTemplate(projectId: string): Promise<void> {
    const seeds = [
      { provider_type: "git", name: $t("projects.template_developer_provider_git"), priority: 10 },
      { provider_type: "tree", name: $t("projects.template_developer_provider_tree"), priority: 20 },
      { provider_type: "rules", name: $t("projects.template_developer_provider_rules"), priority: 30 },
    ];
    for (const s of seeds) {
      try {
        await setProjectProvider({
          providerId: null,
          projectId,
          providerType: s.provider_type,
          name: s.name,
          configJson: "{}",
          path: null,
          enabled: true,
          priority: s.priority,
        });
      } catch (err: unknown) {
        addToast(
          $t("projects.template_developer_provider_failed", {
            values: { name: s.name, err: err instanceof Error ? err.message : String(err) },
          }),
          "info",
        );
      }
    }
  }

  function requestDelete(id: string, name: string): void {
    deleteProjectId = id;
    deleteProjectName = name;
    showDeleteConfirm = true;
  }

  async function confirmDelete(): Promise<void> {
    deleting = true;
    try {
      await deleteProject(deleteProjectId);
      addToast(
        $t("projects.deleted_toast", { values: { name: deleteProjectName } }),
        "success",
      );
      showDeleteConfirm = false;
      if (selectedProjectId === deleteProjectId) {
        selectedProjectId = null;
        selectedProject = null;
      }
      await loadProjects();
    } catch (err: unknown) {
      reportError(err, { surface: "toast" });
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

  const taskRows = $derived<RowTask[]>(projectTasks.map(taskToRow));

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
      color: "hsl(var(--info))",
    },
  ];

  // Relative time. Kept as the pre-existing helper (no i18n keys for these
  // buckets yet); mirrors the wording used across the operator surfaces.
  function fmtRelative(iso: string | null | undefined): string {
    if (!iso) return "";
    const t0 = new Date(iso).getTime();
    if (Number.isNaN(t0)) return "";
    const diff = Date.now() - t0;
    const m = Math.round(diff / 60000);
    if (m < 1) return get(t)("common.relative.just_now");
    if (m < 60) return get(t)("common.relative.minutes", { values: { n: m } });
    const h = Math.round(m / 60);
    if (h < 24) return get(t)("common.relative.hours", { values: { n: h } });
    const d = Math.round(h / 24);
    return get(t)("common.relative.days", { values: { n: d } });
  }

  // Card accent rotation, token-only (was a hardcoded hex array). Hashing an
  // id onto the semantic palette keeps both themes correct.
  const ACCENT_TOKENS = [
    "hsl(var(--primary))",
    "hsl(var(--secondary))",
    "hsl(var(--success))",
    "hsl(var(--warning))",
    "hsl(var(--info))",
  ];
  function accentFor(id: string): string {
    let h = 0;
    for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) | 0;
    return ACCENT_TOKENS[Math.abs(h) % ACCENT_TOKENS.length];
  }

  const hasProjects = $derived($projects.length > 0);
</script>

<div class="flex flex-col h-full min-h-0" data-testid="projects-page">
  {#if loadError && !loading && !hasProjects}
    <!-- Load failure, humanized with the raw detail behind a disclosure. -->
    <PageHeader
      kicker={$t("projects.kicker", {
        values: { count: 0, label: $t("projects.kicker_singular") },
      })}
      title={$t("projects.title")}
      subtitle={$t("projects.subtitle")}
    />
    <div class="flex-1 overflow-auto px-8 pt-5 pb-8">
      <ProjectsLoadError error={loadError} onRetry={loadProjects} />
    </div>
  {:else if !loading && !hasProjects}
    <!-- No projects yet, full-width invitation. -->
    <PageHeader
      kicker={$t("projects.kicker", {
        values: { count: 0, label: $t("projects.kicker_singular") },
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
          <Button
            variant="primary-solid"
            size="sm"
            onclick={openCreateDialog}
            data-testid="projects-new-project-btn"
          >
            {#snippet icon()}<Plus size={12} />{/snippet}
            {$t("projects.new_project")}
          </Button>
        {/snippet}
      </OperatorEmptyState>
    </div>
  {:else}
    <!-- ============ SIDEBAR + DETAIL (mirror Assistants) ============ -->
    <SplitLayout>
      {#snippet sidebar()}
        <SidebarHeader title={$t("projects.title")} count={$projects.length}>
          {#snippet actions()}
            <Button
              variant="ghost"
              size="sm"
              type="button"
              onclick={openCreateDialog}
              data-testid="projects-new-project-btn"
            >
              {#snippet icon()}<Plus size={12} />{/snippet}
              {$t("projects.new_project_short")}
            </Button>
          {/snippet}
          {#snippet search()}
            <div
              class="flex items-center gap-2 px-2 py-1.5 rounded-md bg-surface-1 border border-border transition-colors focus-within:border-ring/60"
            >
              <Search size={12} class="text-muted-foreground" />
              <Input
                type="search"
                unstyled
                bind:value={listFilter}
                placeholder={$t("projects.search_placeholder")}
                class="flex-1 text-body-xs text-foreground"
                data-testid="projects-sidebar-search"
              />
            </div>
          {/snippet}
        </SidebarHeader>

        {#if loading && !hasProjects}
          <div class="flex-1 overflow-auto px-2.5 pb-3 pt-2">
            <SkeletonList count={6} avatar={false} rowClass="px-2 py-1.5" />
          </div>
        {:else if filteredListProjects.length === 0}
          <div class="flex-1 overflow-auto px-4 pb-3">
            <OperatorEmptyState
              tone="neutral"
              title={$t("projects.no_match")}
              desc={$t("projects.no_match_hint")}
            >
              {#snippet icon()}<Search size={20} />{/snippet}
              {#snippet action()}
                <Button variant="outline" size="sm" onclick={() => (listFilter = "")}>
                  {$t("common.clear")}
                </Button>
              {/snippet}
            </OperatorEmptyState>
          </div>
        {:else}
          <div
            class="flex-1 overflow-auto px-2.5 pb-3"
            use:listNavigation={{ idPrefix: "project-nav" }}
          >
            {#each filteredListProjects as p (p.id)}
              <div animate:flip={listFlip()} in:fly={rowIn()} out:fly={rowOut()}>
                <ProjectSidebarRow
                  project={p}
                  accent={accentFor(p.id)}
                  active={p.id === selectedProjectId}
                  live={p.id === selectedProjectId}
                  relativeTime={fmtRelative(p.updated_at || p.created_at)}
                  onSelect={selectProject}
                  onRename={renameProject}
                  onDelete={requestDelete}
                />
              </div>
            {/each}
          </div>
        {/if}
      {/snippet}

      <!-- RIGHT: detail -->
      {#if detailLoading || !selectedProject}
        <div class="flex-1 overflow-auto px-8 pt-6 space-y-4">
          <SkeletonList count={1} rowClass="h-10" />
          <SkeletonList count={4} avatar={false} rowClass="px-4 py-2" />
        </div>
      {:else}
        {#key selectedProjectId}
          <div
            class="flex flex-col flex-1 min-h-0"
            in:fly={contentIn()}
            out:fly={contentOut()}
          >
            <ProjectDetailPane
              project={selectedProject}
              {activeTab}
              onTabChange={(tab) => (activeTab = tab)}
              onNewChat={startNewChat}
              onContextUpdated={reloadDetail}
              onAgentsChanged={reloadDetail}
              onDocumentsChanged={reloadDetail}
              {onProjectSaved}
              onRequestDelete={requestDelete}
              chats={projectChats}
              chatsLoading={projectChatsLoading}
              onOpenChat={openChat}
              formatRelative={fmtRelative}
              {taskRows}
              tasksLoading={projectTasksLoading}
              namespaces={projectMemoryNamespaces}
              memoryLoading={projectMemoryLoading}
            />
          </div>
        {/key}
      {/if}
    </SplitLayout>
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
  data-testid="project-delete-confirm"
/>
