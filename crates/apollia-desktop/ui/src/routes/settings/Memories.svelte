<script lang="ts" context="module">
  export const meta = {
    title: "settings.nav.memories",
    icon: "brain",
    group: "settings.nav.cluster_system",
    cluster: "system",
  } as const;
</script>

<script lang="ts">
  import { t } from "svelte-i18n";
  import { uiMode } from "$lib/stores/mode";
  import UserMemories from "./UserMemories.svelte";
</script>

<section class="space-y-4" data-testid="memories-section-system">
  <header>
    <h1 class="text-xl font-semibold">{$t("settings.memories")}</h1>
  </header>
  <UserMemories mode={$uiMode} />
</section>
