<script lang="ts">
  /**
   * AgentOverviewTab - the at-a-glance pane: tool / trigger stats, the
   * execution-mode and install-path cards (builder-only, raw internals), and
   * example prompts. Operator sees capability + examples; builder additionally
   * sees the install path and execution mode.
   */
  import { t } from "svelte-i18n";
  import { Cpu, Terminal } from "lucide-svelte";
  import { Card } from "$lib/components/operator";
  import { BuilderOnly } from "$lib/components/shared";
  import type { AgentListItem } from "$lib/types";

  interface Props {
    agent: AgentListItem;
  }

  let { agent }: Props = $props();

  const toolCount = $derived(
    agent.tools_required.length + agent.tools_optional.length,
  );
  const triggerCount = $derived(
    agent.tags.filter((tag) => tag.startsWith("trigger:")).length,
  );

  function executionModeLabel(mode: string | null): string {
    switch (mode) {
      case "direct":
        return $t("agent_detail.execution_mode_direct");
      case "orchestrated":
        return $t("agent_detail.execution_mode_orchestrated");
      default:
        return $t("agent_detail.execution_mode_auto");
    }
  }
</script>

<div class="grid grid-cols-1 gap-4 lg:grid-cols-2">
  <Card class="p-4">
    <div class="font-mono text-overline uppercase text-muted-foreground">
      {$t("agents.tab_tools")}
    </div>
    <div class="mt-1 text-display-sm tabular-nums">{toolCount}</div>
    <div class="mt-0.5 text-caption text-muted-foreground">
      {$t("agents.tools_required_optional", {
        values: {
          required: agent.tools_required.length,
          optional: agent.tools_optional.length,
        },
      })}
    </div>
  </Card>

  <Card class="p-4">
    <div class="font-mono text-overline uppercase text-muted-foreground">
      {$t("agents.triggers_stat_title")}
    </div>
    <div class="mt-1 text-display-sm tabular-nums">{triggerCount}</div>
    <div class="mt-0.5 text-caption text-muted-foreground">
      {triggerCount === 0
        ? $t("agents.triggers_stat_none")
        : $t("agents.triggers_stat_active")}
    </div>
  </Card>

  <BuilderOnly>
    {#if agent.execution_mode}
      <Card class="p-4">
        <div class="mb-1.5 flex items-center gap-2">
          <Cpu size={12} class="text-muted-foreground/50" />
          <span class="text-overline uppercase text-muted-foreground/50">
            {$t("agent_detail.execution_mode_title")}
          </span>
        </div>
        <p class="text-body-sm text-foreground/80">
          {executionModeLabel(agent.execution_mode)}
        </p>
      </Card>
    {/if}
    {#if agent.install_path}
      <Card class="p-4">
        <div class="mb-1.5 flex items-center gap-2">
          <Terminal size={12} class="text-muted-foreground/50" />
          <span class="text-overline uppercase text-muted-foreground/50">
            {$t("agent_detail.install_path_title")}
          </span>
        </div>
        <p
          class="truncate font-mono text-caption text-muted-foreground"
          title={agent.install_path}
        >
          {agent.install_path}
        </p>
      </Card>
    {/if}
  </BuilderOnly>

  {#if agent.examples.length > 0}
    <Card class="p-3.5 lg:col-span-2">
      <div
        class="mb-2 font-mono text-overline uppercase text-muted-foreground"
      >
        {$t("agents.examples_title")}
      </div>
      <ul class="m-0 list-none space-y-1 p-0">
        {#each agent.examples.slice(0, 5) as ex (ex)}
          <li class="text-body-xs text-foreground/85">- {ex}</li>
        {/each}
      </ul>
    </Card>
  {/if}
</div>
