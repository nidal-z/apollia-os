<script lang="ts">
  /**
   * AddSourceDialog - the connection a new source is made on: a folder
   * connection for a folder or a code folder, a mailbox connection for a
   * mailbox. Choosing one adds the source. A folder of this machine is
   * picked on the spot, and connected; a mailbox with no connection sends to
   * Connections, where one is signed in.
   */
  import { t } from "svelte-i18n";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { navigateTo } from "$lib/stores/navigation";
  import type { SourceCandidate } from "./agentSources";

  interface Props {
    open: boolean;
    /** What is being added. */
    kind: "folder" | "code_folder" | "mail";
    candidates: SourceCandidate[];
    loading: boolean;
    onclose: () => void;
    onchoose: (candidate: SourceCandidate) => void;
    /** A folder of this machine picked here, to be connected. */
    onpick?: (path: string) => void;
  }

  let { open, kind, candidates, loading, onclose, onchoose, onpick }: Props = $props();

  async function pickFolder(): Promise<void> {
    const picked = await openDialog({ directory: true, multiple: false, title: $t("agent_sources.pick_folder_title") });
    if (typeof picked === "string" && picked) onpick?.(picked);
  }

  const title = $derived(kind === "mail" ? $t("agent_sources.pick_mail_title") : $t("agent_sources.pick_folder_title"));
  const empty = $derived(kind === "mail" ? $t("agent_sources.pick_mail_empty") : $t("agent_sources.pick_folder_empty"));

  function openConnections(): void {
    onclose();
    navigateTo("integrations");
  }
</script>

<Dialog {open} {onclose} size="sm" {title} data-testid="agent-source-pick-dialog">
  {#if loading}
    <p class="m-0 text-body-xs text-muted-foreground">{$t("common.loading")}</p>
  {:else if candidates.length === 0}
    <p class="m-0 text-body-xs text-muted-foreground" data-testid="agent-source-pick-empty">{empty}</p>
  {:else}
    <ul class="m-0 list-none space-y-1 p-0">
      {#each candidates as candidate (candidate.reference)}
        <li>
          <button
            type="button"
            class="flex w-full flex-col rounded-md px-3 py-2 text-left transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
            onclick={() => onchoose(candidate)}
            data-testid="agent-source-pick-candidate"
          >
            <span class="truncate text-body-xs text-foreground">{candidate.label}</span>
            {#if candidate.detail}
              <span class="truncate text-caption text-muted-foreground">{candidate.detail}</span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
  <DialogFooter>
    <Button variant="outline" onclick={onclose} data-testid="agent-source-pick-cancel">{$t("common.cancel")}</Button>
    {#if kind !== "mail" && onpick}
      <Button
        variant={candidates.length === 0 ? "primary-solid" : "outline"}
        onclick={() => void pickFolder()}
        data-testid="agent-source-pick-local-folder"
      >
        {$t("agent_sources.pick_local_folder")}
      </Button>
    {:else if !loading && candidates.length === 0}
      <Button variant="primary-solid" onclick={openConnections} data-testid="agent-source-pick-connections">
        {$t("agent_sources.open_connections")}
      </Button>
    {/if}
  </DialogFooter>
</Dialog>
