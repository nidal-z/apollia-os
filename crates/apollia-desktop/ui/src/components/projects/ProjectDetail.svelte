<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Sheet } from "$lib/components/ui/sheet";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Textarea } from "$lib/components/ui/textarea";
  import { addToast } from "$lib/components/ui/toast/store";
  import {
    FolderOpen, FileText, Puzzle, Eye, Trash2, Upload,
    ChevronDown, ChevronUp, X, Loader2,
  } from "lucide-svelte";
  import type { ProjectDetail, WorkspaceSnapshotView } from "$lib/types";

  interface Props {
    open: boolean;
    project: ProjectDetail | null;
    loading: boolean;
    onclose: () => void;
    onupdated: () => void;
    ondelete: (id: string, name: string) => void;
  }

  let { open, project, loading, onclose, onupdated, ondelete }: Props = $props();

  // Edit state
  let editing = $state(false);
  let editName = $state("");
  let editDescription = $state("");
  let editInstructions = $state("");
  let saving = $state(false);

  // Snapshot preview
  let snapshotOpen = $state(false);
  let snapshot = $state<WorkspaceSnapshotView | null>(null);
  let snapshotLoading = $state(false);

  // Document upload
  let uploading = $state(false);

  $effect(() => {
    if (project) {
      editName = project.name;
      editDescription = project.description ?? "";
      editInstructions = project.instructions ?? "";
    }
    editing = false;
    snapshot = null;
    snapshotOpen = false;
  });

  async function saveChanges(): Promise<void> {
    if (!project || !editName.trim()) return;
    saving = true;
    try {
      await invoke("update_project", {
        id: project.id,
        request: {
          name: editName.trim(),
          description: editDescription.trim() || null,
          instructions: editInstructions.trim() || null,
        },
      });
      addToast($t("projects.updated_toast", { values: { name: editName.trim() } }), "success");
      editing = false;
      onupdated();
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      saving = false;
    }
  }

  async function loadSnapshot(): Promise<void> {
    if (!project) return;
    snapshotLoading = true;
    try {
      snapshot = await invoke<WorkspaceSnapshotView>("get_project_snapshot", {
        projectId: project.id,
      });
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      snapshotLoading = false;
    }
  }

  function toggleSnapshot() {
    snapshotOpen = !snapshotOpen;
    if (snapshotOpen && !snapshot) {
      void loadSnapshot();
    }
  }

  async function removeDocument(docId: string): Promise<void> {
    try {
      await invoke("delete_project_document", { docId });
      addToast($t("projects.document_removed"), "success");
      onupdated();
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }
</script>

<Sheet {open} {onclose} class="w-[480px]">
  {#if loading || !project}
    <div class="flex flex-1 items-center justify-center py-16 text-muted-foreground text-sm">
      {#if loading}
        <Loader2 size={20} class="animate-spin mr-2" />
        {$t("common.loading")}
      {:else}
        {$t("projects.not_found")}
      {/if}
    </div>
  {:else}
    <!-- Header -->
    <div class="flex items-center gap-3 border-b border-border px-5 py-4">
      <FolderOpen size={20} strokeWidth={1.5} class="shrink-0 text-primary/70" />
      {#if editing}
        <Input bind:value={editName} class="flex-1 h-8 text-sm font-medium" autofocus />
      {:else}
        <h2 class="flex-1 text-base font-semibold truncate">{project.name}</h2>
      {/if}
      <button
        class="ml-auto h-7 w-7 inline-flex items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
        onclick={onclose}
        aria-label="Close"
      >
        <X size={16} strokeWidth={1.75} />
      </button>
    </div>

    <!-- Body -->
    <div class="flex-1 overflow-y-auto px-5 py-4 space-y-5">

      <!-- Description -->
      <section class="space-y-1.5">
        <p class="text-xs font-semibold uppercase tracking-wider text-muted-foreground/60">
          {$t("projects.field_description")}
        </p>
        {#if editing}
          <Input bind:value={editDescription} placeholder={$t("projects.field_description_placeholder")} />
        {:else}
          <p class="text-sm text-muted-foreground">
            {project.description ?? $t("projects.no_description")}
          </p>
        {/if}
      </section>

      <!-- Instructions -->
      <section class="space-y-1.5">
        <p class="text-xs font-semibold uppercase tracking-wider text-muted-foreground/60">
          {$t("projects.field_instructions")}
        </p>
        {#if editing}
          <Textarea
            bind:value={editInstructions}
            rows={5}
            class="resize-none text-sm"
            placeholder={$t("projects.field_instructions_placeholder")}
          />
        {:else if project.instructions}
          <p class="text-sm whitespace-pre-wrap rounded-md bg-muted/50 px-3 py-2">
            {project.instructions}
          </p>
        {:else}
          <p class="text-sm text-muted-foreground">{$t("projects.no_instructions")}</p>
        {/if}
      </section>

      <!-- Context Providers -->
      <section class="space-y-1.5">
        <p class="text-xs font-semibold uppercase tracking-wider text-muted-foreground/60">
          {$t("projects.section_providers")}
        </p>
        {#if project.providers.length === 0}
          <p class="text-sm text-muted-foreground">{$t("projects.no_providers")}</p>
        {:else}
          <div class="space-y-1">
            {#each project.providers as provider (provider.id)}
              <div class="flex items-center gap-2 rounded-md bg-muted/40 px-3 py-1.5 text-sm">
                <Puzzle size={14} strokeWidth={1.75} class="shrink-0 text-muted-foreground" />
                <span class="flex-1 truncate">{provider.name}</span>
                <span class="text-xs text-muted-foreground/60 font-mono">{provider.provider_type}</span>
                <span
                  class="text-xs px-1.5 py-0.5 rounded-full {provider.enabled
                    ? 'bg-success/20 text-success'
                    : 'bg-muted text-muted-foreground'}"
                >
                  {provider.enabled ? $t("projects.provider_enabled") : $t("projects.provider_disabled")}
                </span>
              </div>
            {/each}
          </div>
        {/if}
      </section>

      <!-- Documents -->
      <section class="space-y-1.5">
        <div class="flex items-center justify-between">
          <p class="text-xs font-semibold uppercase tracking-wider text-muted-foreground/60">
            {$t("projects.section_documents")}
          </p>
        </div>
        {#if project.documents.length === 0}
          <p class="text-sm text-muted-foreground">{$t("projects.no_documents")}</p>
        {:else}
          <div class="space-y-1">
            {#each project.documents as doc (doc.id)}
              <div class="flex items-center gap-2 rounded-md bg-muted/40 px-3 py-1.5 text-sm">
                <FileText size={14} strokeWidth={1.75} class="shrink-0 text-muted-foreground" />
                <span class="flex-1 truncate">{doc.name}</span>
                <span class="text-xs text-muted-foreground/60">
                  {(doc.size_bytes / 1024).toFixed(1)} KB
                </span>
                <button
                  class="h-5 w-5 inline-flex items-center justify-center rounded text-muted-foreground hover:text-destructive transition-colors"
                  onclick={() => removeDocument(doc.id)}
                  title={$t("common.delete")}
                >
                  <Trash2 size={12} strokeWidth={1.75} />
                </button>
              </div>
            {/each}
          </div>
        {/if}
      </section>

      <!-- Workspace Snapshot Preview -->
      <section class="space-y-1.5">
        <button
          class="flex w-full items-center gap-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground/60 hover:text-foreground transition-colors"
          onclick={toggleSnapshot}
        >
          <Eye size={12} strokeWidth={2} />
          {$t("projects.section_snapshot")}
          {#if snapshotOpen}
            <ChevronUp size={12} class="ml-auto" />
          {:else}
            <ChevronDown size={12} class="ml-auto" />
          {/if}
        </button>
        {#if snapshotOpen}
          {#if snapshotLoading}
            <div class="flex items-center gap-2 py-2 text-sm text-muted-foreground">
              <Loader2 size={14} class="animate-spin" />
              {$t("common.loading")}
            </div>
          {:else if snapshot && snapshot.sections.length > 0}
            <div class="space-y-2 rounded-md bg-muted/30 p-3">
              {#each snapshot.sections as section}
                <div>
                  <p class="text-xs font-medium text-muted-foreground">{section.title}</p>
                  <pre class="text-xs mt-0.5 whitespace-pre-wrap opacity-70 max-h-24 overflow-y-auto">{section.content}</pre>
                </div>
              {/each}
            </div>
          {:else}
            <p class="text-sm text-muted-foreground">{$t("projects.snapshot_empty")}</p>
          {/if}
        {/if}
      </section>
    </div>

    <!-- Footer actions -->
    <div class="border-t border-border px-5 py-3 flex items-center gap-2">
      {#if editing}
        <Button size="sm" onclick={saveChanges} disabled={saving || !editName.trim()}>
          {saving ? $t("common.submitting") : $t("common.save")}
        </Button>
        <Button size="sm" variant="outline" onclick={() => { editing = false; }} disabled={saving}>
          {$t("common.cancel")}
        </Button>
      {:else}
        <Button size="sm" variant="outline" onclick={() => { editing = true; }}>
          {$t("projects.edit_project")}
        </Button>
      {/if}
      <div class="flex-1"></div>
      <Button
        size="sm"
        variant="ghost"
        class="text-destructive hover:bg-destructive/10"
        onclick={() => ondelete(project.id, project.name)}
      >
        <Trash2 size={14} strokeWidth={1.75} class="mr-1.5" />
        {$t("common.delete")}
      </Button>
    </div>
  {/if}
</Sheet>
