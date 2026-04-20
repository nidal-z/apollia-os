<script lang="ts">
  import { Skeleton } from "$lib/components/ui/skeleton";

  interface Props {
    rows?: number;
  }

  let { rows = 3 }: Props = $props();
</script>

<div class="space-y-4" data-testid="settings-section-skeleton">
  <Skeleton variant="table-row" />
  {#each Array.from({ length: rows }) as _, i (i)}
    <Skeleton variant="card" height="6rem" />
  {/each}
</div>
