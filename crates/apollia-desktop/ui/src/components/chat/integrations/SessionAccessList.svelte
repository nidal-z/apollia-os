<script lang="ts">
  /**
   * SessionAccessList - what the agent reaches in this conversation, group by
   * group, each access a switch, with a filter by name. An access of the
   * agent is switched off for the conversation alone; an access the agent
   * does not hold is listed too, and switching it on proposes it here: each
   * of its calls asks for consent. The raw identifier appears in developer
   * mode only.
   */
  import { t } from "svelte-i18n";
  import { Bot, ListFilter, Plug, Workflow, Wrench } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Input } from "$lib/components/ui/input";
  import { Toggle } from "$lib/components/ui/toggle";
  import { DeveloperOnly } from "$lib/components/shared";
  import { developerMode } from "$lib/stores/developerMode";
  import { TOOL_CATALOG } from "$lib/tools/tool-catalog";
  import {
    SESSION_ACCESS_BADGE,
    SESSION_ACCESS_KIND_LABEL,
    SESSION_ACCESS_NOTE,
    groupSessionAccesses,
    isSwitchedOn,
    type SessionAccessKind,
    type SessionAccessRow,
    type SessionAccessState,
  } from "$lib/chat/sessionAccesses";

  interface Props {
    rows: SessionAccessRow[];
    loading: boolean;
    /** A closed conversation is read only. */
    disabled?: boolean;
    onswitch: (row: SessionAccessRow, on: boolean) => void;
  }

  let { rows, loading, disabled = false, onswitch }: Props = $props();

  let query = $state("");

  const KIND_ICON: Record<SessionAccessKind, typeof Wrench> = {
    tools: Wrench,
    connections: Plug,
    workflows: Workflow,
    agents: Bot,
  };
  const BADGE_VARIANT: Record<SessionAccessState, "neutral" | "warning"> = {
    on: "neutral",
    off: "neutral",
    not_granted: "neutral",
    proposed: "warning",
  };
  const NATIVE_ICON = new Map(TOOL_CATALOG.map((tool) => [tool.id, tool.icon]));

  function nameOf(row: SessionAccessRow): string {
    return row.nameKey ? $t(row.nameKey) : row.name;
  }

  function detailOf(row: SessionAccessRow): string {
    return row.detailKey ? $t(row.detailKey) : row.detail;
  }

  const groups = $derived(groupSessionAccesses(rows, query, nameOf, $developerMode));
</script>

<section class="space-y-3" aria-label={$t("chat.integrations.accesses_title")} data-testid="session-accesses">
  <Input
    size="sm"
    icon={ListFilter}
    bind:value={query}
    placeholder={$t("chat.integrations.filter_placeholder")}
    aria-label={$t("chat.integrations.filter_placeholder")}
    data-testid="session-access-filter"
  />

  {#if loading}
    <p class="m-0 text-caption text-muted-foreground">{$t("common.loading")}</p>
  {:else if groups.length === 0}
    <p class="m-0 text-caption italic text-muted-foreground" data-testid="session-access-empty">
      {rows.length === 0 ? $t("chat.integrations.none") : $t("chat.integrations.none_match")}
    </p>
  {:else}
    {#each groups as group, index (group.kind)}
      <div
        class="space-y-1 {index > 0 ? 'border-t border-border pt-3' : ''}"
        data-testid="session-access-group"
        data-kind={group.kind}
      >
        <p class="m-0 text-caption text-muted-foreground">
          {$t(SESSION_ACCESS_KIND_LABEL[group.kind])}
        </p>
        <ul class="m-0 list-none space-y-0.5 p-0">
          {#each group.rows as row (row.id)}
            {@const RowIcon = NATIVE_ICON.get(row.id) ?? KIND_ICON[row.kind]}
            {@const badge = SESSION_ACCESS_BADGE[row.state]}
            {@const note = SESSION_ACCESS_NOTE[row.state]}
            {@const detail = note ? $t(note) : detailOf(row)}
            <li
              class="flex items-start gap-2.5 py-1.5"
              data-testid="session-access-row"
              data-access={row.id}
              data-state={row.state}
            >
              <RowIcon class="mt-0.5 h-3.5 w-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
              <div class="min-w-0 flex-1">
                <div class="flex flex-wrap items-center gap-1.5">
                  <span class="text-xs font-medium text-foreground">{nameOf(row)}</span>
                  {#if badge}
                    <Badge
                      size="sm"
                      variant={BADGE_VARIANT[row.state]}
                      outline={row.state === "not_granted"}
                      data-testid="session-access-badge"
                    >
                      {$t(badge)}
                    </Badge>
                  {/if}
                </div>
                {#if detail}
                  <p class="m-0 mt-0.5 text-micro leading-tight text-muted-foreground">{detail}</p>
                {/if}
                <DeveloperOnly>
                  <code class="mt-0.5 block font-mono text-micro text-muted-foreground" data-testid="session-access-id">
                    {row.id}
                  </code>
                </DeveloperOnly>
              </div>
              <Toggle
                size="sm"
                checked={isSwitchedOn(row)}
                {disabled}
                onchange={(on) => onswitch(row, on)}
                aria-label={$t("chat.integrations.switch_label", { values: { name: nameOf(row) } })}
                data-testid="session-access-switch"
              />
            </li>
          {/each}
        </ul>
      </div>
    {/each}
  {/if}
</section>
