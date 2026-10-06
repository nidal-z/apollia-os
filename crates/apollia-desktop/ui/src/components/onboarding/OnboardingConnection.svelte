<script lang="ts">
  /**
   * First-launch step 6 - a useful connection, optional.
   *
   * One line per connection the agent kept at the step before requires and
   * this machine lacks. "Add" opens the add dialog of the Connections screen
   * on that entry of the selection, above this step; once the connection is
   * added its line shows as present. "Skip" is offered whatever the lines
   * show: nothing here is needed to finish the first launch.
   *
   * The modal mounts this step only when there is something to propose. The
   * decisions live in `connectionStep.ts`; this component renders them.
   */
  import { untrack } from "svelte";
  import { t } from "svelte-i18n";
  import { CircleCheck, Plug } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import ConnectorWizard from "../integrations/ConnectorWizard.svelte";
  import {
    allPresent,
    entryToAdd,
    refreshedLines,
    subtitleKey,
    type ConnectionLine,
    type ConnectionProposal,
  } from "./connectionStep";
  import { connectionSources } from "./connectionStepLive";
  import "./onboarding-ai-setup.css";

  interface Props {
    /** The agent and the connections it lacks, read before the step opened. */
    proposal: ConnectionProposal;
    /** On to the step after this one. */
    onnext: () => void;
    /** Back to the first agent. */
    onback: () => void;
  }

  const { proposal, onnext, onback }: Props = $props();

  // The lines the step opened with; they are then kept here, where a
  // connection added turns its line to present.
  let lines = $state<ConnectionLine[]>(untrack(() => proposal.lines));
  // The line the add dialog is open on.
  let adding = $state<string | null>(null);
  let added = new Set<string>();

  const addingEntry = $derived(adding === null ? null : entryToAdd(lines, adding));
  const done = $derived(allPresent(lines));

  // The add dialog says the connection exists: its line is present at once,
  // then the list is read again for the lines another gesture may have filled.
  async function markAdded(type: string): Promise<void> {
    added = new Set(added).add(type);
    lines = refreshedLines(lines, [], added);
    try {
      lines = refreshedLines(lines, await connectionSources.connections(), added);
    } catch {
      // The list could not be read again: the line the dialog added stays present.
    }
  }
</script>

<div class="ai-setup" data-testid="onboarding-connection" data-done={done}>
  <header class="setup-header">
    <h2 class="setup-title">{$t("onboarding.connection.title")}</h2>
    <p class="setup-subtitle" data-testid="connection-subtitle">
      {$t(subtitleKey(lines), { values: { name: proposal.agentName } })}
    </p>
  </header>

  <ul class="flex flex-col gap-3" data-testid="connection-lines">
    {#each lines as line (line.type)}
      <li
        class="flex items-center gap-3 rounded-lg border border-border bg-card px-4 py-3"
        data-testid="connection-line"
        data-connection={line.type}
        data-state={line.state}
      >
        <span
          class="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-muted text-primary"
          aria-hidden="true"
        >
          <Plug size={14} strokeWidth={2} />
        </span>
        <span class="flex min-w-0 flex-1 flex-col gap-1">
          <span class="text-body-sm font-medium text-foreground">{line.label}</span>
          {#if line.reason}
            <span class="text-caption text-muted-foreground" data-testid="connection-line-reason">
              {line.reason}
            </span>
          {/if}
        </span>
        {#if line.state === "present"}
          <span
            class="flex shrink-0 items-center gap-1.5 text-caption text-success"
            role="status"
            data-testid="connection-line-present"
          >
            <CircleCheck size={14} strokeWidth={2} aria-hidden="true" />
            {$t("onboarding.connection.present")}
          </span>
        {:else if line.entry !== null}
          <Button
            variant="primary-solid"
            size="sm"
            onclick={() => (adding = line.type)}
            data-testid="connection-line-add"
          >
            {$t("onboarding.connection.add")}
          </Button>
        {:else}
          <span class="shrink-0 text-caption text-muted-foreground" data-testid="connection-line-later">
            {$t("onboarding.connection.add_later")}
          </span>
        {/if}
      </li>
    {/each}
  </ul>

  <footer class="setup-footer">
    <Button variant="ghost" size="sm" class="btn-secondary" onclick={onback} data-testid="connection-back">
      ← {$t("common.back")}
    </Button>
    <div class="footer-right">
      <Button variant="outline" size="default" onclick={onnext} data-testid="connection-skip">
        {$t("onboarding.connection.skip")}
      </Button>
      <Button variant="primary-gradient" size="default" onclick={onnext} data-testid="connection-continue">
        {$t("common.continue")}
      </Button>
    </div>
  </footer>
</div>

{#if adding !== null && addingEntry !== null}
  <ConnectorWizard
    server={addingEntry}
    open={true}
    onclose={() => (adding = null)}
    oncomplete={() => {
      if (adding !== null) void markAdded(adding);
    }}
  />
{/if}
