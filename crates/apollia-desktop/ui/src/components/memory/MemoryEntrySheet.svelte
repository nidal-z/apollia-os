<script lang="ts">
  import { Clock, Database, Cog, Copy, Trash2, X, Check, Sparkles } from "lucide-svelte";
  import { t } from "svelte-i18n";
  import { Sheet, SheetHeader, SheetContent, SheetFooter } from "$lib/components/ui/sheet";
  import type { MemoryEntry } from "$lib/types";
  import { uiMode } from "$lib/stores/mode";
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
      const parsed = JSON.parse(raw);
      if (typeof parsed === "object" && parsed !== null) {
        return { isJson: true, text: JSON.stringify(parsed, null, 2) };
      }
      return { isJson: false, text: raw };
    } catch {
      return { isJson: false, text: raw };
    }
  });

  const typeLabel = $derived(
    !entry
      ? ""
      : entry.entry_type === "episodic"
        ? $t("memory.type.episodic")
        : entry.entry_type === "semantic"
          ? $t("memory.type.semantic")
          : entry.entry_type === "procedural"
            ? $t("memory.type.procedural")
            : entry.entry_type,
  );

  function formatDate(iso: string): string {
    return new Date(iso).toLocaleString(undefined, {
      year: "numeric",
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  function ttlDescription(expiresAt: string | null): string {
    if (expiresAt === null) return $t("memory.ttl_none");
    const diffMs = new Date(expiresAt).getTime() - Date.now();
    if (diffMs <= 0) return $t("memory.ttl_expired", { values: { date: formatDate(expiresAt) } });
    const hours = Math.floor(diffMs / (1000 * 60 * 60));
    if (hours < 24) return $t("memory.ttl_in_hours", { values: { hours, date: formatDate(expiresAt) } });
    const days = Math.floor(hours / 24);
    return $t("memory.ttl_in_days", { values: { days, date: formatDate(expiresAt) } });
  }

  async function handleCopy(): Promise<void> {
    if (!entry) return;
    try {
      await navigator.clipboard.writeText(prettyValue.text);
      addToast($t("memory.value_copied"), "success");
    } catch {
      addToast($t("memory.copy_failed"), "error");
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
    <!-- Expressive depth : liseret de marque en tête du Sheet (point focal). -->
    <div class="h-0.5 w-full bg-gradient-primary" aria-hidden="true"></div>
    <SheetHeader title="" {onclose} closeLabel={$t("memory.sheet_close")} class="items-center px-6 py-4">
      {#snippet leading()}
        <div class="inline-flex h-8 w-8 items-center justify-center rounded-md bg-muted text-muted-foreground" title={typeLabel}>
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
          <span class="inline-flex shrink-0 items-center rounded border border-border/60 bg-background px-1.5 py-px text-overline font-medium tracking-tight text-muted-foreground">
            {typeLabel.toLowerCase()}
          </span>
          <span class="truncate font-mono text-overline font-normal tracking-normal text-muted-foreground/70">{namespace}</span>
        </div>
        <code class="mt-1 block truncate font-mono text-body-sm font-medium text-foreground" title={entry.key}>
          {entry.key}
        </code>
      {/snippet}
    </SheetHeader>

    <SheetContent padding="flush" class="space-y-5 px-6 py-4">
      <!-- Value -->
      <section>
        <div class="mb-2 flex items-center justify-between">
          <h3 class="inline-flex items-center gap-2 text-overline uppercase text-muted-foreground/70">
            {$t("memory.value_label")}
            {#if prettyValue.isJson}
              <span class="inline-flex items-center rounded border border-border/60 bg-background px-1.5 py-px text-overline font-medium normal-case tracking-normal text-muted-foreground">
                JSON
              </span>
            {/if}
          </h3>
          <button
            type="button"
            onclick={handleCopy}
            class="inline-flex items-center gap-1 rounded-md px-2 py-1 text-caption text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground"
            data-testid="memory-entry-sheet-copy"
          >
            <Copy size={11} />
            {$t("memory.copy_label")}
          </button>
        </div>
        <pre
          class="max-h-96 overflow-x-auto overflow-y-auto whitespace-pre-wrap break-words rounded-md border border-border/50 bg-surface-1 p-3 font-mono text-caption leading-relaxed text-foreground"
        >{prettyValue.text}</pre>
      </section>

      <!-- Metadata -->
      <section>
        <h3 class="mb-2 text-overline uppercase text-muted-foreground/70">
          {$t("memory.metadata_label")}
        </h3>
        <dl class="space-y-2 text-body-xs">
          <div class="flex items-baseline gap-3">
            <dt class="w-24 shrink-0 text-muted-foreground">{$t("memory.meta_type")}</dt>
            <dd class="text-foreground">{typeLabel}</dd>
          </div>
          <div class="flex items-baseline gap-3">
            <dt class="w-24 shrink-0 text-muted-foreground">{$t("memory.meta_namespace")}</dt>
            <dd class="font-mono text-caption text-foreground">{namespace}</dd>
          </div>
          <div class="flex items-baseline gap-3">
            <dt class="w-24 shrink-0 text-muted-foreground">{$t("memory.meta_id")}</dt>
            <dd class="truncate font-mono text-overline font-normal tracking-normal text-foreground" title={entry.id}>{entry.id}</dd>
          </div>
          <div class="flex items-baseline gap-3">
            <dt class="w-24 shrink-0 text-muted-foreground">{$t("memory.meta_created")}</dt>
            <dd class="text-foreground">{formatDate(entry.created_at)}</dd>
          </div>
          <div class="flex items-baseline gap-3">
            <dt class="w-24 shrink-0 text-muted-foreground">{$t("memory.meta_expiration")}</dt>
            <dd class="text-foreground">{ttlDescription(entry.expires_at)}</dd>
          </div>
          {#if $uiMode === "builder" && entry.score !== null && entry.score !== undefined}
            <div class="flex items-baseline gap-3">
              <dt class="w-24 shrink-0 text-muted-foreground">{$t("memory.meta_score")}</dt>
              <dd class="font-mono text-foreground">{entry.score.toFixed(3)}</dd>
            </div>
          {/if}
        </dl>
        {#if $uiMode === "builder"}
          <div class="mt-3 inline-flex items-center gap-1.5 text-overline font-normal italic tracking-normal text-muted-foreground/80">
            <Sparkles size={11} class="text-primary" />
            {entry.entry_type === "episodic" ? $t("memory.embeddings.none") : $t("memory.embeddings.vector")}
          </div>
        {/if}
      </section>
    </SheetContent>

    <!-- Footer actions -->
    {#if ondelete}
      <SheetFooter class="bg-surface-1/60">
        {#if !confirmingDelete}
          <Button
            variant="ghost"
            size="sm"
            type="button"
            onclick={() => (confirmingDelete = true)}
            class="inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 text-body-xs font-medium text-destructive transition-colors hover:bg-destructive/10"
            data-testid="memory-entry-sheet-delete"
          >
            <Trash2 size={12} />
            {$t("memory.delete_label")}
          </Button>
        {:else}
          <span class="mr-2 text-caption text-muted-foreground">
            {$t("memory.delete_confirm_question")}
          </span>
          <Button
            variant="ghost"
            size="sm"
            type="button"
            onclick={() => (confirmingDelete = false)}
            disabled={deleting}
            class="inline-flex items-center gap-1 rounded-md px-2.5 py-1.5 text-caption font-medium text-muted-foreground hover:bg-muted/60 hover:text-foreground"
          >
            <X size={11} /> {$t("memory.cancel")}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            type="button"
            onclick={handleDeleteConfirm}
            disabled={deleting}
            class="inline-flex items-center gap-1 rounded-md bg-destructive px-2.5 py-1.5 text-caption font-semibold text-primary-foreground hover:bg-destructive/90 disabled:opacity-50"
            data-testid="memory-entry-sheet-delete-confirm"
          >
            <Check size={11} /> {deleting ? "…" : $t("memory.confirm")}
          </Button>
        {/if}
      </SheetFooter>
    {/if}
  {/if}
</Sheet>
