<script lang="ts">
  /**
   * McpToolsTab - the exposed-tools list with a canon Disclosure per tool.
   *
   * Replaces the native `<details>` (which snaps in WKWebView) with the Phase-0
   * `Disclosure`; the tool name is the summary, the JSON input schema the body.
   */
  import { t } from "svelte-i18n";
  import { Card } from "$lib/components/ui/card";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import type { McpToolSummaryView } from "$lib/types";

  interface Props {
    tools: McpToolSummaryView[];
  }

  let { tools }: Props = $props();
</script>

<Card class="max-w-3xl px-4 py-3.5">
  <div class="mb-2.5 flex items-center justify-between">
    <span class="text-label-md font-semibold text-foreground">{$t("connections.tools_exposed")}</span>
    <span class="font-mono text-caption text-muted-foreground">{tools.length}</span>
  </div>
  {#if tools.length === 0}
    <p class="py-4 text-center text-caption text-muted-foreground">
      {$t("integrations.manage.tools_empty")}
    </p>
  {:else}
    <ul class="space-y-1.5" data-testid="mcp-tools-list">
      {#each tools as tool (tool.full_name)}
        <li data-testid={`mcp-tool-${tool.local_name}`}>
          <Disclosure testid={`mcp-tool-disc-${tool.local_name}`}>
            {#snippet summary()}
              <span class="flex min-w-0 flex-col gap-0.5">
                <span class="truncate font-mono text-body-xs text-foreground">{tool.local_name}</span>
                {#if tool.description}
                  <span class="line-clamp-2 text-caption text-muted-foreground">{tool.description}</span>
                {/if}
              </span>
            {/snippet}
            {#snippet meta()}
              <span class="text-caption text-muted-foreground/70">{$t("connections.tool_schema")}</span>
            {/snippet}
            {#snippet children()}
              <pre
                class="overflow-auto rounded bg-muted/40 p-2 font-mono text-caption leading-relaxed text-muted-foreground">{JSON.stringify(
                  tool.input_schema,
                  null,
                  2,
                )}</pre>
            {/snippet}
          </Disclosure>
        </li>
      {/each}
    </ul>
  {/if}
</Card>
