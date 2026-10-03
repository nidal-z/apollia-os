<script lang="ts">
  /**
   * SheetSources - what the agent consults: the user profile, its memory,
   * and any other source checked for it. The profile is read only when it is
   * checked here.
   */
  import { t } from "svelte-i18n";
  import { BookUser, Brain, FolderOpen } from "lucide-svelte";
  import SheetSection from "./SheetSection.svelte";
  import type { SourceView } from "./agentSheet";

  interface Props {
    sources: SourceView[];
    /** Whether the value shown is the user's rather than the package's. */
    modified?: boolean;
  }

  let { sources, modified = false }: Props = $props();
</script>

<SheetSection
  title={$t("agent_sheet.sources_title")}
  legend={$t("agent_sheet.sources_legend")}
  testid="agent-sheet-sources"
  {modified}
>
  {#if sources.length === 0}
    <p class="m-0 text-body-xs italic text-muted-foreground" data-testid="agent-sheet-sources-empty">
      {$t("agent_sheet.sources_empty")}
    </p>
  {:else}
    <ul class="m-0 list-none space-y-1.5 p-0">
      {#each sources as source (source.id)}
        <li class="flex items-center gap-2" data-testid="agent-sheet-source" data-source={source.kind}>
          {#if source.kind === "profile"}
            <BookUser size={14} class="shrink-0 text-muted-foreground" />
            <span class="text-body-xs text-foreground/85">{$t("agent_sheet.source_profile")}</span>
          {:else if source.kind === "memory"}
            <Brain size={14} class="shrink-0 text-muted-foreground" />
            <span class="text-body-xs text-foreground/85">{$t("agent_sheet.source_memory")}</span>
          {:else}
            <FolderOpen size={14} class="shrink-0 text-muted-foreground" />
            <span class="truncate text-body-xs text-foreground/85" title={source.id}>{source.id}</span>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</SheetSection>
