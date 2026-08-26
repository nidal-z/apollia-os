<script lang="ts">
  /**
   * ProjectSettingsTab - editable project metadata.
   *
   * Owns its own form state (seeded from the `project` prop and re-seeded when
   * the project identity or `updated_at` changes), the partial-patch save via
   * the `updateProject` IPC wrapper, and the workspace directory picker. It
   * bubbles a saved project up via `onSaved` and defers destructive deletion
   * to the parent through `onRequestDelete`.
   */
  import { t } from "svelte-i18n";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { FolderOpen, Trash2 } from "lucide-svelte";
  import { SectionTitle } from "$lib/components/operator";
  import { Input } from "$lib/components/ui/input";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast/store";
  import { reportError } from "$lib/errors/reportError";
  import { updateProject } from "$lib/ipc/projects";
  import type { ProjectDetail, UpdateProjectRequest } from "$lib/types";

  interface Props {
    project: ProjectDetail;
    onSaved: (updated: ProjectDetail) => void;
    onRequestDelete: (id: string, name: string) => void;
  }

  let { project, onSaved, onRequestDelete }: Props = $props();

  // Seeded from `project`; the effect below re-seeds on identity/updated_at change.
  // svelte-ignore state_referenced_locally
  let editName = $state(project.name);
  // svelte-ignore state_referenced_locally
  let editDescription = $state(project.description ?? "");
  // svelte-ignore state_referenced_locally
  let editInstructions = $state(project.instructions ?? "");
  // svelte-ignore state_referenced_locally
  let editWorkspacePath = $state(project.workspace_path ?? "");
  let savingMetadata = $state(false);

  // svelte-ignore state_referenced_locally
  let seededKey = $state(`${project.id}:${project.updated_at}`);
  $effect(() => {
    const key = `${project.id}:${project.updated_at}`;
    if (key !== seededKey) {
      seededKey = key;
      resetEditForm();
    }
  });

  function resetEditForm(): void {
    editName = project.name;
    editDescription = project.description ?? "";
    editInstructions = project.instructions ?? "";
    editWorkspacePath = project.workspace_path ?? "";
  }

  const settingsDirty = $derived(
    editName.trim() !== project.name ||
      editDescription.trim() !== (project.description ?? "") ||
      editInstructions.trim() !== (project.instructions ?? "") ||
      editWorkspacePath.trim() !== (project.workspace_path ?? ""),
  );

  async function pickWorkspaceDir(): Promise<void> {
    try {
      const selected = await openDialog({ multiple: false, directory: true });
      if (typeof selected === "string") editWorkspacePath = selected;
    } catch (err) {
      reportError(err, { surface: "toast" });
    }
  }

  async function saveMetadata(): Promise<void> {
    savingMetadata = true;
    try {
      const request: UpdateProjectRequest = {};
      const trimmedName = editName.trim();
      if (trimmedName && trimmedName !== project.name) request.name = trimmedName;
      const desc = editDescription.trim();
      if (desc !== (project.description ?? "")) request.description = desc || null;
      const instr = editInstructions.trim();
      if (instr !== (project.instructions ?? "")) request.instructions = instr || null;
      const ws = editWorkspacePath.trim();
      if (ws !== (project.workspace_path ?? "")) request.workspace_path = ws || null;
      if (Object.keys(request).length === 0) return;
      const updated = await updateProject(project.id, request);
      addToast($t("projects.settings_saved_toast"), "success");
      onSaved(updated);
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      savingMetadata = false;
    }
  }
</script>

<SectionTitle>{$t("projects.tab_settings")}</SectionTitle>
<div class="px-8 pb-6 space-y-4 max-w-2xl">
  <div>
    <div class="text-label-sm text-muted-foreground mb-1">{$t("projects.field_id")}</div>
    <div class="text-body-xs font-mono text-foreground">{project.id}</div>
  </div>

  <div>
    <label for="project-settings-name" class="block text-label-sm text-muted-foreground mb-1">{$t("projects.settings_field_name")}</label>
    <Input id="project-settings-name" bind:value={editName} placeholder={$t("projects.field_name")} class="bg-card" data-testid="settings-name-input" />
  </div>

  <div>
    <label for="project-settings-description" class="block text-label-sm text-muted-foreground mb-1">{$t("projects.settings_field_description")}</label>
    <Textarea id="project-settings-description" bind:value={editDescription} rows={3} class="bg-card" data-testid="settings-description-input" />
  </div>

  <div>
    <label for="project-settings-instructions" class="block text-label-sm text-muted-foreground mb-1">{$t("projects.settings_field_instructions")}</label>
    <Textarea
      id="project-settings-instructions"
      bind:value={editInstructions}
      rows={5}
      placeholder={$t("projects.settings_field_instructions_placeholder")}
      class="bg-card"
      data-testid="settings-instructions-input"
    />
  </div>

  <div>
    <label for="project-settings-workspace" class="block text-label-sm text-muted-foreground mb-1">{$t("projects.field_workspace")}</label>
    <div class="flex gap-2">
      <Input id="project-settings-workspace" bind:value={editWorkspacePath} placeholder={$t("projects.field_workspace_placeholder")} class="flex-1 bg-card" data-testid="settings-workspace-input" />
      <Button variant="outline" size="sm" onclick={pickWorkspaceDir}>
        {#snippet icon()}<FolderOpen size={12} />{/snippet}
        {$t("projects.settings_field_workspace_pick")}
      </Button>
    </div>
  </div>

  <div class="flex items-center gap-2 pt-1">
    <Button variant="primary-solid" size="sm" onclick={saveMetadata} disabled={!settingsDirty || savingMetadata}>
      {savingMetadata ? $t("common.saving") : $t("common.save")}
    </Button>
    <Button variant="outline" size="sm" onclick={resetEditForm} disabled={!settingsDirty || savingMetadata}>
      {$t("common.cancel")}
    </Button>
  </div>

  <div class="pt-3 border-t border-border">
    <Button
      variant="ghost"
      size="sm"
      type="button"
      onclick={() => onRequestDelete(project.id, project.name)}
      class="inline-flex items-center gap-1.5 text-body-xs text-danger-a11y hover:underline"
      data-testid="delete-project-{project.id}"
    >
      {#snippet icon()}<Trash2 size={12} />{/snippet}
      {$t("common.delete")}
    </Button>
  </div>
</div>
