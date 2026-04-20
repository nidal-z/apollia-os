<script lang="ts" context="module">
  export const meta = {
    title: "settings.nav.llm",
    icon: "cpu",
    group: "settings.nav.cluster_ai",
    cluster: "ai",
  } as const;
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Plus, Pencil, Trash2, Star, CheckCircle2, XCircle, PauseCircle } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import Dialog from "$lib/components/ui/dialog/Dialog.svelte";
  import DialogFooter from "$lib/components/ui/dialog/DialogFooter.svelte";
  import SettingSectionSkeleton from "../../components/settings/SettingSectionSkeleton.svelte";
  import LlmBackendDialog from "../../components/settings/LlmBackendDialog.svelte";
  import { llmBackendsStore, settingsLoaders } from "$lib/stores/settings";
  import { addToast } from "$lib/components/ui/toast";
  import type { LlmBackendConfig } from "$lib/types";

  let actionError = $state<string | null>(null);
  let dialogOpen = $state(false);
  let editingBackend = $state<LlmBackendConfig | null>(null);

  // Delete confirmation
  let deleteTarget = $state<LlmBackendConfig | null>(null);
  let deleteConfirmText = $state("");
  let deleting = $state(false);

  async function refresh() {
    await settingsLoaders.llmBackends(true);
  }

  function openAdd() {
    editingBackend = null;
    dialogOpen = true;
  }

  function openEdit(b: LlmBackendConfig) {
    editingBackend = b;
    dialogOpen = true;
  }

  function askDelete(b: LlmBackendConfig) {
    deleteTarget = b;
    deleteConfirmText = "";
    actionError = null;
  }

  const requiresConfirmType = $derived(!!deleteTarget?.is_default);
  const canConfirmDelete = $derived(
    !!deleteTarget && (!requiresConfirmType || deleteConfirmText === "DELETE"),
  );

  async function confirmDelete() {
    if (!deleteTarget || !canConfirmDelete) return;
    deleting = true;
    actionError = null;
    try {
      await invoke("delete_llm_backend", { name: deleteTarget.name });
      addToast($t("settings.llm.delete_toast", { values: { name: deleteTarget.name } }), "success");
      deleteTarget = null;
      await refresh();
    } catch (err) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      deleting = false;
    }
  }

  async function handleSetDefault(b: LlmBackendConfig) {
    actionError = null;
    try {
      await invoke("set_default_llm_backend", { name: b.name });
      await refresh();
    } catch (err) {
      actionError = err instanceof Error ? err.message : String(err);
    }
  }

  function statusOf(b: LlmBackendConfig): { kind: "connected" | "disabled" | "unknown"; label: string } {
    if (!b.enabled) return { kind: "disabled", label: $t("settings.llm.status_disabled") };
    return { kind: "connected", label: $t("settings.llm.status_configured") };
  }

  onMount(() => {
    void settingsLoaders.llmBackends();
  });
</script>

{#if $llmBackendsStore.loading && !$llmBackendsStore.loaded}
  <SettingSectionSkeleton rows={2} />
{:else}
  <section class="space-y-4" data-testid="llm-backends-section">
    <div class="flex items-center justify-between">
      <p class="text-sm text-muted-foreground">{$t("settings.llm_backends_subtitle")}</p>
      <Button size="sm" onclick={openAdd} data-testid="add-backend-btn">
        <Plus class="h-4 w-4 mr-1" />
        {$t("settings.llm_add_backend")}
      </Button>
    </div>

    {#if actionError}
      <div class="rounded-md border border-destructive/30 bg-destructive/5 px-4 py-2 text-sm text-destructive">
        {actionError}
      </div>
    {/if}

    {#if $llmBackendsStore.error}
      <div class="rounded-md border border-destructive/30 bg-destructive/5 px-4 py-2 text-sm text-destructive">
        {$llmBackendsStore.error}
      </div>
    {:else if ($llmBackendsStore.data ?? []).length === 0}
      <div
        class="rounded-lg border border-dashed border-border px-4 py-10 text-center text-sm text-muted-foreground"
        data-testid="llm-backends-empty"
      >
        {$t("settings.llm_no_backends")}
      </div>
    {:else}
      <div class="grid grid-cols-1 lg:grid-cols-2 gap-3" data-testid="llm-backends-list">
        {#each ($llmBackendsStore.data ?? []) as backend (backend.name)}
          {@const status = statusOf(backend)}
          <div
            class="glass-card glass-border rounded-lg p-4 flex flex-col gap-3"
            data-testid="llm-backend-card-{backend.name}"
          >
            <div class="flex items-start justify-between gap-3">
              <div class="min-w-0">
                <div class="flex items-center gap-2 flex-wrap">
                  <span class="text-sm font-mono font-medium truncate">{backend.name}</span>
                  {#if backend.is_default}
                    <span class="inline-flex items-center gap-1 rounded-full bg-primary/10 px-2 py-0.5 text-xs font-medium text-primary">
                      <Star class="h-3 w-3" />
                      {$t("settings.llm.badge_default")}
                    </span>
                  {/if}
                </div>
                <p class="mt-0.5 text-xs text-muted-foreground">
                  {backend.provider} · <span class="font-mono">{backend.model}</span>
                </p>
              </div>
              <div class="flex items-center gap-1">
                <button
                  type="button"
                  class="rounded-md p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground"
                  title={$t("settings.llm.edit")}
                  aria-label={$t("settings.llm.edit")}
                  onclick={() => openEdit(backend)}
                  data-testid="edit-backend-{backend.name}"
                >
                  <Pencil class="h-4 w-4" />
                </button>
                <button
                  type="button"
                  class="rounded-md p-1.5 text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                  title={$t("common.delete")}
                  aria-label={$t("common.delete")}
                  onclick={() => askDelete(backend)}
                  data-testid="delete-backend-{backend.name}"
                >
                  <Trash2 class="h-4 w-4" />
                </button>
              </div>
            </div>

            <div class="flex items-center justify-between text-xs">
              <span class="inline-flex items-center gap-1.5">
                {#if status.kind === "connected"}
                  <CheckCircle2 class="h-3.5 w-3.5 text-success" />
                {:else if status.kind === "disabled"}
                  <PauseCircle class="h-3.5 w-3.5 text-muted-foreground" />
                {:else}
                  <XCircle class="h-3.5 w-3.5 text-destructive" />
                {/if}
                <span class="text-muted-foreground">{status.label}</span>
              </span>
              {#if !backend.is_default}
                <button
                  type="button"
                  class="text-primary hover:underline"
                  onclick={() => handleSetDefault(backend)}
                  data-testid="set-default-{backend.name}"
                >
                  {$t("settings.llm_set_default")}
                </button>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </section>
{/if}

<LlmBackendDialog
  open={dialogOpen}
  backend={editingBackend}
  onclose={() => (dialogOpen = false)}
  onsaved={() => {
    dialogOpen = false;
    void refresh();
  }}
/>

<!-- Delete confirm dialog -->
<Dialog
  open={!!deleteTarget}
  onclose={() => (deleteTarget = null)}
  size="sm"
  title={$t("settings.llm.delete_title")}
  data-testid="llm-delete-dialog"
>
  {#if deleteTarget}
    <p class="text-sm text-muted-foreground">
      {$t("settings.llm.delete_message", { values: { name: deleteTarget.name } })}
    </p>
    {#if requiresConfirmType}
      <div class="mt-4 space-y-1.5">
        <label for="llm-delete-confirm" class="text-xs font-medium text-foreground">
          {$t("settings.llm.delete_type_prompt")}
        </label>
        <input
          id="llm-delete-confirm"
          type="text"
          placeholder="DELETE"
          class="flex h-9 w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm font-mono focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
          bind:value={deleteConfirmText}
          data-testid="llm-delete-confirm-input"
        />
      </div>
    {/if}
  {/if}
  <DialogFooter>
    <Button variant="outline" onclick={() => (deleteTarget = null)} data-testid="llm-delete-cancel">
      {$t("common.cancel")}
    </Button>
    <Button
      variant="destructive"
      onclick={confirmDelete}
      disabled={deleting || !canConfirmDelete}
      data-testid="llm-delete-confirm-btn"
    >
      {deleting ? $t("common.loading") : $t("common.delete")}
    </Button>
  </DialogFooter>
</Dialog>
