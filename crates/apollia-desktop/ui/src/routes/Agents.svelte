<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { t } from "svelte-i18n";
  import { agents } from "$lib/stores/agents";
  import { Button } from "$lib/components/ui/button";
  import AgentCard from "../components/agents/AgentCard.svelte";
  import AgentLogs from "../components/agents/AgentLogs.svelte";
  import MacSandboxBanner from "../components/common/MacSandboxBanner.svelte";

  let startingAgent = $state(false);
  let startError = $state<string | null>(null);

  let logsAgentId = $state<string | null>(null);
  let logsOpen = $state(false);

  async function pickAndStartAgent() {
    startError = null;
    try {
      const path = await openDialog({
        filters: [{ name: "Python Agent", extensions: ["py"] }],
        multiple: false,
      });
      if (!path) return;

      startingAgent = true;
      await invoke("start_agent", { path });
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      startError = message;
    } finally {
      startingAgent = false;
    }
  }

  function openLogs(agentId: string) {
    logsAgentId = agentId;
    logsOpen = true;
  }

  function closeLogs() {
    logsOpen = false;
  }
</script>

<div class="space-y-4">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <h1 class="text-2xl font-bold" data-testid="agents-header">{$t('agents.title')}</h1>
    <Button onclick={pickAndStartAgent} disabled={startingAgent} data-testid="register-agent-btn">
      {startingAgent ? $t('agents.starting') : $t('agents.register')}
    </Button>
  </div>

  <MacSandboxBanner />

  {#if startError}
    <div class="rounded-md border border-[hsl(var(--destructive))] bg-[hsl(var(--destructive))]/10 px-4 py-2 text-sm text-[hsl(var(--destructive))]">
      {startError}
    </div>
  {/if}

  <!-- Agent list or empty state -->
  {#if $agents.length === 0}
    <div class="flex flex-col items-center justify-center gap-4 rounded-lg border border-dashed py-16">
      <p class="text-muted-foreground">{$t('agents.empty')}</p>
      <Button variant="outline" onclick={pickAndStartAgent} disabled={startingAgent}>
        {$t('agents.empty_cta')}
      </Button>
    </div>
  {:else}
    <div class="grid gap-4 sm:grid-cols-1 md:grid-cols-2 lg:grid-cols-3" data-testid="agents-grid">
      {#each $agents as agent (agent.id)}
        <AgentCard {agent} onlogs={openLogs} />
      {/each}
    </div>
  {/if}
</div>

<!-- Logs drawer -->
{#if logsAgentId}
  <AgentLogs agentId={logsAgentId} open={logsOpen} onclose={closeLogs} />
{/if}
