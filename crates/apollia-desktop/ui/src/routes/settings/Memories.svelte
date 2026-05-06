<script lang="ts" context="module">
  export const meta = {
    title: "settings.nav.memories",
    icon: "brain",
    group: "settings.nav.cluster_system",
    cluster: "system",
  } as const;
</script>

<script lang="ts">
  import { uiMode } from "$lib/stores/mode";
  import UserMemoryDashboard from "../../components/memory/UserMemoryDashboard.svelte";
</script>

<section data-testid="memories-section-system">
  <UserMemoryDashboard mode={$uiMode} />
</section>
