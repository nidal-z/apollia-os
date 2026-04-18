<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { FolderOpen, Plus, Trash2, Edit2, Eye } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast/store";
  import { projects } from "$lib/stores/projects";
  import type { ProjectDetail, ProjectSummary } from "$lib/types";
  import EmptyState from "../components/common/EmptyState.svelte";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import CreateProjectDialog from "../components/projects/CreateProjectDialog.svelte";
  import ProjectDetailSheet from "../components/projects/ProjectDetail.svelte";

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
</script>

<div class="mx-auto w-full max-w-4xl space-y-6" data-testid="projects-page">
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
  {:else}
    <div class="space-y-2">
      {#each $projects as project (project.id)}
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
