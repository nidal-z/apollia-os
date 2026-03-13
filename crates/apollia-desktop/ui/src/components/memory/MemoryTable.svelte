<script lang="ts">
  import type { MemoryEntry } from "$lib/types";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    entries: MemoryEntry[];
    searching: boolean;
    ondelete: (entryId: string) => void;
  }

  let { entries, searching, ondelete }: Props = $props();

  let confirmingId = $state<string | null>(null);

  const TYPE_VARIANT: Record<string, "default" | "secondary" | "destructive" | "outline"> = {
    episodic: "default",
    semantic: "secondary",
    procedural: "outline",
  };

  const TYPE_LABEL: Record<string, string> = {
    episodic: "Episodic",
    semantic: "Semantic",
    procedural: "Procedural",
  };

  function truncate(text: string, max: number): string {
    if (text.length <= max) return text;
    return text.slice(0, max) + "\u2026";
  }

  function relativeTime(iso: string): string {
    const now = Date.now();
    const then = new Date(iso).getTime();
    const diffMs = now - then;

    if (diffMs < 0) return "in the future";

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
    if (expiresAt === null) return "\u221E";
    const now = Date.now();
    const exp = new Date(expiresAt).getTime();
    const diffMs = exp - now;

    if (diffMs <= 0) return "expired";

    const hours = Math.floor(diffMs / (1000 * 60 * 60));
    if (hours < 24) return `${hours}h`;

    const days = Math.floor(hours / 24);
    return `${days}d`;
  }

  function handleDeleteClick(entryId: string) {
    confirmingId = entryId;
  }

  function handleConfirmDelete() {
    if (confirmingId !== null) {
      ondelete(confirmingId);
      confirmingId = null;
    }
  }

  function handleCancelDelete() {
    confirmingId = null;
  }
</script>

{#if entries.length === 0}
  <div
    class="flex flex-col items-center justify-center gap-2 rounded-lg border border-dashed py-12"
  >
    <p class="text-muted-foreground">
      {searching
        ? "Aucun résultat pour cette recherche."
        : "Aucune entrée mémoire dans ce namespace."}
    </p>
  </div>
{:else}
  <div class="overflow-x-auto rounded-md border">
    <table class="w-full text-sm">
      <thead class="border-b bg-muted/50">
        <tr>
          <th class="px-4 py-2 text-left font-medium text-muted-foreground">Type</th>
          <th class="px-4 py-2 text-left font-medium text-muted-foreground">Clé</th>
          <th class="px-4 py-2 text-left font-medium text-muted-foreground">Valeur</th>
          {#if searching}
            <th class="px-4 py-2 text-left font-medium text-muted-foreground">Score</th>
          {/if}
          <th class="px-4 py-2 text-left font-medium text-muted-foreground">Date</th>
          <th class="px-4 py-2 text-left font-medium text-muted-foreground">TTL</th>
          <th class="px-4 py-2 text-right font-medium text-muted-foreground">Actions</th>
        </tr>
      </thead>
      <tbody>
        {#each entries as entry (entry.id)}
          <tr class="border-b last:border-b-0 hover:bg-muted/30">
            <td class="px-4 py-2">
              <Badge variant={TYPE_VARIANT[entry.entry_type] ?? "secondary"}>
                {TYPE_LABEL[entry.entry_type] ?? entry.entry_type}
              </Badge>
            </td>
            <td class="max-w-[160px] truncate px-4 py-2 font-mono text-xs" title={entry.key}>
              {entry.key}
            </td>
            <td class="max-w-[300px] px-4 py-2" title={entry.value}>
              {truncate(entry.value, 100)}
            </td>
            {#if searching}
              <td class="px-4 py-2 font-mono text-xs">
                {entry.score !== null ? entry.score.toFixed(2) : "-"}
              </td>
            {/if}
            <td class="whitespace-nowrap px-4 py-2 text-xs text-muted-foreground">
              {relativeTime(entry.created_at)}
            </td>
            <td class="px-4 py-2 text-xs text-muted-foreground">
              {ttlDisplay(entry.expires_at)}
            </td>
            <td class="px-4 py-2 text-right">
              {#if confirmingId === entry.id}
                <div class="inline-flex items-center gap-1">
                  <Button size="sm" variant="destructive" onclick={handleConfirmDelete}>
                    Confirmer
                  </Button>
                  <Button size="sm" variant="outline" onclick={handleCancelDelete}>
                    Annuler
                  </Button>
                </div>
              {:else}
                <Button
                  size="sm"
                  variant="ghost"
                  onclick={() => handleDeleteClick(entry.id)}
                  aria-label="Supprimer"
                >
                  &#x1F5D1;
                </Button>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}
