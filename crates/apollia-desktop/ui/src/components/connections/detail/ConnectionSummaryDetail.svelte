<script lang="ts">
  /**
   * ConnectionSummaryDetail - the page of a connection that has no dedicated
   * page yet (a mailbox, a folder): its name, account, state, the "Remote"
   * badge, and what it offers. A folder shows its path and can be removed.
   * With the developer mode on, the provenance of the connection and of each
   * of its tools.
   */
  import { t } from "svelte-i18n";
  import { Play, Link as LinkIcon } from "lucide-svelte";
  import { DetailHeader } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { removeFolderConnection } from "$lib/ipc/connections";
  import { reportError } from "$lib/errors/reportError";
  import { Card } from "$lib/components/ui/card";
  import { developerMode } from "$lib/stores/developerMode";
  import { accentTokenFor } from "$lib/connections/accent";
  import { statusOfState } from "$lib/connections/connectionRows";
  import ConnectorTile from "../shared/ConnectorTile.svelte";
  import StatusBadge from "../shared/StatusBadge.svelte";
  import type { Connection } from "$lib/types";

  interface Props {
    connection: Connection;
    label: string;
    /** Called once the connection is removed. */
    onremoved?: () => void;
    /** "Try": a conversation limited to the tools of this connection. */
    ontry?: () => void;
  }

  let { connection, label, onremoved, ontry }: Props = $props();

  let removing = $state(false);

  /** Remove a folder connection; its files stay where they are. */
  async function removeFolder(): Promise<void> {
    if (!confirm($t("connections.folder.remove_confirm", { values: { name: label } }))) return;
    removing = true;
    try {
      await removeFolderConnection(connection.id);
      onremoved?.();
    } catch (err: unknown) {
      reportError(err, { surface: "toast", testid: "connection-folder-remove-error" });
    } finally {
      removing = false;
    }
  }
</script>

<DetailHeader
  title={label}
  titleTestid="connection-detail-title"
  meta={connection.account ?? connection.path ?? ""}
>
  {#snippet leading()}
    <ConnectorTile size="lg" accent={accentTokenFor(label)} glow>
      {#snippet icon()}<LinkIcon size={18} />{/snippet}
    </ConnectorTile>
  {/snippet}
  {#snippet badges()}
    <StatusBadge status={statusOfState(connection.state)} />
    {#if connection.remote}
      <Badge size="sm" variant="info" data-testid="connection-detail-remote">
        {$t("connections.remote")}
      </Badge>
    {/if}
  {/snippet}
  {#snippet actions()}
    {#if connection.kind === "folder"}
      <Button
        variant="outline"
        size="sm"
        disabled={removing}
        onclick={() => void removeFolder()}
        data-testid="connection-folder-remove"
      >
        {$t("connections.folder.remove")}
      </Button>
    {/if}
    {#if ontry}
      <Button variant="primary-solid" size="sm" onclick={ontry} data-testid="connection-try-btn">
        {#snippet icon()}<Play size={12} />{/snippet}
        {$t("connections.try.action")}
      </Button>
    {/if}
  {/snippet}
</DetailHeader>

<div class="flex-1 overflow-auto px-8 py-5">
  <Card class="max-w-3xl px-4 py-3.5" data-testid="connection-detail-tools">
    <div class="mb-2.5 flex items-center justify-between">
      <span class="text-label-md font-semibold text-foreground">{$t("connections.tools_exposed")}</span>
      <span class="text-caption text-muted-foreground">{connection.tools.length}</span>
    </div>
    {#if connection.tools.length === 0}
      <p class="py-4 text-center text-caption text-muted-foreground">
        {$t("integrations.manage.tools_empty")}
      </p>
    {:else}
      <ul class="space-y-1">
        {#each connection.tools as tool (tool.id)}
          <li class="text-body-xs text-foreground" data-testid="connection-tool-{tool.id}">
            {tool.id}
            {#if $developerMode}
              <span class="ml-2 font-mono text-caption text-muted-foreground" data-testid="connection-tool-provenance">
                {connection.kind} · {connection.provenance} · {tool.family} ({tool.familySource})
              </span>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </Card>
  {#if $developerMode}
    <p class="mt-3 font-mono text-caption text-muted-foreground" data-testid="connection-detail-provenance">
      {connection.id} · {connection.provenance}{connection.official
        ? ` · ${$t("connections.badge_official")}`
        : ""}
    </p>
  {/if}
</div>
