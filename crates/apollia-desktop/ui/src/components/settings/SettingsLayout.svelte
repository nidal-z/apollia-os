<script lang="ts">
  import type { Snippet } from "svelte";
  import type { SettingsSubRoute } from "$lib/stores/settings";
  import SettingsNav from "./SettingsNav.svelte";

  interface Props {
    active: SettingsSubRoute;
    onnavigate: (route: SettingsSubRoute) => void;
    children: Snippet;
  }

  let { active, onnavigate, children }: Props = $props();
</script>

<div class="flex gap-6" data-testid="settings-layout">
  <SettingsNav {active} {onnavigate} />
  <div class="min-w-0 flex-1" data-testid="settings-content">
    {@render children()}
  </div>
</div>
