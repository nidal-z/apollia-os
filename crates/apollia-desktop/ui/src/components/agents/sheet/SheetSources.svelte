<script lang="ts">
  /**
   * SheetSources - what the agent consults: the sources checked for it, kind
   * by kind, each with where its index stands and the files it left out. The
   * profile is read only when it is checked here.
   */
  import { t } from "svelte-i18n";
  import SheetSection from "./SheetSection.svelte";
  import SourceRow from "./SourceRow.svelte";
  import type { SourceRow as Row } from "./agentSources";

  interface Props {
    rows: Row[];
    /** Whether the value shown is the user's rather than the package's. */
    modified?: boolean;
  }

  let { rows, modified = false }: Props = $props();
</script>

<SheetSection
  title={$t("agent_sheet.sources_title")}
  legend={$t("agent_sheet.sources_legend")}
  testid="agent-sheet-sources"
  {modified}
>
  {#if rows.length === 0}
    <p class="m-0 text-body-xs italic text-muted-foreground" data-testid="agent-sheet-sources-empty">
      {$t("agent_sheet.sources_empty")}
    </p>
  {:else}
    <ul class="m-0 list-none divide-y divide-border/40 p-0">
      {#each rows as row (row.id)}
        <SourceRow {row} />
      {/each}
    </ul>
  {/if}
</SheetSection>
