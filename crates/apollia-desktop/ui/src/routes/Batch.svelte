<script lang="ts">
  /**
   * The view of a batch: the one a launch or a row of the history opened. With
   * no batch opened, the history is shown instead.
   */
  import { openedBatch } from "$lib/stores/batchView";
  import BatchHistoryScreen from "../components/batch/view/BatchHistoryScreen.svelte";
  import BatchScreen from "../components/batch/view/BatchScreen.svelte";
</script>

{#if $openedBatch === null}
  <BatchHistoryScreen />
{:else}
  <BatchScreen batchId={$openedBatch.batchId} origin={$openedBatch.origin} />
{/if}
