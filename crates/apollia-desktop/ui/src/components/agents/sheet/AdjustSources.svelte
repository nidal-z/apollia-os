<script lang="ts">
  /**
   * AdjustSources - the sources the agent consults, each checked or not: the
   * user profile and the agent's memory always offered, any other source the
   * agent holds listed as written. The profile is read only when checked.
   */
  import { t } from "svelte-i18n";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import SheetSection from "./SheetSection.svelte";
  import { sourceChoices, toggleSource, type AdjustDraft } from "./agentAdjust";

  interface Props {
    draft: AdjustDraft;
    /** The sources in force when the session began. */
    inForce: string[];
    modified: boolean;
  }

  let { draft, inForce, modified }: Props = $props();

  const choices = $derived(sourceChoices(inForce, draft.sources));

  function label(id: string): string {
    if (id === "profile") return $t("agent_sheet.source_profile");
    if (id === "memory") return $t("agent_sheet.source_memory");
    return id;
  }
</script>

<SheetSection
  title={$t("agent_sheet.sources_title")}
  legend={$t("agent_adjust.sources_legend")}
  testid="agent-sheet-sources"
  {modified}
>
  <ul class="m-0 list-none space-y-2 p-0">
    {#each choices as choice (choice.id)}
      <li class="flex items-center gap-2" data-testid="agent-adjust-source" data-source={choice.id}>
        <Checkbox
          id={`agent-adjust-source-${choice.id}`}
          checked={choice.checked}
          onchange={(on) => (draft.sources = toggleSource(draft.sources, choice.id, on))}
          data-testid="agent-adjust-source-check"
        />
        <label for={`agent-adjust-source-${choice.id}`} class="truncate text-body-xs text-foreground/85">
          {label(choice.id)}
        </label>
      </li>
    {/each}
  </ul>
</SheetSection>
