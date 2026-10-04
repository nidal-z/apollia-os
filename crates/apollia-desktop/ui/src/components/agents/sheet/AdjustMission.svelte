<script lang="ts">
  /**
   * AdjustMission - the mission and the examples of an agent, editable. The
   * draft is the adjustment session's; nothing is written until it is saved.
   */
  import { t } from "svelte-i18n";
  import { Plus, X } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Textarea } from "$lib/components/ui/textarea";
  import SheetSection from "./SheetSection.svelte";
  import type { AdjustDraft } from "./agentAdjust";

  interface Props {
    draft: AdjustDraft;
    modified: boolean;
  }

  let { draft, modified }: Props = $props();
</script>

<SheetSection
  title={$t("agent_sheet.mission_title")}
  legend={$t("agent_sheet.mission_legend")}
  testid="agent-sheet-mission"
  {modified}
>
  <Textarea
    bind:value={draft.mission}
    rows={4}
    aria-label={$t("agent_adjust.mission_label")}
    placeholder={$t("agent_adjust.mission_placeholder")}
    data-testid="agent-adjust-mission"
  />
  <ul class="m-0 mt-3 list-none space-y-2 p-0">
    {#each draft.examples as _example, index (index)}
      <li class="flex items-center gap-2" data-testid="agent-adjust-example-row">
        <Input
          bind:value={draft.examples[index]}
          aria-label={$t("agent_adjust.example_label", { values: { n: index + 1 } })}
          data-testid="agent-adjust-example"
        />
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={$t("agent_adjust.example_remove")}
          onclick={() => draft.examples.splice(index, 1)}
          data-testid="agent-adjust-example-remove"
        >
          <X size={14} />
        </Button>
      </li>
    {/each}
  </ul>
  <Button
    variant="ghost"
    size="sm"
    class="mt-2"
    onclick={() => draft.examples.push("")}
    data-testid="agent-adjust-example-add"
  >
    {#snippet icon()}<Plus size={12} />{/snippet}
    {$t("agent_adjust.example_add")}
  </Button>
</SheetSection>
