<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { t } from "svelte-i18n";
  import type { AgentStatus } from "$lib/types";
  import { agents } from "$lib/stores/agents";
  import { Button } from "$lib/components/ui/button";
  import { Bot } from "lucide-svelte";
  import { uiMode } from "$lib/stores/mode";
  import AgentCard from "../components/agents/AgentCard.svelte";
  import AgentLogs from "../components/agents/AgentLogs.svelte";
  import AgentDetail from "../components/agents/AgentDetail.svelte";
  import MacSandboxBanner from "../components/common/MacSandboxBanner.svelte";
  import EmptyState from "../components/common/EmptyState.svelte";

  let startingAgent = $state(false);
  let startError = $state<string | null>(null);

  let logsAgentId = $state<string | null>(null);
  let logsOpen = $state(false);

  let detailAgent = $state<AgentStatus | null>(null);
  let detailOpen = $state(false);

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

  function openDetail(agent: AgentStatus) {
    detailAgent = agent;
    detailOpen = true;
  }

  function closeDetail() {
    detailOpen = false;
  }

  function openLogsFromDetail(agentId: string) {
    closeDetail();
    openLogs(agentId);
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
    <EmptyState
      icon={Bot}
      title={$t('agents.empty_title')}
      ctaLabel={$uiMode === "operator" ? $t('agents.empty_cta_operator') : $t('agents.empty_cta_builder')}
      ctaAction={pickAndStartAgent}
    />
  {:else}
    <div class="grid gap-4 sm:grid-cols-1 md:grid-cols-2 lg:grid-cols-3" data-testid="agents-grid">
      {#each $agents as agent (agent.id)}
        <AgentCard {agent} onlogs={openLogs} ondetail={openDetail} />
      {/each}
    </div>
  {/if}
</div>

<!-- Logs drawer -->
{#if logsAgentId}
  <AgentLogs agentId={logsAgentId} open={logsOpen} onclose={closeLogs} />
{/if}

<!-- Agent detail sheet -->
{#if detailAgent}
  <AgentDetail agent={detailAgent} open={detailOpen} onclose={closeDetail} onlogs={openLogsFromDetail} />
{/if}
