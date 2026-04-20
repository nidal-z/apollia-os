<script lang="ts">
  /**
   * MCP tool call preview (US-SP42-054). Renders the JSON-RPC
   * `input_schema` as a humanized parameter table with descriptions
   * and shows a concrete example call.
   */
  import { t } from "svelte-i18n";
  import type { McpPreview } from "./types";

  interface Props {
    payload: McpPreview;
  }

  let { payload }: Props = $props();
</script>

<div class="space-y-3" data-testid="preview-mcp">
  <header class="flex items-center gap-2 text-xs">
    <span class="font-semibold">{payload.tool_name}</span>
    {#if payload.server_name}
      <span class="text-muted-foreground">· {payload.server_name}</span>
    {/if}
  </header>

  {#if payload.input_schema_parameters.length > 0}
    <section>
      <h4 class="mb-1 text-[11px] uppercase tracking-wide text-muted-foreground">
        {$t("permissions.preview.mcp.parameters")}
      </h4>
      <table class="w-full text-[12px]">
        <thead class="text-left text-[11px] uppercase tracking-wide text-muted-foreground">
          <tr>
            <th class="py-1 pr-2">{$t("permissions.preview.mcp.col_name")}</th>
            <th class="py-1 pr-2">{$t("permissions.preview.mcp.col_type")}</th>
            <th class="py-1 pr-2">{$t("permissions.preview.mcp.col_required")}</th>
            <th class="py-1">{$t("permissions.preview.mcp.col_description")}</th>
          </tr>
        </thead>
        <tbody class="align-top">
          {#each payload.input_schema_parameters as p (p.name)}
            <tr class="border-t border-border">
              <td class="py-1 pr-2 font-mono">{p.name}</td>
              <td class="py-1 pr-2 text-muted-foreground">{p.type}</td>
              <td class="py-1 pr-2">
                {#if p.required}
                  <span class="rounded bg-destructive/10 px-1.5 text-[10px] font-semibold text-destructive">
                    {$t("permissions.preview.mcp.required")}
                  </span>
                {:else}
                  <span class="text-muted-foreground">—</span>
                {/if}
              </td>
              <td class="py-1 text-muted-foreground">{p.description ?? ""}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </section>
  {/if}

  {#if payload.output_schema_summary}
    <section>
      <h4 class="mb-1 text-[11px] uppercase tracking-wide text-muted-foreground">
        {$t("permissions.preview.mcp.output")}
      </h4>
      <p class="text-[12px] text-muted-foreground">{payload.output_schema_summary}</p>
    </section>
  {/if}

  {#if payload.example_call}
    <section>
      <h4 class="mb-1 text-[11px] uppercase tracking-wide text-muted-foreground">
        {$t("permissions.preview.mcp.example")}
      </h4>
      <pre class="overflow-x-auto rounded bg-muted px-3 py-2 text-[11px]">{JSON.stringify(payload.example_call, null, 2)}</pre>
    </section>
  {/if}
</div>
