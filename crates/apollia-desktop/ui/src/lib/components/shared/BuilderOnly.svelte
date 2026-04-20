<!--
  Renders its children only when the UI is in builder mode.
  Thin wrapper over `$uiMode === "builder"` to keep callsites
  readable and give a single seam to test against.
-->
<script lang="ts">
  import type { Snippet } from "svelte";
  import { uiMode } from "$lib/stores/mode";

  interface Props {
    children?: Snippet;
  }

  let { children }: Props = $props();
</script>

{#if $uiMode === "builder"}
  {@render children?.()}
{/if}
