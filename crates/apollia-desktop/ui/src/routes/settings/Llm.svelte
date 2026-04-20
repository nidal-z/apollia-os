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
  import { Button } from "$lib/components/ui/button";
  import SettingSectionSkeleton from "../../components/settings/SettingSectionSkeleton.svelte";
  import { llmBackendsStore, settingsLoaders } from "$lib/stores/settings";
  import type { LlmBackendConfig } from "$lib/types";

  let actionError = $state<string | null>(null);
  let showAddForm = $state(false);
  let addSaving = $state(false);
  let newBackend = $state({
    name: "",
    provider: "llama-cpp" as LlmBackendConfig["provider"],
    model: "",
    config_json: "{}",
    enabled: true,
    is_default: false,
  });

  async function refresh() {
    await settingsLoaders.llmBackends(true);
  }

  async function handleDelete(name: string) {
    actionError = null;
    try {
      await invoke("delete_llm_backend", { name });
      await refresh();
    } catch (err) {
      actionError = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleSetDefault(name: string) {
    actionError = null;
    try {
      await invoke("set_default_llm_backend", { name });
      await refresh();
    } catch (err) {
      actionError = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleAdd() {
    actionError = null;
    addSaving = true;
    try {
      let parsedConfig: Record<string, unknown> = {};
      try {
        parsedConfig = JSON.parse(newBackend.config_json);
      } catch {
        actionError = "config_json must be valid JSON";
        return;
      }
      await invoke("create_llm_backend", {
        payload: {
          name: newBackend.name,
          provider: newBackend.provider,
          model: newBackend.model,
          config_json: parsedConfig,
          enabled: newBackend.enabled,
          is_default: newBackend.is_default,
        },
      });
      showAddForm = false;
      newBackend = { name: "", provider: "llama-cpp", model: "", config_json: "{}", enabled: true, is_default: false };
      await refresh();
    } catch (err) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      addSaving = false;
    }
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
      <p class="text-sm text-muted-foreground">{$t('settings.llm_backends_subtitle')}</p>
      <Button size="sm" onclick={() => { showAddForm = !showAddForm; actionError = null; }} data-testid="add-backend-btn">
        {showAddForm ? $t('common.cancel') : $t('settings.llm_add_backend')}
      </Button>
    </div>

    {#if actionError}
      <div class="rounded-md border border-destructive/30 bg-destructive/5 px-4 py-2 text-sm text-destructive">{actionError}</div>
    {/if}

    {#if showAddForm}
      <div class="glass-card glass-border rounded-lg p-4 space-y-4" data-testid="add-backend-form">
        <h3 class="text-sm font-medium uppercase tracking-wider text-muted-foreground">{$t('settings.llm_new_backend')}</h3>
        <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
          <div class="space-y-1.5">
            <label class="text-sm text-muted-foreground" for="backend-name">{$t('settings.llm_backend_name')}</label>
            <input id="backend-name" type="text" placeholder="local-code" bind:value={newBackend.name}
              class="flex h-9 w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm font-mono focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
              data-testid="backend-name-input" />
          </div>
          <div class="space-y-1.5">
            <label class="text-sm text-muted-foreground" for="backend-provider">{$t('settings.llm_backend_provider')}</label>
            <select id="backend-provider" bind:value={newBackend.provider}
              class="flex h-9 w-full appearance-none rounded-md border border-border bg-background px-3 py-1.5 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
              data-testid="backend-provider-select">
              <option value="llama-cpp">llama-cpp (local)</option>
              <option value="openai">OpenAI</option>
              <option value="mistral">Mistral</option>
              <option value="anthropic">Anthropic</option>
              <option value="ollama">Ollama</option>
            </select>
          </div>
        </div>
        <div class="space-y-1.5">
          <label class="text-sm text-muted-foreground" for="backend-model">{$t('settings.llm_backend_model')}</label>
          <input id="backend-model" type="text" placeholder="qwen3-0.6b-q8_0" bind:value={newBackend.model}
            class="flex h-9 w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm font-mono focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
            data-testid="backend-model-input" />
        </div>
        <div class="space-y-1.5">
          <label class="text-sm text-muted-foreground" for="backend-config">{$t('settings.llm_backend_config_json')}</label>
          <textarea id="backend-config" rows={3} bind:value={newBackend.config_json}
            class="flex w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm font-mono focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
            data-testid="backend-config-input"></textarea>
        </div>
        <div class="flex items-center gap-6">
          <label class="flex cursor-pointer items-center gap-2">
            <button type="button" role="switch" aria-checked={newBackend.enabled} aria-label={$t('settings.llm_backend_enabled')}
              onclick={() => { newBackend.enabled = !newBackend.enabled; }}
              class="relative inline-flex h-5 w-9 items-center rounded-full transition-colors {newBackend.enabled ? 'bg-primary' : 'bg-muted'}">
              <span class="inline-block h-3.5 w-3.5 rounded-full bg-white shadow-sm transition-transform {newBackend.enabled ? 'translate-x-4.5' : 'translate-x-0.5'}"></span>
            </button>
            <span class="text-sm text-muted-foreground">{$t('settings.llm_backend_enabled')}</span>
          </label>
          <label class="flex cursor-pointer items-center gap-2">
            <button type="button" role="switch" aria-checked={newBackend.is_default} aria-label={$t('settings.llm_backend_default')}
              onclick={() => { newBackend.is_default = !newBackend.is_default; }}
              class="relative inline-flex h-5 w-9 items-center rounded-full transition-colors {newBackend.is_default ? 'bg-primary' : 'bg-muted'}">
              <span class="inline-block h-3.5 w-3.5 rounded-full bg-white shadow-sm transition-transform {newBackend.is_default ? 'translate-x-4.5' : 'translate-x-0.5'}"></span>
            </button>
            <span class="text-sm text-muted-foreground">{$t('settings.llm_backend_default')}</span>
          </label>
        </div>
        <Button onclick={handleAdd} disabled={addSaving || !newBackend.name || !newBackend.model} data-testid="add-backend-save-btn">
          {addSaving ? $t('common.saving') : $t('settings.llm_add_backend')}
        </Button>
      </div>
    {/if}

    {#if $llmBackendsStore.error}
      <div class="rounded-md border border-destructive/30 bg-destructive/5 px-4 py-2 text-sm text-destructive">{$llmBackendsStore.error}</div>
    {:else if ($llmBackendsStore.data ?? []).length === 0}
      <div class="rounded-lg border border-dashed border-border px-4 py-8 text-center text-sm text-muted-foreground" data-testid="llm-backends-empty">
        {$t('settings.llm_no_backends')}
      </div>
    {:else}
      <div class="space-y-2" data-testid="llm-backends-list">
        {#each ($llmBackendsStore.data ?? []) as backend (backend.name)}
          <div class="glass-card glass-border flex items-center justify-between rounded-lg px-4 py-3" data-testid="llm-backend-row-{backend.name}">
            <div class="flex items-center gap-3">
              <div>
                <div class="flex items-center gap-2">
                  <span class="text-sm font-mono font-medium">{backend.name}</span>
                  {#if backend.is_default}
                    <span class="inline-flex rounded-full bg-primary/10 px-2 py-0.5 text-xs font-medium text-primary">default</span>
                  {/if}
                  {#if !backend.enabled}
                    <span class="inline-flex rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground">disabled</span>
                  {/if}
                </div>
                <p class="text-xs text-muted-foreground">{backend.provider} · {backend.model}</p>
              </div>
            </div>
            <div class="flex items-center gap-2">
              {#if !backend.is_default}
                <Button size="sm" variant="outline" onclick={() => handleSetDefault(backend.name)} data-testid="set-default-{backend.name}">
                  {$t('settings.llm_set_default')}
                </Button>
                <Button size="sm" variant="destructive" onclick={() => handleDelete(backend.name)} data-testid="delete-backend-{backend.name}">
                  {$t('common.delete')}
                </Button>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </section>
{/if}
