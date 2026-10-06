<script lang="ts">
  /**
   * JournalEntryTab - the "Entry" tab of the detail sidebar: the journal entry
   * the user asked to see more of, with everything it records.
   */
  import { t } from "svelte-i18n";
  import { Badge } from "$lib/components/ui/badge";
  import { journalSelection } from "$lib/stores/journalSelection";
  import { APOLLIA_AGENT_ID } from "$lib/chat/chatTargets";
  import JournalEntryDetail from "./JournalEntryDetail.svelte";
  import { STATE_LABEL, STATE_TONE, metaOf, sentenceOf, stateOf } from "./journal";

  function agentName(agent: string | null): string {
    if (agent === null) return $t("journal.agent_unknown");
    return agent === APOLLIA_AGENT_ID ? $t("authorizations.apollia_name") : agent;
  }
</script>

{#if $journalSelection !== null}
  {@const line = $journalSelection.line}
  {@const sentence = sentenceOf(line, agentName)}
  {@const meta = metaOf(line)}
  {@const lineState = stateOf(line)}
  <div class="flex h-full flex-col gap-3 overflow-y-auto p-4" data-testid="detail-sidebar-entry">
    <h3 class="m-0 text-body font-medium text-foreground">
      {$t(sentence.key, { values: sentence.values })}
    </h3>
    <p class="m-0 break-words text-caption text-muted-foreground">
      {#if meta.target !== null}{$t("journal.target", { values: { target: meta.target } })} · {/if}{$t(meta.categoryKey)}
    </p>
    {#if lineState !== null}
      <div>
        <Badge variant={STATE_TONE[lineState]} outline>{$t(STATE_LABEL[lineState])}</Badge>
      </div>
    {/if}
    <JournalEntryDetail
      {line}
      state={$journalSelection.state}
      agentName={agentName(line.summary.agent)}
      layout="stacked"
    />
  </div>
{/if}
