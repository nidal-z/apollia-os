<script lang="ts">
  /**
   * AdjustAccess - the accesses of the agent, editable family by family: each
   * access a switch, and a dialog to grant a tool (native, of a connection or
   * of a package), another agent or a workflow, and one to grant the tools of
   * a connection family by family. A switch turned off during the session
   * leaves the access listed, so it can be granted back before saving. An
   * access that comes from a remote connection carries the "Remote" badge.
   */
  import { t } from "svelte-i18n";
  import { Plus } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Toggle } from "$lib/components/ui/toggle";
  import { DeveloperOnly } from "$lib/components/shared";
  import SheetSection from "./SheetSection.svelte";
  import GrantDialog from "./GrantDialog.svelte";
  import ConnectionGrantDialog from "./ConnectionGrantDialog.svelte";
  import type { ConnectionOffer } from "./connectionGrant";
  import {
    ACCESS_GROUP_LABEL,
    ACCESS_GROUP_RULE,
    PROVENANCE_LABEL,
    type AccessGroup,
  } from "./agentSheet";
  import { grant, revoke, type AdjustDraft, type GrantCandidate } from "./agentAdjust";

  interface Props {
    draft: AdjustDraft;
    /** The accesses listed: in force, and granted during the session. */
    groups: AccessGroup[];
    /** The accesses in force when the session began. */
    inForce: string[];
    modified: boolean;
    /** What the grant dialog offers, read when it opens. */
    loadCandidates: () => Promise<GrantCandidate[]>;
    /** Name of the agent, for the connection dialog. */
    agentName: string;
    /** What the connection dialog offers, read when it opens. */
    loadConnectionOffer: () => Promise<ConnectionOffer>;
    /** The accesses that come from a remote connection. */
    remote: Set<string>;
  }

  let { draft, groups, inForce, modified, loadCandidates, agentName, loadConnectionOffer, remote }: Props =
    $props();

  let granting = $state(false);
  let grantingConnection = $state(false);

  function onSwitch(id: string, on: boolean): void {
    draft.access = on ? grant(draft.access, id) : revoke(draft.access, id);
  }

  function onGrant(ids: string[]): void {
    let next = draft.access;
    for (const id of ids) next = grant(next, id);
    draft.access = next;
    granting = false;
    grantingConnection = false;
  }
</script>

<SheetSection
  title={$t("agent_sheet.access_title")}
  legend={$t("agent_adjust.access_legend")}
  testid="agent-sheet-access"
  {modified}
>
  {#snippet action()}
    <Button
      variant="outline"
      size="sm"
      onclick={() => (grantingConnection = true)}
      data-testid="agent-adjust-grant-connection"
    >
      {#snippet icon()}<Plus size={12} />{/snippet}
      {$t("agent_adjust.connection_open")}
    </Button>
    <Button variant="outline" size="sm" onclick={() => (granting = true)} data-testid="agent-adjust-grant">
      {#snippet icon()}<Plus size={12} />{/snippet}
      {$t("agent_adjust.grant_open")}
    </Button>
  {/snippet}
  <div class="space-y-3">
    {#each groups as group (group.key)}
      <div data-testid="agent-sheet-access-group" data-family={group.key}>
        <div class="flex items-baseline justify-between gap-2">
          <span class="text-body-xs font-semibold text-foreground">{$t(ACCESS_GROUP_LABEL[group.key])}</span>
          <span class="text-caption text-muted-foreground">{$t(ACCESS_GROUP_RULE[group.key])}</span>
        </div>
        {#if group.accesses.length === 0}
          <p class="m-0 mt-1 text-caption italic text-muted-foreground">{$t("agent_sheet.access_none_in_family")}</p>
        {:else}
          <ul class="m-0 mt-1 list-none space-y-1 p-0">
            {#each group.accesses as access (access.id)}
              {@const on = draft.access.includes(access.id)}
              <li
                class="flex items-center gap-2 border-b border-border/40 py-1.5 last:border-b-0"
                data-testid="agent-adjust-access-row"
                data-access={access.id}
              >
                <div class="min-w-0 flex-1">
                  <div class="text-body-xs text-foreground/85">{access.permits ?? access.label}</div>
                  <DeveloperOnly>
                    <div class="mt-0.5 flex flex-wrap items-center gap-1.5">
                      <code class="font-mono text-caption text-muted-foreground">{access.id}</code>
                      <Badge size="sm" variant="neutral">{$t(PROVENANCE_LABEL[access.provenance])}</Badge>
                    </div>
                  </DeveloperOnly>
                </div>
                {#if remote.has(access.id)}
                  <Badge size="sm" variant="info" data-testid="agent-sheet-access-remote">{$t("agent_sheet.remote")}</Badge>
                {/if}
                {#if on !== inForce.includes(access.id)}
                  <Badge size="sm" variant="warning">{$t("agent_adjust.modified")}</Badge>
                {/if}
                <Toggle
                  size="sm"
                  checked={on}
                  onchange={(value) => onSwitch(access.id, value)}
                  aria-label={access.permits ?? access.label}
                  data-testid="agent-adjust-access-switch"
                />
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    {/each}
  </div>
</SheetSection>

<GrantDialog
  open={granting}
  granted={draft.access}
  {loadCandidates}
  onclose={() => (granting = false)}
  onconfirm={onGrant}
/>

<ConnectionGrantDialog
  open={grantingConnection}
  {agentName}
  granted={draft.access}
  loadOffer={loadConnectionOffer}
  onclose={() => (grantingConnection = false)}
  onconfirm={onGrant}
/>
