<script lang="ts">
  /**
   * McpToolsTab - the tools a connection offers.
   *
   * Developer mode off: each tool by its name and description, nothing more.
   * On: the provenance of each tool (the server it comes from, its full name)
   * and its raw input schema in a canon Disclosure (the native `<details>`
   * snaps in WKWebView).
   */
  import { t } from "svelte-i18n";
  import { Card } from "$lib/components/ui/card";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import { developerMode } from "$lib/stores/developerMode";
  import type { Connection, McpToolSummaryView } from "$lib/types";
  import ToolFamilyControl from "./ToolFamilyControl.svelte";

  interface Props {
    tools: McpToolSummaryView[];
    /** The server the tools come from, named in developer mode. */
    serverName: string;
    /** The connection the tools come from, whose list holds their family. */
    connection?: Connection;
    /** A tool was given another family: the connection as the list now shows it. */
    onupdated?: (connection: Connection) => void;
  }

  let { tools, serverName, connection, onupdated }: Props = $props();
</script>

<Card class="max-w-3xl px-4 py-3.5">
  <div class="mb-2.5 flex items-center justify-between">
    <span class="text-label-md font-semibold text-foreground">{$t("connections.tools_exposed")}</span>
    <span class="text-caption text-muted-foreground">{tools.length}</span>
  </div>
  {#if tools.length === 0}
    <p class="py-4 text-center text-caption text-muted-foreground">
      {$t("integrations.manage.tools_empty")}
    </p>
  {:else}
    <ul class="space-y-1.5" data-testid="mcp-tools-list">
      {#each tools as tool (tool.full_name)}
        <li data-testid={`mcp-tool-${tool.local_name}`}>
          {#if $developerMode}
            <Disclosure testid={`mcp-tool-disc-${tool.local_name}`}>
              {#snippet summary()}
                <span class="flex min-w-0 flex-col gap-0.5">
                  <span class="truncate font-mono text-body-xs text-foreground">{tool.local_name}</span>
                  <span
                    class="truncate font-mono text-caption text-muted-foreground/80"
                    data-testid={`mcp-tool-provenance-${tool.local_name}`}
                  >
                    {$t("dev.connections.tool_provenance", {
                      values: { server: serverName, name: tool.full_name },
                    })}
                  </span>
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
                  class="overflow-auto rounded bg-muted/40 p-2 font-mono text-caption leading-relaxed text-muted-foreground"
                  data-testid={`mcp-tool-schema-${tool.local_name}`}>{JSON.stringify(
                    tool.input_schema,
                    null,
                    2,
                  )}</pre>
              {/snippet}
            </Disclosure>
            {#if connection}
              <div class="px-2 pb-1">
                <ToolFamilyControl {connection} toolId={`${connection.id}/${tool.local_name}`} {onupdated} />
              </div>
            {/if}
          {:else}
            <div class="flex min-w-0 flex-col gap-0.5 rounded-md px-2 py-1.5">
              <span class="truncate text-body-xs text-foreground">{tool.local_name}</span>
              {#if tool.description}
                <span class="line-clamp-2 text-caption text-muted-foreground">{tool.description}</span>
              {/if}
              {#if connection}
                <ToolFamilyControl {connection} toolId={`${connection.id}/${tool.local_name}`} {onupdated} />
              {/if}
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</Card>
