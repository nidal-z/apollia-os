<script lang="ts">
  import { onMount, type Component } from "svelte";
  import { t } from "svelte-i18n";
  import SettingsLayout from "../components/settings/SettingsLayout.svelte";
  import SettingSectionSkeleton from "../components/settings/SettingSectionSkeleton.svelte";
  import {
    settingsSubRoute,
    goToSettingsSubRoute,
    DEFAULT_SETTINGS_SUB_ROUTE,
    type SettingsSubRoute,
  } from "$lib/stores/settings";

  /**
   * Lazy loaders for each sub-route. Each entry is a dynamic import whose
   * resulting chunk is emitted as a separate bundle (settings-<name>.js) by
   * Vite's default code-splitting. Sub-routes are only loaded on demand.
   */
  const LOADERS: Record<SettingsSubRoute, () => Promise<{ default: Component }>> = {
    appearance: () => import("./settings/Appearance.svelte"),
    profile: () => import("./settings/Profile.svelte"),
    stt: () => import("./settings/Stt.svelte"),
    llm: () => import("./settings/Llm.svelte"),
    configuration: () => import("./settings/Configuration.svelte"),
    system: () => import("./settings/System.svelte"),
    memories: () => import("./settings/Memories.svelte"),
    shortcuts: () => import("./settings/Shortcuts.svelte"),
    danger: () => import("./settings/Danger.svelte"),
  };

  /** Module cache — components loaded once are kept in memory. */
  const cache = new Map<SettingsSubRoute, Component>();

  interface Props {
    /**
     * Optional initial sub-route override. Used for deep-linking tests
     * (`<Settings sub="stt" />`). If omitted, `/settings` redirects to
     * the default sub-route (`appearance`).
     */
    sub?: string | null;
  }

  let { sub = null }: Props = $props();

  let activeComponent = $state<Component | null>(null);
  let loadError = $state<string | null>(null);

  async function loadRoute(route: SettingsSubRoute): Promise<void> {
    const cached = cache.get(route);
    if (cached) {
      activeComponent = cached;
      return;
    }
    activeComponent = null;
    loadError = null;
    try {
      const mod = await LOADERS[route]();
      cache.set(route, mod.default);
      if ($settingsSubRoute === route) {
        activeComponent = mod.default;
      }
    } catch (err) {
      loadError = err instanceof Error ? err.message : String(err);
    }
  }

  // React to sub-route changes: load the corresponding chunk lazily.
  $effect(() => {
    const route = $settingsSubRoute;
    void loadRoute(route);
  });

  onMount(() => {
    // Redirect `/settings` → `/settings/appearance` on first mount, or honor
    // the explicit `sub` prop (deep-link).
    goToSettingsSubRoute(sub ?? $settingsSubRoute ?? DEFAULT_SETTINGS_SUB_ROUTE);
  });

  function onNavigate(route: SettingsSubRoute): void {
    goToSettingsSubRoute(route);
  }
</script>

<div class="mx-auto w-full max-w-6xl space-y-6" data-testid="settings-page">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-semibold">{$t('settings.title')}</h1>
      <p class="text-xs text-muted-foreground" data-testid="settings-subtitle">{$t('settings.subtitle')}</p>
    </div>
  </div>

  {#if loadError}
    <div class="rounded-md border border-destructive/30 bg-destructive/5 px-4 py-2 text-sm text-destructive">
      {loadError}
    </div>
  {/if}

  <SettingsLayout active={$settingsSubRoute} onnavigate={onNavigate}>
    {#key $settingsSubRoute}
      {#if activeComponent}
        {@const ActiveComponent = activeComponent}
        <ActiveComponent />
      {:else}
        <SettingSectionSkeleton />
      {/if}
    {/key}
  </SettingsLayout>
</div>
