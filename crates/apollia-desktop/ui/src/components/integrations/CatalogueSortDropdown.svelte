<script module lang="ts">
  /** Sort options available for the MCP catalogue list. */
  export type SortOption = "trust" | "name_asc" | "name_desc" | "installed_first";
</script>

<script lang="ts">
  import { t } from "svelte-i18n";
  import { Select } from "$lib/components/ui/select";

  interface Props {
    value: SortOption;
    onchange: (value: SortOption) => void;
  }

  let { value, onchange }: Props = $props();
</script>

<div class="flex items-center gap-2" data-testid="catalogue-sort-dropdown">
  <span class="text-sm text-muted-foreground whitespace-nowrap">
    {$t("integrations.catalogue.sort_label")}
  </span>
  <Select
    id="catalogue-sort"
    class="h-8 w-auto text-sm"
    {value}
    onchange={(e) => onchange((e.target as HTMLSelectElement).value as SortOption)}
    aria-label={$t("integrations.catalogue.sort_label")}
    data-testid="catalogue-sort-select"
  >
    <option value="trust">{$t("integrations.catalogue.sort_trust")}</option>
    <option value="name_asc">{$t("integrations.catalogue.sort_name_asc")}</option>
    <option value="name_desc">{$t("integrations.catalogue.sort_name_desc")}</option>
    <option value="installed_first">{$t("integrations.catalogue.sort_installed")}</option>
  </Select>
</div>
