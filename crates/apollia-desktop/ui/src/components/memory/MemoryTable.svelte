<script lang="ts">
  import { t } from "svelte-i18n";
  import type { MemoryEntry } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { GlossaryTerm } from "$lib/components/ui/tooltip";
  import { Trash2 } from "lucide-svelte";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { DataTable, type Column } from "$lib/components/ui/data-table";
  import { parseTimestampMs } from "$lib/utils";

  interface Props {
    entries: MemoryEntry[];
    searching: boolean;
    ondelete: (entryId: string) => void | Promise<void>;
  }

  let { entries, searching, ondelete }: Props = $props();

  let deleteEntryId = $state<string | null>(null);
  let deleting = $state(false);

  const TYPE_VARIANT: Record<string, "default" | "secondary" | "destructive" | "outline"> = {
    episodic: "default",
    semantic: "secondary",
    procedural: "outline",
  };

  function truncate(text: string, max: number): string {
    if (text.length <= max) return text;
    return text.slice(0, max) + "…";
  }

  function relativeTime(iso: string): string {
    const now = Date.now();
    const then = parseTimestampMs(iso);
    const diffMs = now - then;

    if (diffMs < 0) return $t('memory.in_the_future');

    const seconds = Math.floor(diffMs / 1000);
    if (seconds < 60) return `${seconds}s ago`;

    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m ago`;

    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h ago`;

    const days = Math.floor(hours / 24);
    return `${days}d ago`;
  }

  function ttlDisplay(expiresAt: string | null): string {
    if (expiresAt === null) return "∞";
    const now = Date.now();
    const exp = parseTimestampMs(expiresAt);
    const diffMs = exp - now;

    if (diffMs <= 0) return $t('memory.expired');

    const hours = Math.floor(diffMs / (1000 * 60 * 60));
    if (hours < 24) return `${hours}h`;

    const days = Math.floor(hours / 24);
    return `${days}d`;
  }

  function requestDelete(entryId: string) {
    deleteEntryId = entryId;
  }

  async function handleConfirmDelete() {
    if (deleteEntryId === null) return;
    deleting = true;
    try {
      await ondelete(deleteEntryId);
    } finally {
      deleting = false;
      deleteEntryId = null;
    }
  }

  function handleCancelDelete() {
    deleteEntryId = null;
  }

  const columns = $derived<Column<MemoryEntry>[]>([
    {
      key: "entry_type",
      header: $t('memory.table.type'),
      cell: typeCell,
    },
    {
      key: "key",
      header: $t('memory.table.key'),
      cell: keyCell,
    },
    {
      key: "value",
      header: $t('memory.table.value'),
      cell: valueCell,
    },
    ...(searching
      ? [{ key: "score" as const, header: $t('memory.table.score'), cell: scoreCell } satisfies Column<MemoryEntry>]
      : []),
    {
      key: "created_at",
      header: $t('memory.table.date'),
      cell: dateCell,
    },
    {
      key: "expires_at",
      header: $t('memory.table.ttl'),
      cell: ttlCell,
    },
    {
      key: "id",
      header: $t('memory.table.actions'),
      align: "right",
      cell: actionsCell,
    },
  ]);
</script>

{#snippet typeCell(entry: MemoryEntry)}
  <Badge variant={TYPE_VARIANT[entry.entry_type] ?? "secondary"}>
    <GlossaryTerm term={entry.entry_type} label={$t(`memory.type.${entry.entry_type}`, { default: entry.entry_type })} />
  </Badge>
{/snippet}

{#snippet keyCell(entry: MemoryEntry)}
  <span class="block max-w-[160px] truncate font-mono text-xs" title={entry.key}>
    {entry.key}
  </span>
{/snippet}

{#snippet valueCell(entry: MemoryEntry)}
  <span class="block max-w-[300px]" title={entry.value}>
    {truncate(entry.value, 100)}
  </span>
{/snippet}

{#snippet scoreCell(entry: MemoryEntry)}
  <span class="font-mono text-xs">{entry.score !== null && entry.score !== undefined ? entry.score.toFixed(2) : "-"}</span>
{/snippet}

{#snippet dateCell(entry: MemoryEntry)}
  <span class="whitespace-nowrap text-xs text-muted-foreground">{relativeTime(entry.created_at)}</span>
{/snippet}

{#snippet ttlCell(entry: MemoryEntry)}
  <span class="text-xs text-muted-foreground">{ttlDisplay(entry.expires_at)}</span>
{/snippet}

{#snippet actionsCell(entry: MemoryEntry)}
  <Button
    size="sm"
    variant="ghost"
    onclick={() => requestDelete(entry.id)}
    aria-label={$t('memory.delete_label')}
    data-testid="memory-delete-btn-{entry.id}"
  >
    <Trash2 class="h-4 w-4" />
  </Button>
{/snippet}

{#if entries.length === 0}
  <div
    class="flex flex-col items-center justify-center gap-2 rounded-xl glass-surface glass-border border-dashed py-12"
  >
    <p class="text-muted-foreground">
      {searching
        ? $t('memory.no_search_results')
        : $t('memory.empty_entries')}
    </p>
  </div>
{:else}
  <DataTable
    data={entries}
    {columns}
    rowKey={(e) => e.id}
    class="min-w-[640px]"
    emptyLabel={$t('memory.empty_entries')}
  />
{/if}

<!-- Delete confirmation dialog -->
<ConfirmDialog
  open={deleteEntryId !== null}
  onclose={handleCancelDelete}
  onconfirm={handleConfirmDelete}
  title={$t('memory.delete_confirm_title')}
  message={$t('memory.delete_confirm_message')}
  confirmLabel={$t('memory.delete_confirm_yes')}
  cancelLabel={$t('common.cancel')}
  loading={deleting}
  data-testid="memory-delete-confirm"
/>
