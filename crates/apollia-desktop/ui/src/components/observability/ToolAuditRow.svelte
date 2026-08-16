<script lang="ts">
  // Audit row for a single tool invocation, with an expandable detail panel.
  // Extracted verbatim from AuditTrailTable so the table stays focused on the
  // row dispatch. Expansion state is owned by the parent; this row reports
  // clicks through `ontoggle`.
  import { t, locale } from "svelte-i18n";
  import { ContextMenu as ContextMenuPrimitive } from "bits-ui";
  import { CheckCircle2, XCircle, ChevronDown, Copy, Braces, Search } from "lucide-svelte";
  import { cn } from "$lib/utils";
  import { addToast } from "$lib/components/ui/toast";
  import type { AuditTrailEntry } from "$lib/types";

  interface Props {
    entry: AuditTrailEntry;
    expanded: boolean;
    ontoggle: (id: string) => void;
  }

  let { entry, expanded, ontoggle }: Props = $props();

  const menuItemClass =
    "flex w-full cursor-pointer items-center gap-2 rounded-sm px-2 py-1.5 text-left text-body-xs text-foreground outline-none transition-colors data-[highlighted]:bg-muted";

  type EntryStatus = "ok" | "error" | "unknown";

  /** Status of an audit entry, derived from exit_code + stderr presence.
   *  An MCP tool with no exit_code is considered "ok" unless stderr is set. */
  const status = $derived<EntryStatus>(
    entry.exit_code !== null && entry.exit_code !== undefined
      ? entry.exit_code === 0
        ? "ok"
        : "error"
      : entry.stderr && entry.stderr.trim().length > 0
        ? "error"
        : entry.duration_ms !== null && entry.duration_ms !== undefined
          ? "ok"
          : "unknown",
  );

  function formatTimestamp(iso: string): string {
    if (!iso) return "";
    return new Date(iso).toLocaleString($locale ?? "en");
  }

  function formatDuration(ms: number | null): string {
    if (ms === null || ms === undefined) return "-";
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
  }

  async function writeClipboard(text: string, okKey: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(text);
      addToast($t(okKey), "success");
    } catch {
      addToast($t("observability.audit_copy_failed"), "error");
    }
  }

  /** Copies the row as a tab-separated line (timestamp, tool, agent, duration, status). */
  function copyRow(): void {
    const tsv = [
      formatTimestamp(entry.timestamp),
      entry.tool_name,
      entry.agent_name,
      formatDuration(entry.duration_ms),
      status,
    ].join("\t");
    void writeClipboard(tsv, "observability.audit_row_copied");
  }

  /** Copies the full raw entry as pretty-printed JSON. */
  function copyJson(): void {
    void writeClipboard(JSON.stringify(entry, null, 2), "observability.audit_json_copied");
  }

  function onRowKeydown(event: KeyboardEvent): void {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      ontoggle(entry.id);
    } else if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "c") {
      event.preventDefault();
      copyRow();
    }
  }
</script>

<ContextMenuPrimitive.Root>
  <ContextMenuPrimitive.Trigger>
    {#snippet child({ props })}
      <tr
        {...props}
        role="button"
        tabindex={-1}
        data-audit-row=""
        class="cursor-pointer border-b border-border/30 last:border-0 transition-colors hover:bg-muted/40 outline-none focus-visible:ring-2 focus-visible:ring-primary focus-visible:ring-inset"
        class:bg-muted={expanded}
        onclick={() => ontoggle(entry.id)}
        onkeydown={onRowKeydown}
        data-testid="audit-row-{entry.id}"
      >
        <td class="px-5 py-2.5 text-caption text-muted-foreground tabular-nums whitespace-nowrap">
          {formatTimestamp(entry.timestamp)}
        </td>
        <td class="px-3 py-2.5">
          <code class="font-mono text-body-xs text-foreground">{entry.tool_name}</code>
        </td>
        <td class="px-3 py-2.5 text-muted-foreground">{entry.agent_name}</td>
        <td class="px-3 py-2.5 text-right tabular-nums">{formatDuration(entry.duration_ms)}</td>
        <td class="px-3 py-2.5">
          {#if status === "ok"}
            <span class="inline-flex items-center gap-1.5 text-success">
              <CheckCircle2 class="h-3.5 w-3.5" />
              <span class="text-caption font-medium">{$t('observability.audit_status_ok')}</span>
            </span>
          {:else if status === "error"}
            <span class="inline-flex items-center gap-1.5 text-destructive">
              <XCircle class="h-3.5 w-3.5" />
              <span class="text-caption font-medium">{$t('observability.audit_status_error')}</span>
            </span>
          {:else}
            <span class="text-caption text-muted-foreground/60">
              {$t('observability.audit_status_unknown')}
            </span>
          {/if}
        </td>
        <td class="px-5 py-2.5 text-right">
          <ChevronDown
            class="h-3.5 w-3.5 text-muted-foreground/60 transition-transform inline-block"
            style={expanded ? "transform: rotate(180deg);" : ""}
          />
        </td>
      </tr>
    {/snippet}
  </ContextMenuPrimitive.Trigger>
  <ContextMenuPrimitive.Portal>
    <ContextMenuPrimitive.Content
      class="glass-card glass-border rounded-lg shadow-lg p-1 min-w-[11rem]"
      style="z-index: var(--z-overlay);"
      data-testid="audit-row-menu"
    >
      <ContextMenuPrimitive.Item class={cn(menuItemClass)} onSelect={copyRow}>
        <Copy size={14} aria-hidden="true" />
        <span class="flex-1">{$t('observability.audit_menu_copy_row')}</span>
      </ContextMenuPrimitive.Item>
      <ContextMenuPrimitive.Item class={cn(menuItemClass)} onSelect={copyJson}>
        <Braces size={14} aria-hidden="true" />
        <span class="flex-1">{$t('observability.audit_menu_copy_json')}</span>
      </ContextMenuPrimitive.Item>
      <ContextMenuPrimitive.Item class={cn(menuItemClass)} onSelect={() => ontoggle(entry.id)}>
        <Search size={14} aria-hidden="true" />
        <span class="flex-1">{$t('observability.audit_menu_inspect')}</span>
      </ContextMenuPrimitive.Item>
    </ContextMenuPrimitive.Content>
  </ContextMenuPrimitive.Portal>
</ContextMenuPrimitive.Root>

{#if expanded}
  <tr class="bg-muted/20">
    <td colspan="6" class="px-5 pb-4 pt-1">
      <div class="space-y-3 rounded-lg glass-inset border border-border/30 p-4">
        {#if entry.args_json}
          <div>
            <span class="section-meta">
              {$t('observability.table.arguments')}
            </span>
            <pre
              class="mt-1.5 overflow-x-auto rounded glass-surface p-3 text-code-sm font-mono leading-relaxed"
            >{entry.args_json}</pre>
          </div>
        {/if}
        {#if entry.stdout}
          <div>
            <span class="section-meta">
              stdout
            </span>
            <pre
              class="mt-1.5 overflow-x-auto rounded glass-surface p-3 text-code-sm font-mono leading-relaxed"
            >{entry.stdout}</pre>
          </div>
        {/if}
        {#if entry.stderr}
          <div>
            <span class="section-meta text-destructive">
              stderr
            </span>
            <pre
              class="mt-1.5 overflow-x-auto rounded glass-surface p-3 text-code-sm font-mono leading-relaxed text-destructive"
            >{entry.stderr}</pre>
          </div>
        {/if}
        {#if !entry.args_json && !entry.stdout && !entry.stderr}
          <p class="text-body-xs text-muted-foreground italic">
            {$t('observability.table.no_details')}
          </p>
        {/if}
      </div>
    </td>
  </tr>
{/if}
