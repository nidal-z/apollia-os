<script lang="ts">
  /**
   * SheetTechnical - the internals of the agent, in developer mode only:
   * how it executes, where it is installed, the definition it resolves from
   * and which of its fields the user overrode, and the messages it exchanged
   * with other agents.
   */
  import { t } from "svelte-i18n";
  import { DeveloperOnly } from "$lib/components/shared";
  import type { AgentListItem } from "$lib/types";
  import type { ResolvedAgent } from "$lib/ipc/agentSheet";
  import AgentMessagesPanel from "../AgentMessagesPanel.svelte";
  import SheetSection from "./SheetSection.svelte";

  interface Props {
    agent: AgentListItem;
    definition: ResolvedAgent | null;
  }

  let { agent, definition }: Props = $props();

  const rowLabel = "text-muted-foreground";
  const rowValue = "m-0 break-all font-mono text-foreground/85";
</script>

<DeveloperOnly>
  <SheetSection
    title={$t("agent_sheet.technical_title")}
    legend={$t("agent_sheet.technical_legend")}
    testid="agent-sheet-technical"
  >
    <dl class="m-0 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-caption">
      <dt class={rowLabel}>{$t("agent_sheet.technical_execution_mode")}</dt>
      <dd class={rowValue}>{agent.execution_mode ?? "auto"}</dd>
      {#if definition}
        <dt class={rowLabel}>{$t("agent_sheet.technical_origin")}</dt>
        <dd class={rowValue}>{definition.origin}{definition.package ? ` · ${definition.package}` : ""} · {definition.version}</dd>
        <dt class={rowLabel}>{$t("agent_sheet.technical_autonomy")}</dt>
        <dd class={rowValue}>{definition.autonomy}</dd>
        <dt class={rowLabel}>{$t("agent_sheet.technical_overridden")}</dt>
        <dd class={rowValue}>{definition.overridden.length > 0 ? definition.overridden.join(", ") : "-"}</dd>
      {/if}
    </dl>
    {#if agent.supports_a2a}
      <div class="mt-3">
        <div class="mb-2 text-caption font-semibold text-muted-foreground">{$t("agent_detail.messages_title")}</div>
        <AgentMessagesPanel agentName={agent.name} />
      </div>
    {/if}
  </SheetSection>
</DeveloperOnly>
