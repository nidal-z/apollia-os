<script lang="ts">
  /**
   * InactiveServerDetail - the page of a server the runtime does not run:
   * one that was stopped, that failed to start, or that a test left behind.
   * It has no tools to show, but it can be started again or removed, so it
   * never stays in the list with no way out.
   */
  import { t } from "svelte-i18n";
  import { Link as LinkIcon, RefreshCw, Trash2 } from "lucide-svelte";
  import { DetailHeader } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { Card } from "$lib/components/ui/card";
  import { removeMcpServer, restartMcpServer } from "$lib/ipc/connections";
  import { reportError } from "$lib/errors/reportError";
  import StatusBadge from "../shared/StatusBadge.svelte";

  interface Props {
    /** The name the runtime knows the server by. */
    name: string;
    label: string;
    /** Called once the server started again. */
    onchanged: () => void;
    /** Called once the server is removed. */
    onremoved: () => void;
  }

  let { name, label, onchanged, onremoved }: Props = $props();

  let busy = $state(false);
  let confirming = $state(false);

  async function reconnect(): Promise<void> {
    busy = true;
    try {
      await restartMcpServer(name);
      onchanged();
    } catch (err: unknown) {
      reportError(err, { surface: "toast" });
    } finally {
      busy = false;
    }
  }

  async function remove(): Promise<void> {
    busy = true;
    try {
      await removeMcpServer(name);
      onremoved();
    } catch (err: unknown) {
      reportError(err, { surface: "toast" });
    } finally {
      busy = false;
      confirming = false;
    }
  }
</script>

<DetailHeader title={label} titleTestid="connection-detail-title" meta={name}>
  {#snippet leading()}
    <span class="inline-flex h-10 w-10 items-center justify-center rounded-lg bg-muted text-foreground">
      <LinkIcon size={18} />
    </span>
  {/snippet}
  {#snippet badges()}
    <StatusBadge status="idle" />
  {/snippet}
  {#snippet actions()}
    {#if confirming}
      <Button variant="outline" size="sm" disabled={busy} onclick={() => (confirming = false)}>
        {$t("common.cancel")}
      </Button>
      <Button
        variant="destructive"
        size="sm"
        disabled={busy}
        onclick={() => void remove()}
      >
        {#snippet icon()}<Trash2 size={12} />{/snippet}
        {$t("connections.inactive.remove_confirm")}
      </Button>
    {:else}
      <Button
        variant="ghost"
        size="sm"
        class="text-muted-foreground"
        disabled={busy}
        onclick={() => (confirming = true)}
      >
        {#snippet icon()}<Trash2 size={12} />{/snippet}
        {$t("connections.inactive.remove")}
      </Button>
      <Button
        variant="primary-solid"
        size="sm"
        disabled={busy}
        loading={busy}
        onclick={() => void reconnect()}
      >
        {#snippet icon()}<RefreshCw size={12} />{/snippet}
        {$t("connections.inactive.reconnect")}
      </Button>
    {/if}
  {/snippet}
</DetailHeader>

<div class="flex-1 overflow-auto px-8 py-5">
  <Card class="max-w-3xl px-4 py-3.5">
    <p class="text-body-sm font-medium text-foreground">{$t("connections.inactive.title")}</p>
    <p class="mt-1 text-body-sm text-muted-foreground">{$t("connections.inactive.body")}</p>
  </Card>
</div>
