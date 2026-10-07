<script lang="ts">
  /**
   * FolderConnectionDialog - what follows the system picker when a folder is
   * added as a connection: the folder added, its readable path and what the
   * agents may do in it; or the refusal of the admission; or the failure.
   * "Choose another folder" opens the picker again.
   */
  import { t } from "svelte-i18n";
  import { CircleCheck, Folder, Play, TriangleAlert, XCircle } from "lucide-svelte";
  import { Dialog } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import ConnectionRefusal from "../../integrations/ConnectionRefusal.svelte";
  import {
    folderAbilities,
    readablePath,
    type FolderOutcome,
  } from "$lib/connections/folderConnection";
  import type { Connection } from "$lib/types";

  type Shown = Exclude<FolderOutcome, { kind: "cancelled" }>;

  interface Props {
    open: boolean;
    outcome: Shown | null;
    onclose: () => void;
    onchooseanother: () => void;
    /** Open the place where the refusing rule is read. */
    onopenrule: () => void;
    /** "Try" the folder just added. */
    ontry?: (connection: Connection) => void;
  }

  let { open, outcome, onclose, onchooseanother, onopenrule, ontry }: Props = $props();

  const added = $derived(outcome?.kind === "added" ? outcome.connection : null);
  const abilities = $derived(added ? folderAbilities(added) : null);
  const title = $derived(
    added
      ? $t("connections.folder.added_title", { values: { name: added.label } })
      : $t("connections.folder.not_added_title"),
  );
</script>

<Dialog {open} {onclose} size="md" {title} data-testid="folder-connection-dialog">
  {#if added && abilities}
    <div class="space-y-4" data-testid="folder-connection-added">
      <p class="text-caption text-muted-foreground">{$t("connections.folder.added_subtitle")}</p>
      <div
        class="flex items-center gap-2 rounded-md bg-muted px-3 py-2 text-body-sm text-foreground"
        title={added.path ?? ""}
        data-testid="folder-connection-path"
      >
        <Folder size={14} class="shrink-0 text-muted-foreground" aria-hidden="true" />
        <span class="truncate">{readablePath(added.path ?? added.label)}</span>
      </div>
      <div class="space-y-2">
        <p class="text-caption text-muted-foreground">{$t("connections.folder.abilities_title")}</p>
        {#if abilities.read}
          <p class="flex items-center gap-2 text-body-sm text-foreground" data-testid="folder-ability-read">
            <CircleCheck size={14} class="shrink-0 text-success" aria-hidden="true" />
            {$t("connections.folder.ability_read")}
          </p>
        {/if}
        {#if abilities.writeOrDelete}
          <p class="flex items-center gap-2 text-body-sm text-foreground" data-testid="folder-ability-write">
            <TriangleAlert size={14} class="shrink-0 text-warning" aria-hidden="true" />
            {$t("connections.folder.ability_write")}
          </p>
        {/if}
      </div>
      <p class="text-body-sm text-muted-foreground">{$t("connections.folder.bounds")}</p>
    </div>
  {:else if outcome?.kind === "refused"}
    <ConnectionRefusal refusal={outcome.refusal} onopen={onopenrule} />
  {:else if outcome?.kind === "failed"}
    <div
      class="space-y-2 rounded-md border border-destructive/30 bg-destructive/5 px-4 py-3"
      data-testid="folder-connection-error"
    >
      <div class="flex items-center gap-2">
        <XCircle size={16} class="shrink-0 text-destructive" aria-hidden="true" />
        <p class="text-sm font-medium text-destructive">{$t("connections.folder.add_failed")}</p>
      </div>
      <p class="break-all text-xs text-muted-foreground">{outcome.message}</p>
    </div>
  {/if}

  <div class="mt-6 flex items-center justify-end gap-2">
    <Button variant="ghost" size="sm" onclick={onchooseanother} data-testid="folder-connection-choose-another">
      {$t("connections.folder.choose_another")}
    </Button>
    {#if added && ontry}
      <Button variant="outline" size="sm" onclick={() => ontry(added)} data-testid="folder-connection-try">
        {#snippet icon()}<Play size={12} />{/snippet}
        {$t("connections.try.action")}
      </Button>
    {/if}
    <Button variant="primary-solid" size="sm" onclick={onclose} data-testid="folder-connection-done">
      {$t("connections.folder.done")}
    </Button>
  </div>
</Dialog>
