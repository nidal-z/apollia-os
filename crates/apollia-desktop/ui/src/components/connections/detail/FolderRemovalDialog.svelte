<script lang="ts">
  /**
   * FolderRemovalDialog - the confirmation before a folder connection is
   * removed. When sources were made on the folder, it lists them with where
   * their index stands: they are removed with the connection, and the agents
   * that consult them lose them. The files of the folder are never touched.
   */
  import { t } from "svelte-i18n";
  import { Loader2 } from "lucide-svelte";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import type { DependentSource } from "$lib/connections/folderRemoval";
  import { SOURCE_STATE_BADGE, type SourceRowState } from "../../agents/sheet/agentSources";

  interface Props {
    open: boolean;
    /** Name of the connection. */
    name: string;
    /** The sources made on the folder. */
    dependents: DependentSource[];
    removing: boolean;
    onclose: () => void;
    onconfirm: () => void;
  }

  let { open, name, dependents, removing, onclose, onconfirm }: Props = $props();

  const STATE: Record<DependentSource["indexState"], SourceRowState> = {
    absent: "pending",
    indexing: "indexing",
    ready: "ready",
    error: "error",
    keywords_only: "keywords_only",
  };
  const count = $derived(dependents.length);
</script>

<Dialog
  {open}
  {onclose}
  size="md"
  role="alertdialog"
  title={$t("connections.folder.remove_title", { values: { name } })}
  data-testid="connection-folder-remove-dialog"
>
  <p class="m-0 text-body-sm text-muted-foreground">
    {count > 0
      ? $t("connections.folder.remove_with_sources", { values: { count } })
      : $t("connections.folder.remove_plain")}
  </p>
  {#if count > 0}
    <p class="mb-1 mt-4 text-caption font-semibold uppercase tracking-wide text-muted-foreground">
      {$t("connections.folder.dependent_sources", { values: { count } })}
    </p>
    <ul class="m-0 list-none space-y-1 p-0">
      {#each dependents as source (source.id)}
        {@const badge = SOURCE_STATE_BADGE[STATE[source.indexState]]}
        <li class="flex items-center gap-2 py-1" data-testid="connection-folder-remove-source">
          <span class="min-w-0 flex-1 truncate text-body-xs text-foreground">{source.label}</span>
          {#if badge}
            <Badge size="sm" variant={badge.tone}>{$t(badge.label)}</Badge>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
  <DialogFooter>
    <Button variant="outline" onclick={onclose} data-testid="connection-folder-remove-cancel">
      {$t("common.cancel")}
    </Button>
    <Button variant="destructive" disabled={removing} onclick={onconfirm} data-testid="connection-folder-remove-confirm">
      {#if removing}<Loader2 class="mr-2 h-4 w-4 animate-spin" />{/if}
      {count > 0
        ? $t("connections.folder.remove_confirm_sources", { values: { count } })
        : $t("connections.folder.remove_confirm_plain")}
    </Button>
  </DialogFooter>
</Dialog>
