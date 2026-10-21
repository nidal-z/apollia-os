<script lang="ts">
  /**
   * JournalConnectionFilter - the connection the journal was opened on, said
   * by its name, with the gesture that removes it. The bar has no menu for a
   * connection: it is chosen from the connection itself, and only shown and
   * removed here.
   */
  import { t } from "svelte-i18n";
  import { X } from "lucide-svelte";
  import type { JournalConnectionFocus } from "$lib/stores/journalFocus";

  interface Props {
    connection: JournalConnectionFocus;
    onRemove: () => void;
  }

  let { connection, onRemove }: Props = $props();
</script>

<div
  class="inline-flex max-w-full items-center gap-2 rounded-full border border-border bg-muted/40 py-1 pl-3 pr-1 text-caption text-foreground"
  data-testid="journal-filter-connection"
  data-prefix={connection.prefix}
>
  <span class="truncate">
    {$t("journal.filter.connection", { values: { name: connection.label } })}
  </span>
  <button
    type="button"
    class="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    aria-label={$t("journal.filter.connection_remove", { values: { name: connection.label } })}
    onclick={onRemove}
    data-testid="journal-filter-connection-remove"
  >
    <X size={12} strokeWidth={2} />
  </button>
</div>
