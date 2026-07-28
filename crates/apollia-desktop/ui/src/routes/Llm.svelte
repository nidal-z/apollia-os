<script lang="ts">
  import { flip } from "svelte/animate";
  import { fly } from "svelte/transition";
  import { t } from "svelte-i18n";
  import { Brain, Plus, RefreshCw } from "lucide-svelte";
  import { llmBackends, connectionStatus } from "$lib/stores/sse";
  import { uiMode } from "$lib/stores/mode";
  import { currentRoute } from "$lib/stores/navigation";
  import type { LlmBackendConfig } from "$lib/types";
  import type { HumanizedError } from "$lib/errors/humanize";
  import { reportError } from "$lib/errors/reportError";
  import {
    listLlmBackends,
    setDefaultLlmBackend,
    reloadLlmFromDb,
    reloadLlm,
  } from "$lib/ipc/llm";
  import { listFlip, rowIn } from "$lib/design/listMotion";
  import { listNavigation } from "$lib/components/operator/listNavigation";
  import { addToast } from "$lib/components/ui/toast";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { Card } from "$lib/components/ui/card";
  import { Button } from "$lib/components/ui/button";
  import { PageLayout, EmptyState } from "$lib/components/operator";
  import LlmBackendCard from "../components/llm/LlmBackendCard.svelte";
  import LlmStats from "../components/llm/LlmStats.svelte";
  import LlmDeleteDialog from "../components/llm/LlmDeleteDialog.svelte";
  import LlmErrorNotice from "../components/llm/LlmErrorNotice.svelte";
  import LlmBackendDialog from "../components/settings/LlmBackendDialog.svelte";

  const SKELETON_COUNT = 2;

  const isOperator = $derived($uiMode === "operator");
  const isBuilder = $derived($uiMode === "builder");

  let dialogOpen = $state(false);
  let editingBackend = $state<LlmBackendConfig | null>(null);
  let deleteTarget = $state<LlmBackendConfig | null>(null);
  let reloading = $state(false);
  let pageError = $state<HumanizedError | null>(null);

  const anyDialogOpen = $derived(dialogOpen || !!deleteTarget);

  async function refresh() {
    try {
      llmBackends.set(await listLlmBackends());
    } catch (err: unknown) {
      reportError(err, { surface: "toast" });
    }
  }

  function openAdd() {
    editingBackend = null;
    dialogOpen = true;
  }

  function openEdit(backend: LlmBackendConfig) {
    editingBackend = backend;
    dialogOpen = true;
  }

  function openSettings() {
    currentRoute.set("settings");
  }

  async function handleSetDefault(backend: LlmBackendConfig) {
    if (backend.is_default) return;
    try {
      await setDefaultLlmBackend(backend.name);
      await reloadLlmFromDb();
      addToast($t("llm.default_set_toast", { values: { name: backend.name } }), "success");
      await refresh();
    } catch (err: unknown) {
      reportError(err, { surface: "toast" });
    }
  }

  async function handleReload() {
    if (reloading) return;
    reloading = true;
    pageError = null;
    try {
      await reloadLlm();
      addToast($t("settings.llm.reload_toast"), "success");
      await refresh();
    } catch (err: unknown) {
      pageError = reportError(err, { surface: "inline" });
    } finally {
      reloading = false;
    }
  }

  function onSaved() {
    dialogOpen = false;
    void refresh();
  }

  function onKeydown(event: KeyboardEvent) {
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
    } else if (key === "r" && isBuilder) {
      event.preventDefault();
      void handleReload();
    }
  }

  $effect(() => {
    window.addEventListener("keydown", onKeydown);
    return () => window.removeEventListener("keydown", onKeydown);
  });
</script>

{#snippet emptyIcon()}
  <Brain size={26} strokeWidth={1.8} aria-hidden="true" />
{/snippet}

{#snippet emptyActions()}
  <div class="flex flex-wrap justify-center gap-2">
    <Button variant="primary-gradient" size="sm" onclick={openAdd} data-testid="llm-empty-add-btn">
      <Plus size={14} />
      {$t("settings.llm_add_backend")}
    </Button>
    <Button variant="outline" size="sm" onclick={openSettings}>
      {$t("llm.open_settings")}
    </Button>
  </div>
{/snippet}

{#snippet headerActions()}
  {#if isBuilder}
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
  {/if}
  <Button variant="primary-gradient" size="sm" onclick={openAdd} data-testid="llm-add-backend-btn">
    <Plus size={14} />
    {$t("settings.llm_add_backend")}
  </Button>
{/snippet}

<PageLayout
  title={isOperator ? $t("llm.title_operator") : $t("llm.title")}
  subtitle={$t("llm.subtitle")}
  actions={headerActions}
  data-testid="llm-page"
>
  <div class="space-y-6 px-8 pb-8 pt-6">
    {#if pageError}
      <LlmErrorNotice
        error={pageError}
        loading={reloading}
        onretry={handleReload}
        onOpenSettings={openSettings}
      />
    {/if}

    {#if $connectionStatus === "connecting"}
      <div class="grid gap-3 sm:grid-cols-1 md:grid-cols-2" data-testid="llm-skeleton">
        {#each { length: SKELETON_COUNT } as _}
          <Card class="space-y-3 p-4">
            <div class="flex items-center justify-between">
              <Skeleton variant="text" class="h-5 w-[50%]" />
              <Skeleton variant="text" class="h-5 w-14 rounded-full" />
            </div>
            <Skeleton variant="text" class="h-3 w-[70%]" />
            <Skeleton variant="text" class="h-3 w-[40%]" />
          </Card>
        {/each}
      </div>
    {:else if $llmBackends.length === 0}
      <div data-testid="llm-empty">
        <EmptyState
          icon={emptyIcon}
          title={isOperator ? $t("llm.empty_operator") : $t("llm.empty_title")}
          desc={isOperator ? $t("llm.empty_operator_hint") : $t("llm.empty_subtitle")}
          action={emptyActions}
        />
      </div>
    {:else}
      <div
        class="grid gap-3 sm:grid-cols-1 md:grid-cols-2"
        data-testid="llm-grid"
        use:listNavigation={{ rowSelector: '[data-testid="llm-backend-card"]' }}
      >
        {#each $llmBackends as backend (backend.name)}
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

      <div class="mt-6">
        <h2 class="mb-3 text-sm font-medium uppercase tracking-wider text-muted-foreground">
          {$t("llm.session_stats_title")}
        </h2>
        <LlmStats />
      </div>
    {/if}
  </div>
</PageLayout>

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
