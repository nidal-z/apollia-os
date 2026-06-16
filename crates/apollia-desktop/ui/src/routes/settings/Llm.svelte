<script lang="ts" context="module">
  import { Card } from "$lib/components/ui/card";
  import { Input } from "$lib/components/ui/input";
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
  import { Plus, Pencil, Trash2, Plug, Star, CheckCircle2, XCircle, PauseCircle } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import Dialog from "$lib/components/ui/dialog/Dialog.svelte";
  import DialogFooter from "$lib/components/ui/dialog/DialogFooter.svelte";
  import { ErrorBanner } from "$lib/components/operator";
  import SettingSectionSkeleton from "../../components/settings/SettingSectionSkeleton.svelte";
  import LlmBackendDialog from "../../components/settings/LlmBackendDialog.svelte";
  import { llmBackendsStore, settingsLoaders } from "$lib/stores/settings";
  import { addToast } from "$lib/components/ui/toast";
  import type { LlmBackendConfig, LlmPingResult } from "$lib/types";

  let actionError = $state<string | null>(null);
  let dialogOpen = $state(false);
  let editingBackend = $state<LlmBackendConfig | null>(null);

  // Delete confirmation
  let deleteTarget = $state<LlmBackendConfig | null>(null);
  let deleteConfirmText = $state("");
  let deleting = $state(false);

  // Inline ping test state - keyed by backend name.
  const FEEDBACK_DURATION_MS = 5_000;
  let testingMap = $state<Record<string, boolean>>({});
  let testResultMap = $state<Record<string, LlmPingResult>>({});
  const feedbackTimers: Record<string, ReturnType<typeof setTimeout>> = {};

  async function handleTest(b: LlmBackendConfig) {
    const name = b.name;
    if (testingMap[name]) return;
    testingMap = { ...testingMap, [name]: true };
    if (feedbackTimers[name]) {
      clearTimeout(feedbackTimers[name]);
      delete feedbackTimers[name];
    }
    const { [name]: _drop, ...rest } = testResultMap;
    testResultMap = rest;
    try {
      const result: LlmPingResult = await invoke("ping_llm_backend", { name });
      testResultMap = { ...testResultMap, [name]: result };
      if (result.available) {
        addToast(
          $t("settings.llm.test_ok_toast", { values: { name } }),
          "success",
        );
      } else {
        addToast(
          $t("settings.llm.test_failed_toast", {
            values: { name, error: result.error ?? $t("common.status.error") },
          }),
          "error",
        );
      }
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      testResultMap = {
        ...testResultMap,
        [name]: { backend: name, available: false, latency_ms: null, error: message },
      };
      addToast(
        $t("settings.llm.test_failed_toast", { values: { name, error: message } }),
        "error",
      );
    } finally {
      testingMap = { ...testingMap, [name]: false };
      feedbackTimers[name] = setTimeout(() => {
        const { [name]: _gone, ...remaining } = testResultMap;
        testResultMap = remaining;
        delete feedbackTimers[name];
      }, FEEDBACK_DURATION_MS);
      // Refresh the backend list so the persistent status tooltip
      // reflects the cached ping result once the temporary badge fades.
      void refresh();
    }
  }

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

  function statusOf(
    b: LlmBackendConfig,
  ): { kind: "connected" | "disabled" | "error"; label: string; tooltip?: string } {
    if (!b.enabled) return { kind: "disabled", label: $t("settings.llm.status_disabled") };
    if (b.last_ping_error) {
      return {
        kind: "error",
        label: $t("settings.llm.status_error"),
        tooltip: b.last_ping_error,
      };
    }
    return { kind: "connected", label: $t("settings.llm.status_configured") };
  }

  /** Ping every enabled backend in parallel so cached errors are populated. */
  async function autoPingAllEnabled() {
    const backends = $llmBackendsStore.data ?? [];
    const enabled = backends.filter((b) => b.enabled);
    if (enabled.length === 0) return;
    await Promise.all(
      enabled.map((b) =>
        invoke<LlmPingResult>("ping_llm_backend", { name: b.name }).catch(() => null),
      ),
    );
    await refresh();
  }

  onMount(async () => {
    await settingsLoaders.llmBackends();
    void autoPingAllEnabled();
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
      <ErrorBanner message={actionError} />
    {/if}

    {#if $llmBackendsStore.error}
      <ErrorBanner message={$llmBackendsStore.error} />
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
          <Card class="rounded-lg p-4 flex flex-col gap-3" data-testid="llm-backend-card-{backend.name}">
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
                <Button variant="ghost" size="sm"
                  type="button"
                  class="rounded-md p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-transparent"
                  title={$t("settings.llm.test")}
                  aria-label={$t("settings.llm.test")}
                  onclick={() => handleTest(backend)}
                  disabled={!backend.enabled || !!testingMap[backend.name]}
                  data-testid="test-backend-{backend.name}"
                >
                  <Plug class="h-4 w-4 {testingMap[backend.name] ? 'animate-pulse' : ''}" />
                </Button>
                <Button variant="ghost" size="sm"
                  type="button"
                  class="rounded-md p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground"
                  title={$t("settings.llm.edit")}
                  aria-label={$t("settings.llm.edit")}
                  onclick={() => openEdit(backend)}
                  data-testid="edit-backend-{backend.name}"
                >
                  <Pencil class="h-4 w-4" />
                </Button>
                <Button variant="ghost" size="sm"
                  type="button"
                  class="rounded-md p-1.5 text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                  title={$t("common.delete")}
                  aria-label={$t("common.delete")}
                  onclick={() => askDelete(backend)}
                  data-testid="delete-backend-{backend.name}"
                >
                  <Trash2 class="h-4 w-4" />
                </Button>
              </div>
            </div>

            <div class="flex items-center justify-between gap-2 text-xs">
              <span
                class="inline-flex items-center gap-1.5"
                title={status.tooltip ?? undefined}
                data-testid="status-{backend.name}"
              >
                {#if status.kind === "connected"}
                  <CheckCircle2 class="h-3.5 w-3.5 text-success" />
                {:else if status.kind === "disabled"}
                  <PauseCircle class="h-3.5 w-3.5 text-muted-foreground" />
                {:else}
                  <XCircle class="h-3.5 w-3.5 text-destructive" />
                {/if}
                <span class="text-muted-foreground">{status.label}</span>
              </span>
              <div class="flex items-center gap-2 shrink-0">
                {#if testResultMap[backend.name]}
                  {@const result = testResultMap[backend.name]}
                  {#if result.available}
                    <Badge variant="success" size="sm" data-testid="test-result-ok-{backend.name}">
                      OK{result.latency_ms !== null ? ` · ${result.latency_ms} ms` : ""}
                    </Badge>
                  {:else}
                    <Badge variant="danger" size="sm" class="max-w-[220px]" data-testid="test-result-err-{backend.name}">
                      <span class="truncate">{$t("common.status.error")}{result.error ? `: ${result.error}` : ""}</span>
                    </Badge>
                  {/if}
                {/if}
                {#if !backend.is_default}
                  <Button variant="ghost" size="sm"
                    type="button"
                    class="text-primary hover:underline"
                    onclick={() => handleSetDefault(backend)}
                    data-testid="set-default-{backend.name}"
                  >
                    {$t("settings.llm_set_default")}
                  </Button>
                {/if}
              </div>
            </div>
          </Card>
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
        <Input
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
