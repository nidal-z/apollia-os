<script lang="ts" context="module">
  export const meta = {
    title: "settings.nav.profile",
    icon: "user",
    group: "settings.nav.cluster_personalization",
    cluster: "personalization",
  } as const;
</script>

<script lang="ts">
  import { uiMode } from "$lib/stores/mode";
  import UserMemories from "./UserMemories.svelte";
</script>

<section data-testid="memories-section">
  <UserMemories mode={$uiMode} />
</section>
