<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { FolderOpen, Plus, Trash2, Home, Search } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Breadcrumbs, type BreadcrumbItem } from "$lib/components/ui/breadcrumbs";
  import { addToast } from "$lib/components/ui/toast/store";
  import { projects } from "$lib/stores/projects";
  import type { ProjectDetail, ProjectSummary } from "$lib/types";
  import EmptyState from "../components/common/EmptyState.svelte";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import CreateProjectDialog from "../components/projects/CreateProjectDialog.svelte";
  import ProjectDetailSheet from "../components/projects/ProjectDetail.svelte";
  import ProjectsViewToggle, { type ProjectsView } from "../components/projects/ProjectsViewToggle.svelte";

  const VIEW_STORAGE_KEY = "projects.view";
  const SORT_STORAGE_KEY = "projects.sort";

  type SortMode = "recent" | "oldest";

  // ─── State ────────────────────────────────────────────────────────────────

  let loading = $state(false);
  let showCreateDialog = $state(false);
  let showDeleteConfirm = $state(false);
  let deleteProjectId = $state("");
  let deleteProjectName = $state("");
  let deleting = $state(false);

  let selectedProjectId = $state<string | null>(null);
  let selectedProject = $state<ProjectDetail | null>(null);
  let detailOpen = $state(false);
  let detailLoading = $state(false);

  let view = $state<ProjectsView>(loadPreference(VIEW_STORAGE_KEY, "list") as ProjectsView);
  let sort = $state<SortMode>(loadPreference(SORT_STORAGE_KEY, "recent") as SortMode);
  let searchQuery = $state("");

  function loadPreference(key: string, fallback: string): string {
    try {
      const stored = localStorage.getItem(key);
      if (stored) return stored;
    } catch {
      // localStorage unavailable
    }
    return fallback;
  }

  function persistPreference(key: string, value: string): void {
    try {
      localStorage.setItem(key, value);
    } catch {
      // ignore
    }
  }

  // ─── Lifecycle ────────────────────────────────────────────────────────────

  void loadProjects();

  // ─── Handlers ────────────────────────────────────────────────────────────

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

  async function openDetail(id: string): Promise<void> {
    selectedProjectId = id;
    detailOpen = true;
    detailLoading = true;
    try {
      selectedProject = await invoke<ProjectDetail>("get_project", { id });
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
      detailOpen = false;
    } finally {
      detailLoading = false;
    }
  }

  function handleDeleteRequest(id: string, name: string) {
    deleteProjectId = id;
    deleteProjectName = name;
    showDeleteConfirm = true;
  }

  async function handleDeleteConfirm(): Promise<void> {
    deleting = true;
    try {
      await invoke("delete_project", { id: deleteProjectId });
      addToast($t("projects.deleted_toast", { values: { name: deleteProjectName } }), "success");
      showDeleteConfirm = false;
      if (selectedProjectId === deleteProjectId) {
        detailOpen = false;
        selectedProject = null;
      }
      void loadProjects();
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      deleting = false;
    }
  }

  function handleProjectCreated(id: string, name: string) {
    addToast($t("projects.created_toast", { values: { name } }), "success");
    showCreateDialog = false;
    void loadProjects();
    void openDetail(id);
  }

  function handleProjectUpdated() {
    if (selectedProjectId) {
      void openDetail(selectedProjectId);
    }
    void loadProjects();
  }

  function handleViewChange(next: ProjectsView) {
    view = next;
    persistPreference(VIEW_STORAGE_KEY, next);
  }

  function handleSortChange(event: Event) {
    const next = (event.target as HTMLSelectElement).value as SortMode;
    sort = next;
    persistPreference(SORT_STORAGE_KEY, next);
  }

  // ─── Derived ─────────────────────────────────────────────────────────────

  const filteredProjects = $derived.by<ProjectSummary[]>(() => {
    let result = $projects;
    const q = searchQuery.trim().toLowerCase();
    if (q) {
      result = result.filter(
        (p) =>
          p.name.toLowerCase().includes(q) ||
          (p.description ?? "").toLowerCase().includes(q),
      );
    }
    result = [...result].sort((a, b) => {
      const ta = new Date(a.updated_at || a.created_at).getTime();
      const tb = new Date(b.updated_at || b.created_at).getTime();
      return sort === "recent" ? tb - ta : ta - tb;
    });
    return result;
  });

  const breadcrumbItems = $derived<BreadcrumbItem[]>(
    selectedProject && detailOpen
      ? [
          { label: $t("nav.home"), icon: Home },
          { label: $t("projects.title") },
          { label: selectedProject.name },
        ]
      : [
          { label: $t("nav.home"), icon: Home },
          { label: $t("projects.title") },
        ],
  );
</script>

<div class="mx-auto w-full max-w-4xl space-y-6" data-testid="projects-page">
  <Breadcrumbs items={breadcrumbItems} />

  <!-- Header -->
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-semibold">{$t("projects.title")}</h1>
      <p class="text-xs text-muted-foreground mt-0.5">{$t("projects.subtitle")}</p>
    </div>
    <Button size="sm" onclick={() => (showCreateDialog = true)}>
      <Plus size={14} strokeWidth={2} class="mr-1.5" />
      {$t("projects.new_project")}
    </Button>
  </div>

  {#if !loading && $projects.length > 0}
    <!-- Toolbar: search + sort + view toggle -->
    <div class="flex flex-wrap items-center gap-2">
      <div class="relative">
        <Search
          size={13}
          class="absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground/60"
          strokeWidth={1.75}
        />
        <input
          type="search"
          class="h-8 w-56 rounded-md border border-border/60 bg-background pl-7 pr-2 text-xs placeholder:text-muted-foreground/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
          bind:value={searchQuery}
          placeholder={$t("projects.search_placeholder")}
          aria-label={$t("projects.search_placeholder")}
          data-testid="projects-search-input"
        />
      </div>
      <select
        class="h-8 rounded-md border border-border/60 bg-background px-2 text-xs focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
        value={sort}
        onchange={handleSortChange}
        aria-label={$t("projects.filter_date_label")}
        data-testid="projects-sort-select"
      >
        <option value="recent">{$t("projects.filter_date_recent")}</option>
        <option value="oldest">{$t("projects.filter_date_oldest")}</option>
      </select>
      <div class="ml-auto">
        <ProjectsViewToggle {view} onchange={handleViewChange} />
      </div>
    </div>
  {/if}

  <!-- Content -->
  {#if loading}
    <div class="flex items-center justify-center py-16 text-muted-foreground text-sm">
      {$t("common.loading")}
    </div>
  {:else if $projects.length === 0}
    <EmptyState
      icon={FolderOpen}
      title={$t("projects.empty_title")}
      subtitle={$t("projects.empty_subtitle")}
      ctaLabel={$t("projects.new_project")}
      ctaAction={() => (showCreateDialog = true)}
    />
  {:else if filteredProjects.length === 0}
    <div class="glass-card glass-border rounded-lg px-4 py-10 text-center">
      <p class="text-xs text-muted-foreground">{$t("projects.no_match")}</p>
    </div>
  {:else if view === "grid"}
    <div
      class="grid gap-3 md:grid-cols-2 lg:grid-cols-3"
      data-testid="projects-grid"
    >
      {#each filteredProjects as project (project.id)}
        <div
          class="glass-card glass-border rounded-lg p-4 transition-colors hover:bg-muted/40 cursor-pointer flex flex-col gap-2"
          role="button"
          tabindex="0"
          data-testid="project-card-{project.id}"
          onclick={() => openDetail(project.id)}
          onkeydown={(e) => e.key === "Enter" && openDetail(project.id)}
        >
          <div class="flex items-center gap-2">
            <FolderOpen size={18} strokeWidth={1.5} class="shrink-0 text-primary/70" />
            <p class="text-sm font-medium truncate flex-1">{project.name}</p>
            <button
              class="h-6 w-6 inline-flex items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
              title={$t("common.delete")}
              onclick={(e) => { e.stopPropagation(); handleDeleteRequest(project.id, project.name); }}
              data-testid="delete-project-{project.id}"
            >
              <Trash2 size={12} strokeWidth={1.75} />
            </button>
          </div>
          {#if project.description}
            <p class="text-xs text-muted-foreground line-clamp-2">{project.description}</p>
          {/if}
        </div>
      {/each}
    </div>
  {:else}
    <div class="space-y-2" data-testid="projects-list">
      {#each filteredProjects as project (project.id)}
        <div
          class="flex items-center gap-4 rounded-lg border glass-border glass-panel px-4 py-3 transition-colors hover:bg-muted/40 cursor-pointer"
          role="button"
          tabindex="0"
          data-testid="project-card-{project.id}"
          onclick={() => openDetail(project.id)}
          onkeydown={(e) => e.key === "Enter" && openDetail(project.id)}
        >
          <FolderOpen size={20} strokeWidth={1.5} class="shrink-0 text-primary/70" />
          <div class="flex-1 min-w-0">
            <p class="text-sm font-medium truncate">{project.name}</p>
            {#if project.description}
              <p class="text-xs text-muted-foreground truncate">{project.description}</p>
            {/if}
          </div>
          <div class="flex items-center gap-1 shrink-0">
            <button
              class="h-7 w-7 inline-flex items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
              title={$t("common.delete")}
              onclick={(e) => { e.stopPropagation(); handleDeleteRequest(project.id, project.name); }}
              data-testid="delete-project-{project.id}"
            >
              <Trash2 size={14} strokeWidth={1.75} />
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<!-- Dialogs -->
<CreateProjectDialog
  open={showCreateDialog}
  onclose={() => (showCreateDialog = false)}
  oncreated={handleProjectCreated}
/>

<ProjectDetailSheet
  open={detailOpen}
  project={selectedProject}
  loading={detailLoading}
  onclose={() => { detailOpen = false; selectedProject = null; }}
  onupdated={handleProjectUpdated}
  ondelete={(id, name) => handleDeleteRequest(id, name)}
/>

<ConfirmDialog
  open={showDeleteConfirm}
  title={$t("projects.delete_confirm_title")}
  message={$t("projects.delete_confirm_message", { values: { name: deleteProjectName } })}
  confirmLabel={$t("projects.delete_confirm_yes")}
  loading={deleting}
  onconfirm={handleDeleteConfirm}
  onclose={() => (showDeleteConfirm = false)}
/>
