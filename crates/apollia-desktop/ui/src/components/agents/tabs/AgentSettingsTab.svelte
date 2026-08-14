<script lang="ts">
  /**
   * AgentSettingsTab - runtime controls (start / stop with a two-step confirm,
   * logs), the auto-start toggle, the LLM info panel, and the A2A message log
   * (builder-only). Owns its own action state and talks to the IPC layer
   * directly; the sidebar's quick toggle lives in useAgentActions instead.
   */
  import { t } from "svelte-i18n";
  import { MessageSquare, Play, Square } from "lucide-svelte";
  import { Card, ErrorBanner } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { Toggle } from "$lib/components/ui/toggle";
  import { BuilderOnly } from "$lib/components/shared";
  import { reportError } from "$lib/errors/reportError";
  import {
    disableAgent,
    enableAgent,
    startAgent,
    stopAgent,
  } from "$lib/ipc/agents";
  import { isActive } from "../agentStatus";
  import { showStartButton } from "./startVisibility";
  import AgentLlmInfo from "../AgentLlmInfo.svelte";
  import AgentMessagesPanel from "../AgentMessagesPanel.svelte";
  import type { AgentListItem } from "$lib/types";

  interface Props {
    agent: AgentListItem;
    onOpenLogs: (agentId: string) => void;
  }

  let { agent, onOpenLogs }: Props = $props();

  let confirmStopVisible = $state(false);
  let stoppingAgent = $state(false);
  let startLoading = $state(false);
  let toggleLoading = $state(false);
  let errorMessage = $state<string | null>(null);

  $effect(() => {
    void agent.name;
    confirmStopVisible = false;
    errorMessage = null;
  });

  const installed = $derived(agent.installed_at !== null);
  const running = $derived(isActive(agent));
  const startVisible = $derived(showStartButton(agent));

  async function handleStart(): Promise<void> {
    if (!agent.install_path) return;
    startLoading = true;
    errorMessage = null;
    try {
      await startAgent(agent.install_path);
    } catch (err) {
      errorMessage = reportError(err, { surface: "inline" }).friendly_message;
    } finally {
      startLoading = false;
    }
  }

  async function handleStop(): Promise<void> {
    if (!agent.id) return;
    confirmStopVisible = false;
    stoppingAgent = true;
    errorMessage = null;
    try {
      await stopAgent(agent.id);
    } catch (err) {
      errorMessage = reportError(err, { surface: "inline" }).friendly_message;
    } finally {
      stoppingAgent = false;
    }
  }

  async function handleToggleEnabled(): Promise<void> {
    toggleLoading = true;
    errorMessage = null;
    try {
      if (agent.enabled) await disableAgent(agent.name);
      else await enableAgent(agent.name);
    } catch (err) {
      errorMessage = reportError(err, { surface: "inline" }).friendly_message;
    } finally {
      toggleLoading = false;
    }
  }
</script>

<div class="max-w-3xl space-y-4" data-testid="agent-detail-settings">
  {#if errorMessage}
    <ErrorBanner message={errorMessage} />
  {/if}

  <Card class="p-3.5">
    <div class="mb-3 font-mono text-overline uppercase text-muted-foreground">
      {$t("agents.execution_section")}
    </div>
    {#if confirmStopVisible}
      <div class="flex flex-wrap items-center gap-2">
        <span class="text-body-xs text-muted-foreground">{$t("agents.stop_confirm")}</span>
        <Button
          size="sm"
          variant="destructive"
          onclick={handleStop}
          disabled={stoppingAgent}
          data-testid="agent-detail-stop-confirm"
        >
          {stoppingAgent ? $t("agents.stopping") : $t("common.confirm")}
        </Button>
        <Button size="sm" variant="outline" onclick={() => (confirmStopVisible = false)}>
          {$t("common.cancel")}
        </Button>
      </div>
    {:else}
      <div class="flex flex-wrap items-center gap-2">
        {#if startVisible}
          <Button
            size="sm"
            variant="primary-solid"
            onclick={handleStart}
            disabled={startLoading}
            data-testid="agent-detail-start-btn"
          >
            {#snippet icon()}<Play size={12} />{/snippet}
            {startLoading ? $t("agents.starting_agent") : $t("agents.start")}
          </Button>
        {/if}
        {#if running && agent.id}
          <Button
            size="sm"
            variant="outline"
            onclick={() => (confirmStopVisible = true)}
            data-testid="agent-detail-stop-btn"
          >
            {#snippet icon()}<Square size={12} />{/snippet}
            {$t("agents.stop")}
          </Button>
        {/if}
        {#if agent.id}
          {@const agentId = agent.id}
          <Button
            size="sm"
            variant="ghost"
            onclick={() => onOpenLogs(agentId)}
            data-testid="agent-detail-logs-btn"
          >
            {$t("agents.logs")}
          </Button>
        {/if}
      </div>
    {/if}

    {#if installed}
      <div class="mt-3 flex items-center justify-between gap-3 border-t border-border/40 pt-3">
        <div>
          <div class="text-body-xs font-medium text-foreground">
            {$t("agents.auto_start_enabled")}
          </div>
          <div class="mt-0.5 text-caption text-muted-foreground">
            {$t("agents.auto_start_hint")}
          </div>
        </div>
        <Toggle
          checked={agent.enabled}
          onchange={handleToggleEnabled}
          disabled={toggleLoading}
          size="sm"
          aria-label={$t("agents.auto_start_enabled")}
          data-testid="agent-detail-enabled-toggle"
        />
      </div>
    {/if}
  </Card>

  <Card class="p-3.5">
    <AgentLlmInfo />
  </Card>

  {#if agent.supports_a2a}
    <BuilderOnly>
      <Card class="p-3.5">
        <div class="mb-3 flex items-center gap-2">
          <MessageSquare size={12} class="text-muted-foreground/50" />
          <span class="text-overline uppercase text-muted-foreground/50">
            {$t("agent_detail.messages_title")}
          </span>
        </div>
        <AgentMessagesPanel agentName={agent.name} />
      </Card>
    </BuilderOnly>
  {/if}
</div>
