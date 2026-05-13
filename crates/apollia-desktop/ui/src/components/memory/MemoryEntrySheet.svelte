<script lang="ts">
  import { Clock, Database, Cog, Copy, Trash2, X, Check } from "lucide-svelte";
  import { Sheet, SheetHeader, SheetContent, SheetFooter } from "$lib/components/ui/sheet";
  import type { MemoryEntry } from "$lib/types";
  import { addToast } from "$lib/components/ui/toast/store";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    entry: MemoryEntry | null;
    namespace: string;
    open: boolean;
    onclose: () => void;
    ondelete?: (entryId: string) => Promise<void> | void;
  }

  let { entry, namespace, open, onclose, ondelete }: Props = $props();

  // ── Pretty-print JSON si la valeur est du JSON valide ───────────────────────
  const prettyValue = $derived.by(() => {
    if (!entry?.value) return { isJson: false, text: "" };
    const raw = entry.value;
    try {
      // Tenter parse — gère "{...}", "[...]", strings JSON-encodées, nombres.
      const parsed = JSON.parse(raw);
      if (typeof parsed === "object" && parsed !== null) {
        return { isJson: true, text: JSON.stringify(parsed, null, 2) };
      }
      // String, nombre, bool — rendre en tant que tel sans forcer pretty
      return { isJson: false, text: raw };
    } catch {
      return { isJson: false, text: raw };
    }
  });

  // Icone type — rendue explicitement via if/else dans le markup (cohérence
  // avec le reste du codebase qui n'utilise pas de pattern dynamic via $derived).

  const typeLabel = $derived(
    !entry ? "" :
    entry.entry_type === "episodic" ? "Épisodique" :
    entry.entry_type === "semantic" ? "Sémantique" :
    entry.entry_type === "procedural" ? "Procédurale" : entry.entry_type
  );

  function formatDate(iso: string): string {
    const d = new Date(iso);
    return d.toLocaleString(undefined, {
      year: "numeric",
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  function ttlDescription(expiresAt: string | null): string {
    if (expiresAt === null) return "Pas d'expiration (TTL ∞)";
    const diffMs = new Date(expiresAt).getTime() - Date.now();
    if (diffMs <= 0) return `Expiré le ${formatDate(expiresAt)}`;
    const hours = Math.floor(diffMs / (1000 * 60 * 60));
    if (hours < 24) return `Expire dans ${hours}h (le ${formatDate(expiresAt)})`;
    const days = Math.floor(hours / 24);
    return `Expire dans ${days}j (le ${formatDate(expiresAt)})`;
  }

  // ── Actions ─────────────────────────────────────────────────────────────────
  async function handleCopy(): Promise<void> {
    if (!entry) return;
    try {
      await navigator.clipboard.writeText(prettyValue.text);
      addToast("Valeur copiée", "success");
    } catch {
      addToast("Impossible de copier", "error");
    }
  }

  let confirmingDelete = $state(false);
  let deleting = $state(false);

  async function handleDeleteConfirm(): Promise<void> {
    if (!entry || !ondelete) return;
    deleting = true;
    try {
      await ondelete(entry.id);
      confirmingDelete = false;
      onclose();
    } finally {
      deleting = false;
    }
  }

  $effect(() => {
    if (!open) confirmingDelete = false;
  });
</script>

<Sheet {open} {onclose} width="lg">
  {#if entry}
    <SheetHeader title="" {onclose} closeLabel="Fermer" class="items-center px-6 py-4">
      {#snippet leading()}
        <div
          class="w-[32px] h-[32px] rounded-md inline-flex items-center justify-center bg-muted text-muted-foreground"
          title={typeLabel}
        >
          {#if entry.entry_type === "episodic"}
            <Clock size={16} />
          {:else if entry.entry_type === "procedural"}
            <Cog size={16} />
          {:else}
            <Database size={16} />
          {/if}
        </div>
      {/snippet}
      {#snippet titleSlot()}
        <div class="flex items-center gap-2">
          <span
            class="inline-flex shrink-0 items-center px-1.5 py-px rounded text-[10px] font-medium tracking-tight border border-border/60 bg-background text-muted-foreground"
          >
            {typeLabel.toLowerCase()}
          </span>
          <span class="text-[10.5px] text-muted-foreground/70 font-mono truncate">{namespace}</span>
        </div>
        <code class="block mt-1 text-[13px] font-mono font-medium text-foreground truncate" title={entry.key}>
          {entry.key}
        </code>
      {/snippet}
    </SheetHeader>

    <SheetContent padding="flush" class="px-6 py-4 space-y-5">
      <!-- Value -->
      <section>
        <div class="flex items-center justify-between mb-2">
          <h3 class="text-[10.5px] uppercase tracking-[1px] font-semibold text-muted-foreground/70">
            Valeur
            {#if prettyValue.isJson}
              <span
                class="ml-2 inline-flex items-center px-1.5 py-px rounded text-[9.5px] font-medium tracking-tight border border-border/60 bg-background text-muted-foreground normal-case tracking-normal"
              >
                JSON
              </span>
            {/if}
          </h3>
          <button
            type="button"
            onclick={handleCopy}
            class="inline-flex items-center gap-1 px-2 py-1 rounded-md text-[11px] text-muted-foreground hover:bg-muted/60 hover:text-foreground transition-colors"
            data-testid="memory-entry-sheet-copy"
          >
            <Copy size={11} />
            Copier
          </button>
        </div>
        <pre
          class="text-[11.5px] font-mono leading-relaxed p-3 rounded-md border border-border/50 bg-surface-1 text-foreground overflow-x-auto whitespace-pre-wrap break-words max-h-[400px] overflow-y-auto"
        >{prettyValue.text}</pre>
      </section>

      <!-- Metadata -->
      <section>
        <h3 class="text-[10.5px] uppercase tracking-[1px] font-semibold text-muted-foreground/70 mb-2">
          Métadonnées
        </h3>
        <dl class="space-y-2 text-[12px]">
          <div class="flex items-baseline gap-3">
            <dt class="w-[100px] shrink-0 text-muted-foreground">Type</dt>
            <dd class="text-foreground">{typeLabel}</dd>
          </div>
          <div class="flex items-baseline gap-3">
            <dt class="w-[100px] shrink-0 text-muted-foreground">Namespace</dt>
            <dd class="text-foreground font-mono text-[11.5px]">{namespace}</dd>
          </div>
          <div class="flex items-baseline gap-3">
            <dt class="w-[100px] shrink-0 text-muted-foreground">ID</dt>
            <dd class="text-foreground font-mono text-[10.5px] truncate" title={entry.id}>{entry.id}</dd>
          </div>
          <div class="flex items-baseline gap-3">
            <dt class="w-[100px] shrink-0 text-muted-foreground">Créée</dt>
            <dd class="text-foreground">{formatDate(entry.created_at)}</dd>
          </div>
          <div class="flex items-baseline gap-3">
            <dt class="w-[100px] shrink-0 text-muted-foreground">Expiration</dt>
            <dd class="text-foreground">{ttlDescription(entry.expires_at)}</dd>
          </div>
          {#if entry.score !== null && entry.score !== undefined}
            <div class="flex items-baseline gap-3">
              <dt class="w-[100px] shrink-0 text-muted-foreground">Score BM25</dt>
              <dd class="text-foreground font-mono">{entry.score.toFixed(3)}</dd>
            </div>
          {/if}
        </dl>
      </section>
    </SheetContent>

    <!-- Footer actions -->
    {#if ondelete}
      <SheetFooter class="bg-surface-1/60">
        {#if !confirmingDelete}
          <Button variant="ghost" size="sm"
            type="button"
            onclick={() => (confirmingDelete = true)}
            class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[12px] font-medium text-destructive hover:bg-destructive/10 transition-colors"
            data-testid="memory-entry-sheet-delete"
          >
            <Trash2 size={12} />
            Supprimer
          </Button>
        {:else}
          <span class="text-[11.5px] text-muted-foreground mr-2">
            Confirmer la suppression ?
          </span>
          <Button variant="ghost" size="sm"
            type="button"
            onclick={() => (confirmingDelete = false)}
            disabled={deleting}
            class="inline-flex items-center gap-1 px-2.5 py-1.5 rounded-md text-[11.5px] font-medium text-muted-foreground hover:bg-muted/60 hover:text-foreground"
          >
            <X size={11} /> Annuler
          </Button>
          <Button variant="ghost" size="sm"
            type="button"
            onclick={handleDeleteConfirm}
            disabled={deleting}
            class="inline-flex items-center gap-1 px-2.5 py-1.5 rounded-md bg-destructive text-white text-[11.5px] font-semibold hover:bg-destructive/90 disabled:opacity-50"
            data-testid="memory-entry-sheet-delete-confirm"
          >
            <Check size={11} /> {deleting ? "…" : "Confirmer"}
          </Button>
        {/if}
      </SheetFooter>
    {/if}
  {/if}
</Sheet>
