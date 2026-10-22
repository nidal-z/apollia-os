<script lang="ts">
  /**
   * ConnectionListHeader - what spans the Connections page above its list:
   * the title and the add action, the search, the kind chips, then one
   * banner per source that could not be read. A source that fails names
   * itself; the others are still shown below.
   */
  import { t } from "svelte-i18n";
  import { Plus, Search } from "lucide-svelte";
  import { ErrorBanner, FilterChipBar } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { KIND_FILTERS, KIND_LABEL_KEYS, kindCounts, type ConnectionRow, type KindFilter } from "$lib/connections/connectionRows";
  import { FAILURE_KEYS, type SourceFailure } from "$lib/connections/pageSources";

  interface Props {
    rows: ConnectionRow[];
    query: string;
    kind: KindFilter;
    failures: SourceFailure[];
    onsearch: (query: string) => void;
    onkind: (kind: KindFilter) => void;
    onadd: () => void;
    onretry: () => void;
  }

  let { rows, query, kind, failures, onsearch, onkind, onadd, onretry }: Props = $props();

  const counts = $derived(kindCounts(rows));
</script>

<div class="flex items-start gap-3">
  <div class="min-w-0 flex-1">
    <h1 class="text-lg font-semibold text-foreground">{$t("nav.connections")}</h1>
    <p class="mt-0.5 text-sm text-muted-foreground">{$t("connections.page_subtitle")}</p>
  </div>
  <Button variant="primary-solid" size="sm" onclick={onadd} data-testid="connections-open-catalogue">
    {#snippet icon()}<Plus size={13} />{/snippet}
    {$t("connections.add_connection")}
  </Button>
</div>

<div class="flex flex-wrap items-center gap-3">
  <div class="w-full max-w-sm">
    <Input
      type="search"
      size="sm"
      icon={Search}
      value={query}
      oninput={(e: Event) => onsearch((e.currentTarget as HTMLInputElement).value)}
      placeholder={$t("connections.list.search_placeholder")}
      aria-label={$t("connections.list.search_placeholder")}
      data-testid="connections-search"
    />
  </div>
  <FilterChipBar
    size="compact"
    chips={KIND_FILTERS.map((key) => ({ key, label: $t(KIND_LABEL_KEYS[key]), count: counts[key] }))}
    activeKey={kind}
    onchange={(key) => onkind(key as KindFilter)}
    aria-label={$t("connections.list.kind_filter_label")}
    testidPrefix="connections-kind-filter"
  />
</div>

{#each failures as failure (failure.source)}
  <ErrorBanner
    tone="warning"
    message={$t(FAILURE_KEYS[failure.source], { values: { error: failure.message } })}
    {onretry}
    data-testid="connections-source-error-{failure.source}"
  />
{/each}
