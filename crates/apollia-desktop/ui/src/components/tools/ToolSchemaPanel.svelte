<!--
  Side panel for tool schema introspection.

  Displays the full ToolDescriptor (name, version, kind, description,
  input/output JSON schemas, permissions) when an operator or builder
  clicks on a tool in the Memory/Tools list.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { uiMode } from "$lib/stores/mode";
  import type { ToolDescriptorView } from "$lib/types";
  import { Sheet } from "$lib/components/ui/sheet";
  import { Badge } from "$lib/components/ui/badge";
  import JsonViewer from "../common/JsonViewer.svelte";
  import EmptyState from "../common/EmptyState.svelte";
  import { FileQuestion } from "lucide-svelte";
  import { LoadingSpinner } from "$lib/components/feedback";

  interface Props {
    toolName: string;
    open: boolean;
    onclose: () => void;
  }

  let { toolName, open, onclose }: Props = $props();

  let descriptor = $state<ToolDescriptorView | null>(null);
  let loading = $state(true);
  let loadError = $state(false);

  const isBuilder = $derived($uiMode === "builder");

  const panelTitle = $derived(
    isBuilder
      ? $t("memory.tool_schema.title_builder")
      : $t("memory.tool_schema.title_operator"),
  );

  const inputLabel = $derived(
    isBuilder
      ? $t("memory.tool_schema.input_label_builder")
      : $t("memory.tool_schema.input_label_operator"),
  );

  const outputLabel = $derived(
    isBuilder
      ? $t("memory.tool_schema.output_label_builder")
      : $t("memory.tool_schema.output_label_operator"),
  );

  const permissionsLabel = $derived(
    isBuilder
      ? $t("memory.tool_schema.permissions_label_builder")
      : $t("memory.tool_schema.permissions_label_operator"),
  );

  const emptyMessage = $derived(
    isBuilder
      ? $t("memory.tool_schema.empty_builder")
      : $t("memory.tool_schema.empty_operator"),
  );

  const KIND_STYLES: Record<string, { bg: string; text: string; label: string }> = {
    native: { bg: "bg-primary", text: "text-primary-foreground", label: "Native" },
    mcp: { bg: "bg-secondary", text: "text-secondary-foreground", label: "MCP" },
    python: { bg: "bg-warning", text: "text-warning-foreground", label: "Python" },
  };

  function kindStyle(kind: string): { bg: string; text: string; label: string } {
    return KIND_STYLES[kind] ?? { bg: "bg-muted", text: "text-foreground", label: kind };
  }

  $effect(() => {
    if (!open || !toolName) return;
    loading = true;
    loadError = false;
    descriptor = null;

    invoke<ToolDescriptorView | null>("describe_tool", { name: toolName })
      .then((result) => {
        descriptor = result;
        loading = false;
      })
      .catch(() => {
        loadError = true;
        loading = false;
      });
  });
</script>

<Sheet {open} onclose={onclose}>
  <div
    class="flex h-full flex-col gap-5 overflow-y-auto p-6 pt-10"
    data-testid="tool-schema-panel"
  >
    {#if loading}
      <div class="flex flex-1 items-center justify-center" data-testid="tool-schema-loading">
        <LoadingSpinner size={24} tone="muted" />
      </div>
    {:else if !descriptor || loadError}
      <EmptyState
        icon={FileQuestion}
        title={emptyMessage}
        page="tool-schema"
      />
    {:else}
      {@const style = kindStyle(descriptor.kind)}
      <!-- Panel title (bimodal) -->
      <p class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/50">{panelTitle}</p>

      <!-- Header: name + kind badge -->
      <div class="flex items-center gap-3">
        <h3 class="text-lg font-semibold tracking-tight">{descriptor.name}</h3>
        <div
          class="inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium {style.bg} {style.text}"
          data-testid="tool-kind-badge"
        >
          {style.label}
        </div>
      </div>

      <!-- Version -->
      <p class="text-xs text-muted-foreground">v{descriptor.version}</p>

      <!-- Description -->
      {#if descriptor.description}
        <p class="text-sm leading-relaxed line-clamp-4">{descriptor.description}</p>
      {/if}

      <!-- Input Schema -->
      <div class="glass-card glass-border rounded-xl p-4">
        <p class="mb-2 text-xs font-medium uppercase tracking-wider text-muted-foreground/60">{inputLabel}</p>
        {#if descriptor.input_schema}
          <div data-testid="tool-schema-json">
            <JsonViewer json={JSON.stringify(descriptor.input_schema)} />
          </div>
        {:else}
          <p class="text-xs text-muted-foreground/40 italic">{$t("memory.tool_schema.no_schema")}</p>
        {/if}
      </div>

      <!-- Output Schema -->
      <div class="glass-card glass-border rounded-xl p-4">
        <p class="mb-2 text-xs font-medium uppercase tracking-wider text-muted-foreground/60">{outputLabel}</p>
        {#if descriptor.output_schema}
          <div data-testid="tool-schema-json">
            <JsonViewer json={JSON.stringify(descriptor.output_schema)} />
          </div>
        {:else}
          <p class="text-xs text-muted-foreground/40 italic">{$t("memory.tool_schema.no_schema")}</p>
        {/if}
      </div>

      <!-- Permissions -->
      <div class="glass-card glass-border rounded-xl p-4">
        <p class="mb-2 text-xs font-medium uppercase tracking-wider text-muted-foreground/60">{permissionsLabel}</p>
        {#if descriptor.permissions.length > 0}
          <div class="flex flex-wrap gap-1.5">
            {#each descriptor.permissions as perm}
              <Badge variant="outline">{perm}</Badge>
            {/each}
          </div>
        {:else}
          <p class="text-xs text-muted-foreground/40 italic">{$t("memory.tool_schema.no_permissions")}</p>
        {/if}
      </div>
    {/if}
  </div>
</Sheet>
