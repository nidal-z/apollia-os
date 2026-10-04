<script lang="ts">
  /**
   * SheetMission - what the agent is for, and example requests it answers.
   */
  import { t } from "svelte-i18n";
  import SheetSection from "./SheetSection.svelte";
  import type { MissionView } from "./agentSheet";

  interface Props {
    view: MissionView;
    /** Whether the value shown is the user's rather than the package's. */
    modified?: boolean;
  }

  let { view, modified = false }: Props = $props();
</script>

<SheetSection
  title={$t("agent_sheet.mission_title")}
  legend={$t("agent_sheet.mission_legend")}
  testid="agent-sheet-mission"
  {modified}
>
  {#if view.mission}
    <p class="m-0 whitespace-pre-wrap text-body-sm leading-relaxed text-foreground/85" data-testid="agent-sheet-mission-text">
      {view.mission}
    </p>
  {:else}
    <p class="m-0 text-body-xs italic text-muted-foreground" data-testid="agent-sheet-mission-empty">
      {$t("agent_sheet.mission_empty")}
    </p>
  {/if}
  {#if view.examples.length > 0}
    <ul class="m-0 mt-3 list-none space-y-1.5 p-0" data-testid="agent-sheet-examples">
      {#each view.examples as example (example)}
        <li class="text-body-xs text-foreground/85" data-testid="agent-sheet-example">{example}</li>
      {/each}
    </ul>
  {/if}
</SheetSection>
