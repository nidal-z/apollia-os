<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import { agents as agentsStore } from "$lib/stores/sse";
  import type { AgentListItem } from "$lib/types";

  interface Props {
    onAgentStarted: (agentId: string, agentName: string) => void;
    onSkip: () => void;
  }

  let { onAgentStarted, onSkip }: Props = $props();

  let helloAgentPath = $state<string | null>(null);
  let selectedPath = $state<string | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let started = $state(false);
  let agentId = $state<string | null>(null);
  let agentName = $state<string | null>(null);

  /** Resolve agent display name from the agents store, falling back to path stem. */
  function resolveAgentName(id: string, path: string): string {
    const fromStore = $agentsStore.find((a: AgentListItem) => a.id === id);
    if (fromStore?.name) return fromStore.name;
    const stem = path.split("/").pop()?.replace(".py", "") ?? "agent";
    return stem
      .replace(/[-_]/g, " ")
      .replace(/\b\w/g, (c) => c.toUpperCase());
  }

  async function checkHelloAgent() {
    try {
      const result = await invoke<string | null>("check_hello_agent_exists");
      helloAgentPath = result;
    } catch {
      helloAgentPath = null;
    }
  }

  async function pickFile() {
    const file = await open({
      title: "Select a Python agent file",
      filters: [{ name: "Python", extensions: ["py"] }],
      multiple: false,
    });
    if (file) {
      selectedPath = file as string;
      await startAgent(selectedPath);
    }
  }

  async function useHelloAgent() {
    if (helloAgentPath) {
      selectedPath = helloAgentPath;
      await startAgent(helloAgentPath);
    }
  }

  async function startAgent(path: string) {
    loading = true;
    error = null;
    try {
      const id = await invoke<string>("start_agent", { path });
      agentId = id;
      agentName = resolveAgentName(id, path);
      started = true;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  async function retry() {
    if (selectedPath) {
      await startAgent(selectedPath);
    }
  }

  onMount(() => {
    checkHelloAgent();
  });
</script>

<div class="space-y-6" data-testid="step-first-agent">
  <div>
    <h2 class="text-xl font-semibold">{$t('onboarding.agent_title')}</h2>
    <p class="mt-1 text-sm text-muted-foreground">
      {$t('onboarding.agent_subtitle', { values: { manifest: 'manifest()', run: 'run()', py: '.py' } })}
    </p>
  </div>

  {#if !started}
    <div class="flex flex-col items-center gap-4 rounded-lg glass-border border-dashed glass-card p-8">
      <Button onclick={pickFile} disabled={loading}>
        {loading ? $t('onboarding.starting_agent') : $t('onboarding.choose_file')}
      </Button>

      {#if helloAgentPath}
        <div class="text-center">
          <p class="mb-2 text-xs text-muted-foreground">{$t('common.or')}</p>
          <Button variant="outline" size="sm" onclick={useHelloAgent} disabled={loading}>
            {$t('onboarding.use_hello_agent')}
          </Button>
        </div>
      {/if}
    </div>
  {/if}

  {#if loading}
    <div class="flex items-center gap-3 rounded-lg glass-border glass-card p-4">
      <span class="inline-block h-5 w-5 animate-spin rounded-full border-2 border-primary border-t-transparent"></span>
      <p class="text-sm text-muted-foreground">{$t('onboarding.starting_agent')}</p>
    </div>
  {/if}

  {#if error}
    <div class="rounded-lg border border-[hsl(var(--destructive))] bg-[hsl(var(--destructive))]/10 p-4">
      <p class="text-sm text-[hsl(var(--destructive))]">{error}</p>
      <Button variant="outline" size="sm" class="mt-3" onclick={retry}>{$t('common.retry')}</Button>
    </div>
  {/if}

  {#if started && agentId}
    <div class="flex items-center gap-3 rounded-lg border border-[var(--apollia-success)] bg-[var(--apollia-success)]/10 p-4">
      <span class="text-lg text-[var(--apollia-success)]">&#10003;</span>
      <div>
        <p class="text-sm font-medium">{$t('onboarding.agent_started')}</p>
        <p class="text-sm font-semibold text-foreground">{agentName}</p>
        <p class="text-xs text-muted-foreground font-mono">{agentId}</p>
      </div>
    </div>
  {/if}

  <div class="flex justify-end gap-3">
    <Button variant="ghost" onclick={onSkip}>{$t('common.skip')}</Button>
    {#if started && agentId && agentName}
      <Button onclick={() => onAgentStarted(agentId ?? "", agentName ?? "")}>{$t('common.continue')}</Button>
    {/if}
  </div>
</div>
