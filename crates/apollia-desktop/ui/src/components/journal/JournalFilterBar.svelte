<script lang="ts">
  /**
   * JournalFilterBar - the five filters of the journal, side by side: agent,
   * family, nature, result, period. Each change is handed to the screen, which
   * asks the route for the matching entries.
   */
  import { t } from "svelte-i18n";
  import { Select } from "$lib/components/ui/select";
  import type { JournalNature, JournalOutcome } from "$lib/ipc/journal";
  import {
    FAMILY_OPTIONS,
    NATURE_LABEL,
    PERIOD_LABEL,
    PERIOD_OPTIONS,
    RESULT_LABEL,
    RESULT_OPTIONS,
    familyLabelKey,
    natureOptions,
    type JournalFilters,
    type JournalPeriod,
  } from "./journal";

  interface Props {
    filters: JournalFilters;
    /** The agents the list can be narrowed to. */
    agents: Array<{ id: string; name: string }>;
    developerMode: boolean;
    onChange: (filters: JournalFilters) => void;
  }

  let { filters, agents, developerMode, onChange }: Props = $props();

  function picked(event: Event): string {
    return (event.currentTarget as HTMLSelectElement).value;
  }
</script>

<div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-5" data-testid="journal-filters">
  <label class="flex min-w-0 flex-col gap-1 text-caption text-muted-foreground">
    {$t("journal.filter.agent")}
    <Select
      size="sm"
      value={filters.agent}
      onchange={(event) => onChange({ ...filters, agent: picked(event) })}
      data-testid="journal-filter-agent"
    >
      <option value="">{$t("journal.filter.all_agents")}</option>
      {#each agents as agent (agent.id)}
        <option value={agent.id}>{agent.name}</option>
      {/each}
    </Select>
  </label>

  <label class="flex min-w-0 flex-col gap-1 text-caption text-muted-foreground">
    {$t("journal.filter.family")}
    <Select
      size="sm"
      value={filters.family}
      onchange={(event) => onChange({ ...filters, family: picked(event) })}
      data-testid="journal-filter-family"
    >
      <option value="">{$t("journal.filter.all_families")}</option>
      {#each FAMILY_OPTIONS as family (family)}
        <option value={family}>{$t(familyLabelKey(family) ?? "agent_sheet.family_unclassified")}</option>
      {/each}
    </Select>
  </label>

  <label class="flex min-w-0 flex-col gap-1 text-caption text-muted-foreground">
    {$t("journal.filter.nature")}
    <Select
      size="sm"
      value={filters.nature}
      onchange={(event) => onChange({ ...filters, nature: picked(event) as JournalNature | "" })}
      data-testid="journal-filter-nature"
    >
      <option value="">{$t("journal.filter.all_natures")}</option>
      {#each natureOptions(developerMode) as nature (nature)}
        <option value={nature}>{$t(NATURE_LABEL[nature])}</option>
      {/each}
    </Select>
  </label>

  <label class="flex min-w-0 flex-col gap-1 text-caption text-muted-foreground">
    {$t("journal.filter.result")}
    <Select
      size="sm"
      value={filters.result}
      onchange={(event) => onChange({ ...filters, result: picked(event) as JournalOutcome | "" })}
      data-testid="journal-filter-result"
    >
      <option value="">{$t("journal.filter.all_results")}</option>
      {#each RESULT_OPTIONS as result (result)}
        <option value={result}>{$t(RESULT_LABEL[result])}</option>
      {/each}
    </Select>
  </label>

  <label class="flex min-w-0 flex-col gap-1 text-caption text-muted-foreground">
    {$t("journal.filter.period")}
    <Select
      size="sm"
      value={filters.period}
      onchange={(event) => onChange({ ...filters, period: picked(event) as JournalPeriod })}
      data-testid="journal-filter-period"
    >
      {#each PERIOD_OPTIONS as period (period)}
        <option value={period}>{$t(PERIOD_LABEL[period])}</option>
      {/each}
    </Select>
  </label>
</div>
