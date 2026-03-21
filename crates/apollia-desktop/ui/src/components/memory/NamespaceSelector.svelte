<script lang="ts">
  import { t } from "svelte-i18n";
  import { Select } from "$lib/components/ui/select";

  interface Props {
    namespaces: string[];
    selected: string;
    onselect: (ns: string) => void;
  }

  let { namespaces, selected, onselect }: Props = $props();
</script>

<div class="flex items-center gap-2">
  <label for="ns-select" class="text-sm font-medium text-muted-foreground">{$t('memory.namespace_label')}</label>
  <Select
    id="ns-select"
    class="h-8 w-auto"
    value={selected}
    onchange={(e) => onselect((e.target as HTMLSelectElement).value)}
  >
    {#each namespaces as ns}
      <option value={ns}>{ns}</option>
    {/each}
  </Select>
</div>
