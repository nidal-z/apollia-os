<script lang="ts">
  import SharedTrustBadge from "$lib/components/shared/TrustBadge.svelte";
  import type { TrustLevel } from "$lib/types";

  interface Props {
    level: TrustLevel;
  }

  let { level }: Props = $props();
</script>

<SharedTrustBadge {level} />
