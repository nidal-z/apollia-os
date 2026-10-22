<script lang="ts">
  /**
   * TrialSucceeded - the trial of a connection just added answered: where to
   * find it, what a mailbox is never used for, and, for a folder, its path,
   * what the agents may do in it and the offer to choose another one.
   */
  import { t } from "svelte-i18n";
  import { CheckCircle2, Folder } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { folderAbilities, readablePath } from "$lib/connections/folderConnection";
  import type { AddEntry } from "$lib/connections/addWindow";
  import type { Connection } from "$lib/types";

  interface Props {
    kind: AddEntry["kind"];
    name: string;
    /** The tools the trial found, null when there was none to run. */
    tools: number | null;
    /** The folder added, for a folder. */
    folder?: Connection | null;
    /** Open the system picker again, for a folder. */
    onchooseanother?: () => void;
  }

  let { kind, name, tools, folder = null, onchooseanother }: Props = $props();

  const abilities = $derived(folder ? folderAbilities(folder) : null);
</script>

<div class="rounded-md border border-success/30 bg-success/10 px-4 py-3" data-testid="test-result-success">
  <div class="flex items-center gap-2">
    <CheckCircle2 size={16} class="shrink-0 text-success" aria-hidden="true" />
    <p class="text-sm font-medium text-success-a11y">
      {tools !== null
        ? $t("integrations.wizard.added_success", { values: { count: tools } })
        : $t("connections.add_flow.added", { values: { name } })}
    </p>
  </div>
  {#if kind === "mailbox"}
    <p class="mt-1 text-xs text-muted-foreground" data-testid="add-flow-mailbox-read-only">
      {$t("connections.mailbox.read_only")}
    </p>
  {/if}
  <p class="mt-1 text-xs text-muted-foreground">{$t("connections.add_flow.added_where")}</p>
</div>
{#if folder && abilities}
  <div
    class="flex items-center gap-2 rounded-md bg-muted px-3 py-2 text-body-sm text-foreground"
    title={folder.path ?? ""}
    data-testid="folder-connection-path"
  >
    <Folder size={14} class="shrink-0 text-muted-foreground" aria-hidden="true" />
    <span class="truncate">{readablePath(folder.path ?? folder.label)}</span>
  </div>
  {#if abilities.read}
    <p class="text-caption text-muted-foreground" data-testid="folder-ability-read">
      {$t("connections.folder.ability_read")}
    </p>
  {/if}
  {#if abilities.writeOrDelete}
    <p class="text-caption text-muted-foreground" data-testid="folder-ability-write">
      {$t("connections.folder.ability_write")}
    </p>
  {/if}
  {#if onchooseanother}
    <Button variant="ghost" size="sm" onclick={onchooseanother} data-testid="folder-connection-choose-another">
      {#snippet icon()}<Folder size={12} />{/snippet}
      {$t("connections.folder.choose_another")}
    </Button>
  {/if}
{/if}
