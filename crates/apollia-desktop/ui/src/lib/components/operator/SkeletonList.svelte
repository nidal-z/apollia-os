<script lang="ts">
  /**
   * SkeletonList - canonical loading placeholder for a list or sidebar.
   *
   * Renders `count` skeleton rows so list-loading states look the same across
   * screens instead of each one hand-rolling its own skeleton layout.
   */
  import { Skeleton } from "$lib/components/ui/skeleton";

  interface Props {
    /** Number of skeleton rows. */
    count?: number;
    /** Show a leading avatar/icon block per row. */
    avatar?: boolean;
    /** Extra classes on each row. */
    rowClass?: string;
    class?: string;
  }

  let { count = 5, avatar = true, rowClass = "", class: className = "" }: Props = $props();
</script>

<div class="flex flex-col gap-2 {className}" aria-hidden="true">
  {#each Array(count) as _, i (i)}
    <div class="flex items-center gap-3 {rowClass}">
      {#if avatar}
        <Skeleton class="h-8 w-8 shrink-0 rounded-md" />
      {/if}
      <div class="min-w-0 flex-1 space-y-1.5">
        <Skeleton class="h-3 w-1/2 rounded" />
        <Skeleton class="h-2.5 w-1/3 rounded" />
      </div>
    </div>
  {/each}
</div>
