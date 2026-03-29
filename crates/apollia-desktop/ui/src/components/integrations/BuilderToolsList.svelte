<script lang="ts">
  import { t } from "svelte-i18n";
  import type { McpToolSummaryView } from "$lib/types";

  interface Props {
    tools: McpToolSummaryView[];
  }

  let { tools }: Props = $props();
</script>

{#if tools.length === 0}
  <p class="py-4 text-sm text-muted-foreground" data-testid="tools-empty">
    {$t("integrations.builder.detail.no_tools")}
  </p>
{:else}
  <ul class="flex flex-col gap-2" data-testid="tools-list">
    {#each tools as tool (tool.full_name)}
      <li class="glass-card glass-border rounded-lg px-3 py-2.5 flex flex-col gap-1">
        <span
          class="font-mono text-xs font-medium text-foreground"
          data-testid="tool-name"
        >
          {tool.full_name}
        </span>

        {#if tool.description}
          <p class="text-xs text-muted-foreground" data-testid="tool-description">
            {tool.description}
          </p>
        {/if}

        <details class="mt-1">
          <summary
            class="cursor-pointer select-none text-xs text-muted-foreground hover:text-foreground"
          >
            {$t("integrations.builder.detail.schema_toggle")}
          </summary>
          <pre
            class="mt-1.5 overflow-x-auto rounded bg-muted px-3 py-2 text-[11px] leading-relaxed text-muted-foreground"
            data-testid="tool-schema"
          >{JSON.stringify(tool.input_schema, null, 2)}</pre>
        </details>
      </li>
    {/each}
  </ul>
{/if}
