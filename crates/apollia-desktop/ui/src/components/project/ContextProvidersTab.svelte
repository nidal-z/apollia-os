<script lang="ts">
  import { t } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { Plus, Eye, RefreshCw, Layers } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import EmptyState from "$lib/components/operator/EmptyState.svelte";
  import SectionTitle from "$lib/components/operator/SectionTitle.svelte";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { addToast } from "$lib/components/ui/toast/store";
  import type { ProjectDetail, ProjectProviderRow, WorkspaceSnapshotView } from "$lib/types";
  import ProviderCard from "./ProviderCard.svelte";
  import ProviderEditDialog from "./ProviderEditDialog.svelte";
  import SnapshotPreview from "./SnapshotPreview.svelte";

  interface Props {
    project: ProjectDetail;
    /** Called after any mutation so the parent can reload `get_project`. */
    onUpdated: () => Promise<void> | void;
  }

  let { project, onUpdated }: Props = $props();

  // ── Dialog state ────────────────────────────────────────────────────────────
  let dialogOpen = $state(false);
  let editing = $state<ProjectProviderRow | null>(null);

  // ── Delete confirm ──────────────────────────────────────────────────────────
  let deleteTarget = $state<ProjectProviderRow | null>(null);
  let deleting = $state(false);

  // ── Snapshot preview ────────────────────────────────────────────────────────
  let snapshotOpen = $state(false);
  let snapshot = $state<WorkspaceSnapshotView | null>(null);
  let snapshotLoading = $state(false);

  const sortedProviders = $derived(
    [...project.providers].sort((a, b) => a.priority - b.priority),
  );

  function openCreate(): void {
    editing = null;
    dialogOpen = true;
  }

  function openEdit(row: ProjectProviderRow): void {
    editing = row;
    dialogOpen = true;
  }

  function requestDelete(row: ProjectProviderRow): void {
    deleteTarget = row;
  }

  async function confirmDelete(): Promise<void> {
    if (!deleteTarget) return;
    deleting = true;
    try {
      await invoke("remove_project_provider", { providerId: deleteTarget.id });
      addToast($t("projects.provider_deleted_toast"), "success");
      deleteTarget = null;
      await onUpdated();
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      deleting = false;
    }
  }

  async function loadSnapshot(): Promise<void> {
    snapshotLoading = true;
    try {
      snapshot = await invoke<WorkspaceSnapshotView>("get_project_snapshot", {
        projectId: project.id,
      });
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
      snapshot = null;
    } finally {
      snapshotLoading = false;
    }
  }

  async function togglePreview(): Promise<void> {
    snapshotOpen = !snapshotOpen;
    if (snapshotOpen && !snapshot) {
      await loadSnapshot();
    }
  }
</script>

<SectionTitle count={project.providers.length}>
  {$t("projects.tab_context")}
</SectionTitle>

<div class="px-8 pb-6 space-y-4">
  <!-- Header actions -->
  <div class="flex items-center justify-between gap-3">
    <p class="m-0 text-[12px] text-muted-foreground leading-[1.55] max-w-[640px]">
      {$t("projects.context_intro")}
    </p>
    <div class="flex gap-2 shrink-0">
      <Button variant="outline" size="sm" onclick={togglePreview}>
        {#snippet icon()}<Eye size={12} />{/snippet}
        {$t("projects.context_snapshot_preview")}
      </Button>
      <Button variant="primary-solid" size="sm" onclick={openCreate}>
        {#snippet icon()}<Plus size={12} />{/snippet}
        {$t("projects.context_add_provider")}
      </Button>
    </div>
  </div>

  <!-- Providers list -->
  {#if project.providers.length === 0}
    <EmptyState
      title={$t("projects.context_empty_title")}
      desc={$t("projects.context_empty_desc")}
    >
      {#snippet icon()}<Layers size={20} />{/snippet}
      {#snippet action()}
        <Button variant="primary-solid" size="sm" onclick={openCreate}>
          {#snippet icon()}<Plus size={12} />{/snippet}
          {$t("projects.context_add_provider")}
        </Button>
      {/snippet}
    </EmptyState>
  {:else}
    <div class="space-y-2" data-testid="providers-list">
      {#each sortedProviders as provider (provider.id)}
        <ProviderCard
          {provider}
          onedit={openEdit}
          ondelete={requestDelete}
          onchanged={onUpdated}
        />
      {/each}
    </div>
  {/if}

  <!-- Snapshot preview -->
  {#if snapshotOpen}
    <div class="rounded-xl border border-border bg-card p-4 space-y-3">
      <div class="flex items-center justify-between gap-3">
        <div>
          <div class="text-[12.5px] font-semibold text-foreground">
            {$t("projects.context_snapshot_preview")}
          </div>
          <div class="text-[10.5px] text-muted-foreground mt-0.5">
            {$t("projects.context_snapshot_cwd_hint")}
          </div>
        </div>
        <Button variant="outline" size="sm" onclick={loadSnapshot}>
          {#snippet icon()}<RefreshCw size={12} />{/snippet}
          {$t("projects.context_snapshot_refresh")}
        </Button>
      </div>
      <SnapshotPreview {snapshot} loading={snapshotLoading} />
    </div>
  {/if}
</div>

<ProviderEditDialog
  open={dialogOpen}
  projectId={project.id}
  {editing}
  onclose={() => (dialogOpen = false)}
  onsaved={() => {
    dialogOpen = false;
    void onUpdated();
    // Invalidate cached snapshot so the next preview shows the new config.
    snapshot = null;
  }}
/>

<ConfirmDialog
  open={deleteTarget !== null}
  title={$t("projects.provider_delete_confirm_title")}
  message={$t("projects.provider_delete_confirm_message", {
    values: { name: deleteTarget?.name ?? "" },
  })}
  confirmLabel={$t("projects.delete_confirm_yes")}
  loading={deleting}
  onconfirm={confirmDelete}
  onclose={() => (deleteTarget = null)}
  data-testid="provider-delete-confirm"
/>
