<script lang="ts">
  /**
   * One line of the Library list: a package with its publisher, its version,
   * what it holds and the state of its signature. The whole line opens it.
   */
  import { t } from "svelte-i18n";
  import { ChevronRight, Package, ShieldAlert, ShieldCheck } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import type { LibraryEntry } from "$lib/library/libraryState";

  interface Props {
    entry: LibraryEntry;
    onopen: (key: string) => void;
  }

  let { entry, onopen }: Props = $props();

  const name = $derived(entry.provided ? $t("library.provided.name") : entry.key);
  const origin = $derived(
    entry.provided
      ? $t("library.provided.origin")
      : entry.publisher || $t("library.publisher_unknown"),
  );
</script>

<li>
  <button
    type="button"
    class="flex w-full items-center gap-3 rounded-lg border border-border bg-card px-4 py-3 text-left transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    onclick={() => onopen(entry.key)}
    data-testid="library-package-row"
    data-package={entry.key}
    data-signed={entry.signed}
  >
    <span class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
      <Package size={18} />
    </span>
    <span class="min-w-0 flex-1">
      <span class="block truncate text-body-sm font-medium text-foreground" data-testid="library-package-name">
        {name}
      </span>
      <span class="block truncate text-caption text-muted-foreground" data-testid="library-package-origin">
        {origin} · {$t("packages.preview.version", { values: { version: entry.version } })}
      </span>
    </span>
    <span class="hidden min-w-0 shrink text-caption text-muted-foreground md:block" data-testid="library-package-content">
      {$t("library.content_summary", { values: { ...entry.counts } })}
    </span>
    {#if entry.signed}
      <Badge variant="success" outline data-testid="library-package-signature">
        {#snippet icon()}<ShieldCheck size={11} />{/snippet}
        {$t("library.signed")}
      </Badge>
    {:else}
      <Badge variant="warning" outline data-testid="library-package-signature">
        {#snippet icon()}<ShieldAlert size={11} />{/snippet}
        {$t("library.unsigned")}
      </Badge>
    {/if}
    <ChevronRight size={16} class="shrink-0 text-muted-foreground" />
  </button>
</li>
