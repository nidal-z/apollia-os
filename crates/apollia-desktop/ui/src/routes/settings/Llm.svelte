<script lang="ts" module>
  export const meta = {
    title: "settings.nav.llm",
    icon: "cpu",
    group: "settings.nav.cluster_ai",
    cluster: "ai",
  } as const;
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import { flip } from "svelte/animate";
  import { fly } from "svelte/transition";
  import { t } from "svelte-i18n";
  import { Plus, RefreshCw, Cpu } from "lucide-svelte";
  import type { LlmBackendConfig } from "$lib/types";
  import type { HumanizedError } from "$lib/errors/humanize";
  import { humanize } from "$lib/errors/humanize";
  import { reportError } from "$lib/errors/reportError";
  import {
    pingLlmBackend,
    setDefaultLlmBackend,
    reloadLlmFromDb,
    reloadLlm,
  } from "$lib/ipc/llm";
  import { llmBackendsStore, settingsLoaders } from "$lib/stores/settings";
  import { listFlip, rowIn } from "$lib/design/listMotion";
  import { listNavigation } from "$lib/components/operator/listNavigation";
  import { addToast } from "$lib/components/ui/toast";
  import { Button } from "$lib/components/ui/button";
  import { Card } from "$lib/components/ui/card";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { EmptyState } from "$lib/components/operator";
  import LlmBackendCard from "../../components/llm/LlmBackendCard.svelte";
  import LlmDeleteDialog from "../../components/llm/LlmDeleteDialog.svelte";
  import LlmActionErrorBanner from "../../components/settings/LlmActionErrorBanner.svelte";
  import LlmBackendDialog from "../../components/settings/LlmBackendDialog.svelte";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import SettingsSection from "../../components/settings/SettingsSection.svelte";

  const SKELETON_COUNT = 2;

  let dialogOpen = $state(false);
  let editingBackend = $state<LlmBackendConfig | null>(null);
  let deleteTarget = $state<LlmBackendConfig | null>(null);
  let reloading = $state(false);
  let actionError = $state<HumanizedError | null>(null);

  const anyDialogOpen = $derived(dialogOpen || !!deleteTarget);
  const backends = $derived($llmBackendsStore.data ?? []);
  const loading = $derived($llmBackendsStore.loading && !$llmBackendsStore.loaded);
  const loadError = $derived.by<HumanizedError | null>(() => {
    const raw = $llmBackendsStore.error;
    return raw ? humanize(raw, (k) => $t(k)) : null;
  });

  async function refresh(): Promise<void> {
    await settingsLoaders.llmBackends(true);
  }

  function openAdd(): void {
    editingBackend = null;
    dialogOpen = true;
  }

  function openEdit(backend: LlmBackendConfig): void {
    editingBackend = backend;
    dialogOpen = true;
  }

  async function handleSetDefault(backend: LlmBackendConfig): Promise<void> {
    if (backend.is_default) return;
    actionError = null;
    try {
      await setDefaultLlmBackend(backend.name);
      // Rebuild the in-memory router so the new default takes effect at once,
      // instead of leaving an inconsistent default until the next full reload.
      await reloadLlmFromDb();
      await refresh();
    } catch (err: unknown) {
      actionError = reportError(err, { surface: "inline" });
    }
  }

  async function handleReload(): Promise<void> {
    if (reloading) return;
    reloading = true;
    actionError = null;
    try {
      await reloadLlm();
      addToast($t("settings.llm.reload_toast"), "success");
      await refresh();
    } catch (err: unknown) {
      actionError = reportError(err, { surface: "inline" });
    } finally {
      reloading = false;
    }
  }

  function onSaved(): void {
    dialogOpen = false;
    void refresh();
  }

  // Ping every enabled backend once so cached `last_ping_error` is populated and
  // each card can render its persistent connected / error status on first paint.
  async function autoPingAllEnabled(): Promise<void> {
    const enabled = backends.filter((b) => b.enabled);
    if (enabled.length === 0) return;
    await Promise.all(enabled.map((b) => pingLlmBackend(b.name).catch(() => null)));
    await refresh();
  }

  function onKeydown(event: KeyboardEvent): void {
    if (anyDialogOpen || event.metaKey || event.ctrlKey || event.altKey) return;
    const target = event.target as HTMLElement | null;
    if (
      target &&
      (target.isContentEditable ||
        ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName))
    ) {
      return;
    }
    const key = event.key.toLowerCase();
    if (key === "a") {
      event.preventDefault();
      openAdd();
    } else if (key === "r") {
      event.preventDefault();
      void handleReload();
    }
  }

  onMount(() => {
    void (async () => {
      await settingsLoaders.llmBackends();
      void autoPingAllEnabled();
    })();
    window.addEventListener("keydown", onKeydown);
    return () => window.removeEventListener("keydown", onKeydown);
  });
</script>

{#snippet sectionIcon()}
  <Cpu size={15} strokeWidth={1.75} aria-hidden="true" />
{/snippet}

{#snippet sectionActions()}
  <div class="flex items-center gap-2">
    <Button
      variant="outline"
      size="sm"
      onclick={handleReload}
      disabled={reloading}
      data-testid="llm-reload-btn"
    >
      <RefreshCw size={14} class={reloading ? "animate-spin" : ""} />
      {$t("settings.llm.reload_btn")}
    </Button>
    <Button variant="primary-gradient" size="sm" onclick={openAdd} data-testid="add-backend-btn">
      <Plus size={14} />
      {$t("settings.llm_add_backend")}
    </Button>
  </div>
{/snippet}

{#snippet emptyIcon()}
  <Cpu size={24} strokeWidth={1.8} aria-hidden="true" />
{/snippet}

{#snippet emptyAction()}
  <Button variant="primary-gradient" size="sm" onclick={openAdd} data-testid="llm-empty-add-btn">
    <Plus size={14} />
    {$t("settings.llm_add_backend")}
  </Button>
{/snippet}

<SettingsSubPage route="llm" data-testid="llm-backends-section">
  <SettingsSection title={$t("settings.llm_backends")} icon={sectionIcon} actions={sectionActions}>
    {#if actionError}
      <LlmActionErrorBanner error={actionError} onretry={handleReload} loading={reloading} />
    {/if}

    {#if loading}
      <div class="grid grid-cols-1 gap-3 lg:grid-cols-2" data-testid="llm-skeleton">
        {#each { length: SKELETON_COUNT } as _}
          <Card class="space-y-3 p-4">
            <div class="flex items-center justify-between">
              <Skeleton variant="text" class="h-5 w-1/2" />
              <Skeleton variant="text" class="h-5 w-14 rounded-full" />
            </div>
            <Skeleton variant="text" class="h-3 w-2/3" />
            <Skeleton variant="text" class="h-3 w-2/5" />
          </Card>
        {/each}
      </div>
    {:else if loadError}
      <LlmActionErrorBanner error={loadError} onretry={refresh} />
    {:else if backends.length === 0}
      <div data-testid="llm-backends-empty">
        <EmptyState icon={emptyIcon} title={$t("settings.llm_no_backends")} action={emptyAction} />
      </div>
    {:else}
      <div
        class="grid grid-cols-1 gap-3 lg:grid-cols-2"
        data-testid="llm-backends-list"
        use:listNavigation={{ rowSelector: '[data-testid="llm-backend-card"]' }}
      >
        {#each backends as backend (backend.name)}
          <div animate:flip={listFlip()} in:fly={rowIn()}>
            <LlmBackendCard
              {backend}
              onSetDefault={handleSetDefault}
              onEdit={openEdit}
              onRemove={(b) => (deleteTarget = b)}
            />
          </div>
        {/each}
      </div>
    {/if}
  </SettingsSection>
</SettingsSubPage>

<LlmBackendDialog
  open={dialogOpen}
  backend={editingBackend}
  onclose={() => (dialogOpen = false)}
  onsaved={onSaved}
/>

<LlmDeleteDialog
  target={deleteTarget}
  onclose={() => (deleteTarget = null)}
  ondeleted={() => {
    deleteTarget = null;
    void refresh();
  }}
/>
