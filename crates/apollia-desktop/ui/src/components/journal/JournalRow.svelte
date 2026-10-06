<script lang="ts">
  /**
   * JournalRow - one journal entry on one line, in plain language: a glyph,
   * a sentence whose subject is the agent, then what was acted on, the family
   * and the hour, the state, and "see more". Unfolded, the detail of the entry
   * opens under the line.
   */
  import { locale, t } from "svelte-i18n";
  import { Activity, Bot, KeyRound, Landmark, Plug } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import type { JournalLine } from "$lib/ipc/journal";
  import type { JournalDetailState } from "$lib/stores/journalSelection";
  import JournalEntryDetail from "./JournalEntryDetail.svelte";
  import {
    STATE_LABEL,
    STATE_TONE,
    iconOf,
    kindTag,
    lineKey,
    metaOf,
    sentenceOf,
    stateOf,
    type LineIcon,
  } from "./journal";

  interface Props {
    line: JournalLine;
    agentName: (agent: string | null) => string;
    /** The detail is shown under the line. */
    unfolded: boolean;
    /** The detail of this entry is the one the detail sidebar shows. */
    selected: boolean;
    detail: JournalDetailState | null;
    onToggle: (line: JournalLine) => void;
  }

  let { line, agentName, unfolded, selected, detail, onToggle }: Props = $props();

  const ICONS: Record<LineIcon, typeof Bot> = {
    agent: Bot,
    access: KeyRound,
    connection: Plug,
    policy: Landmark,
    system: Activity,
  };

  const sentence = $derived(sentenceOf(line, agentName));
  const meta = $derived(metaOf(line));
  const lineState = $derived(stateOf(line));
  const Icon = $derived(ICONS[iconOf(line)]);
  const open = $derived(unfolded || selected);

  const hour = $derived.by(() => {
    const date = new Date(line.ts);
    if (Number.isNaN(date.getTime())) return "";
    return date.toLocaleTimeString($locale ?? "en", { hour: "2-digit", minute: "2-digit" });
  });
</script>

<li
  class="border-t border-border first:border-t-0 {selected ? 'bg-primary/10' : ''}"
  data-testid="journal-entry"
  data-entry={lineKey(line)}
  data-kind={kindTag(line.kind)}
  data-state={lineState ?? "none"}
>
  <div class="flex items-center gap-3 px-4 py-3">
    <span
      class="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-muted text-primary"
      aria-hidden="true"
    >
      <Icon size={16} strokeWidth={1.5} />
    </span>
    <div class="min-w-0 flex-1">
      <p class="m-0 text-body-sm font-medium text-foreground" data-testid="journal-entry-sentence">
        {$t(sentence.key, { values: sentence.values })}
      </p>
      <p class="m-0 break-words text-caption text-muted-foreground">
        {#if meta.target !== null}
          <span data-testid="journal-entry-target">{$t("journal.target", { values: { target: meta.target } })}</span>
          ·
        {/if}
        {$t(meta.categoryKey)}{#if hour !== ""}
          · {hour}{/if}
      </p>
    </div>
    {#if lineState !== null}
      <Badge variant={STATE_TONE[lineState]} outline data-testid="journal-entry-state">
        {$t(STATE_LABEL[lineState])}
      </Badge>
    {/if}
    <Button
      variant="link"
      size="sm"
      aria-expanded={open}
      onclick={() => onToggle(line)}
      data-testid="journal-entry-toggle"
    >
      {$t(open ? "journal.see_less" : "journal.see_more")}
    </Button>
  </div>
  {#if unfolded}
    <div class="px-4 pb-4 pl-[3.75rem]">
      <JournalEntryDetail {line} state={detail} agentName={agentName(line.summary.agent)} />
    </div>
  {/if}
</li>
