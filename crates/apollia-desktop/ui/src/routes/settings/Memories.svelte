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
  import RecentExtractions from "../../components/memory/RecentExtractions.svelte";
  import UserMemoryDashboard from "../../components/memory/UserMemoryDashboard.svelte";
</script>

<section class="space-y-6" data-testid="memories-section-system">
  <RecentExtractions mode={$uiMode} />
  <UserMemoryDashboard mode={$uiMode} />
</section>
