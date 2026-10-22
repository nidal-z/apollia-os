<script lang="ts">
  /**
   * SourceRow - one source of an agent's sheet: its kind and name, where its
   * index stands (waiting, being read with the files read so far, ready,
   * failed, keywords only with a link to the AI screen), and the files left
   * out, over 50 MB or unreadable. In adjustment the row carries its box and,
   * for a source of the registry, its removal. A source read through a
   * connection its user paused carries the "Paused" badge.
   */
  import type { Snippet } from "svelte";
  import { t } from "svelte-i18n";
  import { BookUser, Brain, CircleHelp, FileText, Folder, FolderCode, Mail, Trash2 } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { navigateTo } from "$lib/stores/navigation";
  import {
    SKIP_REASON_KEY,
    SOURCE_NAME_KEY,
    SOURCE_STATE_BADGE,
    sourceStatusKey,
    type SourceRow,
  } from "./agentSources";

  interface Props {
    row: SourceRow;
    /** Checks or unchecks the source; absent in reading. */
    ontoggle?: (checked: boolean) => void;
    /** Removes the source from the registry; absent in reading. */
    onremove?: () => void;
    /** What follows the row: a note about what the agent may read. */
    note?: Snippet;
    /** Whether the connection it is read through is paused. */
    paused?: boolean;
  }

  let { row, ontoggle, onremove, note, paused = false }: Props = $props();

  let showSkipped = $state(false);

  const ICON = {
    folder: Folder,
    file: FileText,
    mail: Mail,
    code_folder: FolderCode,
    profile: BookUser,
    memory: Brain,
    missing: CircleHelp,
  } as const;

  const Icon = $derived(ICON[row.kind]);
  const skippedCount = $derived(row.tooLarge + row.unreadable);
  const nameKey = $derived(SOURCE_NAME_KEY[row.kind]);
  const name = $derived(nameKey ? $t(nameKey, { values: { name: row.label } }) : row.label);
  const status = $derived($t(sourceStatusKey(row), { values: { count: row.documents } }));
  const badge = $derived(SOURCE_STATE_BADGE[row.state]);
</script>

<li class="py-2" data-testid="agent-sheet-source" data-source={row.kind} data-state={row.state} data-paused={paused ? "true" : undefined}>
  <div class="flex items-start gap-2.5">
    {#if ontoggle}
      <Checkbox
        id={`agent-adjust-source-${row.id}`}
        class="mt-0.5"
        checked={row.checked}
        onchange={ontoggle}
        data-testid="agent-adjust-source-check"
      />
    {/if}
    <Icon size={14} class="mt-0.5 shrink-0 text-muted-foreground" aria-hidden="true" />
    <div class="min-w-0 flex-1">
      {#if ontoggle}
        <label for={`agent-adjust-source-${row.id}`} class="block truncate text-body-xs text-foreground/85">
          {name}
        </label>
      {:else}
        <span class="block truncate text-body-xs text-foreground/85">{name}</span>
      {/if}
      <p class="m-0 text-caption text-muted-foreground" data-testid="agent-sheet-source-status">
        {status}
        {#if skippedCount > 0}
          ·
          <button
            type="button"
            class="underline underline-offset-2 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
            aria-expanded={showSkipped}
            onclick={() => (showSkipped = !showSkipped)}
            data-testid="agent-sheet-source-skipped"
          >
            {$t("agent_sources.skipped", { values: { count: skippedCount } })}
          </button>
        {/if}
        {#if row.state === "keywords_only"}
          <button
            type="button"
            class="ml-1 text-primary underline underline-offset-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
            onclick={() => navigateTo("llm")}
            data-testid="agent-sheet-source-open-ai"
          >
            {$t("agent_sources.open_ai")}
          </button>
        {/if}
      </p>
      {#if showSkipped && skippedCount > 0}
        <ul class="m-0 mt-1 list-none space-y-0.5 p-0" data-testid="agent-sheet-source-skipped-list">
          {#each row.skipped as file (file.path)}
            <li class="truncate text-caption text-muted-foreground">
              {file.path} · {$t(SKIP_REASON_KEY[file.reason])}
            </li>
          {/each}
        </ul>
      {/if}
      {@render note?.()}
    </div>
    {#if paused}
      <Badge size="sm" variant="warning">{$t("agent_sheet.paused")}</Badge>
    {/if}
    {#if badge}
      <Badge size="sm" variant={badge.tone} data-testid="agent-sheet-source-state">{$t(badge.label)}</Badge>
    {/if}
    {#if onremove && row.registered}
      <Button
        variant="ghost"
        size="sm"
        aria-label={$t("agent_sources.remove_label", { values: { name } })}
        onclick={onremove}
        data-testid="agent-adjust-source-remove"
      >
        {#snippet icon()}<Trash2 size={12} />{/snippet}
      </Button>
    {/if}
  </div>
</li>
