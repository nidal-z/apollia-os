<script lang="ts">
  interface Props {
    namespaces: string[];
    selected: string;
    onselect: (ns: string) => void;
  }

  let { namespaces, selected, onselect }: Props = $props();
</script>

<div class="flex items-center gap-2">
  <label for="ns-select" class="text-sm font-medium text-muted-foreground">Namespace</label>
  <select
    id="ns-select"
    class="rounded-md border bg-background px-3 py-1.5 text-sm shadow-sm focus:outline-none focus:ring-1 focus:ring-ring"
    value={selected}
    onchange={(e) => onselect((e.target as HTMLSelectElement).value)}
  >
    {#each namespaces as ns}
      <option value={ns}>{ns}</option>
    {/each}
  </select>
</div>
