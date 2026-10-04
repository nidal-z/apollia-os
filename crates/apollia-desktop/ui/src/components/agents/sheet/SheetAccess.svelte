<script lang="ts">
  /**
   * SheetAccess - the accesses granted to the agent, grouped by action
   * family whatever their provenance, each with what it lets the agent do.
   * Every family is listed, an empty one says so. The raw identifier and the
   * provenance of an access appear in developer mode only. An access that
   * comes from a remote connection carries the "Remote" badge.
   */
  import { t } from "svelte-i18n";
  import { Badge } from "$lib/components/ui/badge";
  import { DeveloperOnly } from "$lib/components/shared";
  import SheetSection from "./SheetSection.svelte";
  import {
    ACCESS_GROUP_LABEL,
    ACCESS_GROUP_RULE,
    PROVENANCE_LABEL,
    type AccessGroup,
  } from "./agentSheet";

  interface Props {
    groups: AccessGroup[];
    loading: boolean;
    /** Whether the value shown is the user's rather than the package's. */
    modified?: boolean;
    /** The accesses that come from a remote connection. */
    remote?: Set<string>;
  }

  let { groups, loading, modified = false, remote = new Set<string>() }: Props = $props();
</script>

<SheetSection
  title={$t("agent_sheet.access_title")}
  legend={$t("agent_sheet.access_legend")}
  testid="agent-sheet-access"
  {modified}
>
  {#if loading}
    <p class="m-0 text-body-xs text-muted-foreground">{$t("common.loading")}</p>
  {:else}
    <div class="space-y-3">
      {#each groups as group (group.key)}
        <div data-testid="agent-sheet-access-group" data-family={group.key}>
          <div class="flex items-baseline justify-between gap-2">
            <span class="text-body-xs font-semibold text-foreground">
              {$t(ACCESS_GROUP_LABEL[group.key])}
            </span>
            <span class="text-caption text-muted-foreground">
              {$t(ACCESS_GROUP_RULE[group.key])}
            </span>
          </div>
          {#if group.accesses.length === 0}
            <p class="m-0 mt-1 text-caption italic text-muted-foreground" data-testid="agent-sheet-access-empty">
              {$t("agent_sheet.access_none_in_family")}
            </p>
          {:else}
            <ul class="m-0 mt-1 list-none space-y-1 p-0">
              {#each group.accesses as access (access.id)}
                <li
                  class="flex items-start gap-2 border-b border-border/40 py-1.5 last:border-b-0"
                  data-testid="agent-sheet-access-row"
                >
                  <div class="min-w-0 flex-1">
                    <div class="text-body-xs text-foreground/85">
                      {access.permits ?? access.label}
                    </div>
                    <DeveloperOnly>
                      <div class="mt-0.5 flex flex-wrap items-center gap-1.5">
                        <code class="font-mono text-caption text-muted-foreground" data-testid="agent-sheet-access-id">
                          {access.id}
                        </code>
                        <Badge size="sm" variant="neutral" data-testid="agent-sheet-access-provenance">
                          {$t(PROVENANCE_LABEL[access.provenance])}
                        </Badge>
                      </div>
                    </DeveloperOnly>
                  </div>
                  {#if remote.has(access.id)}
                    <Badge size="sm" variant="info" data-testid="agent-sheet-access-remote">
                      {$t("agent_sheet.remote")}
                    </Badge>
                  {/if}
                  {#if access.sensitive}
                    <Badge size="sm" variant="warning">{$t("agent_sheet.access_sensitive")}</Badge>
                  {/if}
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</SheetSection>
