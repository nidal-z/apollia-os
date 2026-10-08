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

  /** Past this many characters a mission opens folded. */
  const LONG_MISSION = 480;
  const long = $derived((view.mission ?? "").length > LONG_MISSION);
  let unfolded = $state(false);
</script>

<SheetSection
  title={$t("agent_sheet.mission_title")}
  legend={$t("agent_sheet.mission_legend")}
  testid="agent-sheet-mission"
  {modified}
>
  {#if view.mission}
    <!-- A long mission opens on its first lines; the rest is one gesture away. -->
    <p
      class="m-0 whitespace-pre-wrap text-body-sm leading-relaxed text-foreground/85 {long && !unfolded
        ? 'line-clamp-6'
        : ''}"
      data-testid="agent-sheet-mission-text"
    >
      {view.mission}
    </p>
    {#if long}
      <button
        type="button"
        class="mt-2 rounded-sm text-caption font-medium text-primary underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
        aria-expanded={unfolded}
        onclick={() => (unfolded = !unfolded)}
      >
        {$t(unfolded ? "agent_sheet.mission_less" : "agent_sheet.mission_more")}
      </button>
    {/if}
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
