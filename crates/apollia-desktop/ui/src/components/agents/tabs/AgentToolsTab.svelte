<script lang="ts">
  /**
   * AgentToolsTab - the declared tools list. Each tool shows its scope, whether
   * it is required or optional, and a warning badge when it is sensitive
   * (writes, sends, or deletes) and will ask for confirmation on each call.
   */
  import { t } from "svelte-i18n";
  import { Zap } from "lucide-svelte";
  import { Card } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import type { AgentListItem } from "$lib/types";

  interface Props {
    agent: AgentListItem;
  }

  let { agent }: Props = $props();

  const tools = $derived([
    ...agent.tools_required.map((id) => ({ id, required: true })),
    ...agent.tools_optional.map((id) => ({ id, required: false })),
  ]);

  function isSensitive(id: string): boolean {
    return (
      id.startsWith("fs.write") || id.includes("send") || id.includes("delete")
    );
  }

  function scopeOf(id: string): string {
    return id.split(/[._]/)[0] || "tool";
  }
</script>

<Card class="p-3.5">
  <div class="mb-2.5 flex items-center justify-between">
    <span class="text-body-xs font-semibold text-foreground">
      {$t("agents.available_tools")}
    </span>
    <span class="font-mono text-caption text-muted-foreground">{tools.length}</span>
  </div>
  {#if tools.length === 0}
    <div class="py-6 text-center text-caption text-muted-foreground">
      {$t("agents.no_tool_declared")}
    </div>
  {:else}
    {#each tools as tool, i (tool.id)}
      <div
        class="flex items-center gap-2.5 py-2 {i === tools.length - 1
          ? ''
          : 'border-b border-border/40'}"
      >
        <span
          class="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary"
        >
          <Zap size={11} />
        </span>
        <div class="min-w-0 flex-1">
          <div class="truncate font-mono text-body-xs font-medium text-foreground">
            {tool.id}
          </div>
          <div class="text-caption text-muted-foreground">
            {tool.required ? $t("agents.tool_required") : $t("agents.tool_optional")}
          </div>
        </div>
        <Badge size="sm" variant="neutral">{scopeOf(tool.id)}</Badge>
        {#if isSensitive(tool.id)}
          <Badge size="sm" variant="warning">{$t("agents.tool_ask_each_time")}</Badge>
        {/if}
      </div>
    {/each}
  {/if}
</Card>
