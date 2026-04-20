<!--
  Renders its children only when the UI is in operator mode.
-->
<script lang="ts">
  import type { Snippet } from "svelte";
  import { uiMode } from "$lib/stores/mode";

  interface Props {
    children?: Snippet;
  }

  let { children }: Props = $props();
</script>

{#if $uiMode === "operator"}
  {@render children?.()}
{/if}
