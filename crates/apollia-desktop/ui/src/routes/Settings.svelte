<script lang="ts">
  import { onMount, onDestroy, tick, type Component } from "svelte";
  import { t } from "svelte-i18n";
  import SettingsLayout from "../components/settings/SettingsLayout.svelte";
  import SettingSectionSkeleton from "../components/settings/SettingSectionSkeleton.svelte";
  import UnsavedChangesBadge from "../components/settings/UnsavedChangesBadge.svelte";
  import UnsavedChangesDialog from "../components/settings/UnsavedChangesDialog.svelte";
  import {
    settingsSubRoute,
    goToSettingsSubRoute,
    requestSettingsSubRoute,
    installSettingsNavigationGuard,
    DEFAULT_SETTINGS_SUB_ROUTE,
    type SettingsSubRoute,
  } from "$lib/stores/settings";
  import {
    settingsDirtyStore,
    hasExplicitDirty,
    getRouteState,
    setScroll,
    getScroll,
  } from "$lib/stores/settingsDirty";
  import {
    registerScopedShortcut,
    isSaveCombo,
    inSettingsScope,
  } from "$lib/keyboard/scopedShortcuts";
  import { addToast } from "$lib/components/ui/toast";

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

  const cache = new Map<SettingsSubRoute, Component>();

  interface Props {
    sub?: string | null;
  }

  let { sub = null }: Props = $props();

  let activeComponent = $state<Component | null>(null);
  let loadError = $state<string | null>(null);
  let scrollContainer = $state<HTMLDivElement | null>(null);

  // ─── Nav-guard / unsaved-changes dialog ──────────────────────────
  let pendingRoute = $state<SettingsSubRoute | null>(null);
  let dialogOpen = $state(false);
  let dialogSaving = $state(false);

  // Exposed via $lib/settings/useSettingsForm — global form handles registered
  // by sub-routes (keyed by route) so the header Cmd+S / dialog Save can
  // trigger the active form. A route with explicit autoSave SHOULD register.
  type SaveFn = () => Promise<boolean>;
  const registeredSavers = new Map<SettingsSubRoute, SaveFn>();

  async function loadRoute(route: SettingsSubRoute): Promise<void> {
    const cached = cache.get(route);
    if (cached) {
      activeComponent = cached;
      await tick();
      restoreScroll(route);
      return;
    }
    activeComponent = null;
    loadError = null;
    try {
      const mod = await LOADERS[route]();
      cache.set(route, mod.default);
      if ($settingsSubRoute === route) {
        activeComponent = mod.default;
        await tick();
        requestAnimationFrame(() => restoreScroll(route));
      }
    } catch (err) {
      loadError = err instanceof Error ? err.message : String(err);
    }
  }

  function restoreScroll(route: SettingsSubRoute): void {
    const y = getScroll(route);
    if (scrollContainer) scrollContainer.scrollTop = y;
  }

  function rememberScroll(route: SettingsSubRoute): void {
    if (scrollContainer) setScroll(route, scrollContainer.scrollTop);
  }

  // React to sub-route changes: load the corresponding chunk lazily.
  let previousRoute: SettingsSubRoute | null = null;
  $effect(() => {
    const route = $settingsSubRoute;
    if (previousRoute && previousRoute !== route) rememberScroll(previousRoute);
    previousRoute = route;
    void loadRoute(route);
  });

  onMount(() => {
    goToSettingsSubRoute(sub ?? $settingsSubRoute ?? DEFAULT_SETTINGS_SUB_ROUTE);
  });

  // Install nav guard: block transitions while the current route is dirty in
  // explicit mode. Opens the 3-option dialog.
  const uninstallGuard = installSettingsNavigationGuard((from, to) => {
    const state = getRouteState(from);
    if (!state.dirty || state.autoSave !== "explicit") return true;
    pendingRoute = to;
    dialogOpen = true;
    return false;
  });

  // Scoped Cmd+S / Ctrl+S — saves the active sub-route if dirty.
  const unregisterSaveShortcut = registerScopedShortcut({
    combo: "cmd+s",
    match: isSaveCombo,
    scope: inSettingsScope,
    handler: () => {
      void triggerSave($settingsSubRoute);
    },
  });

  async function triggerSave(route: SettingsSubRoute): Promise<boolean> {
    const state = getRouteState(route);
    if (!state.dirty || state.autoSave !== "explicit") return true;
    const saver = registeredSavers.get(route);
    if (!saver) return false;
    if (state.savingState === "saving") return false;
    const ok = await saver();
    if (ok) {
      addToast($t("settings.save_toast_success"), "success", {
        "data-testid": "settings-save-toast-success",
      });
    } else {
      const err = getRouteState(route).lastError ?? "";
      addToast($t("settings.save_toast_error", { values: { error: err } }), "error", {
        "data-testid": "settings-save-toast-error",
      });
    }
    return ok;
  }

  // beforeunload: block window close if any explicit dirty form exists.
  function handleBeforeUnload(e: BeforeUnloadEvent): void {
    if ($hasExplicitDirty) {
      e.preventDefault();
      e.returnValue = "";
    }
  }

  if (typeof window !== "undefined") {
    window.addEventListener("beforeunload", handleBeforeUnload);
  }

  onDestroy(() => {
    uninstallGuard();
    unregisterSaveShortcut();
    if (typeof window !== "undefined") {
      window.removeEventListener("beforeunload", handleBeforeUnload);
    }
  });

  function onNavigate(route: SettingsSubRoute): void {
    requestSettingsSubRoute(route);
  }

  // Dialog handlers ───────────────────────────────────────────────
  async function onDiscard() {
    const target = pendingRoute;
    pendingRoute = null;
    dialogOpen = false;
    // Reset the dirty route before switching, otherwise the guard will re-fire.
    const current = $settingsSubRoute;
    settingsDirtyStore.update((m) => {
      const next = { ...m };
      if (next[current]) next[current] = { ...next[current]!, dirty: false };
      return next;
    });
    // Ask the active form to reset its local state.
    registeredResetters.get(current)?.();
    if (target) requestSettingsSubRoute(target);
  }

  async function onSaveAndContinue() {
    dialogSaving = true;
    const current = $settingsSubRoute;
    const target = pendingRoute;
    const ok = await triggerSave(current);
    dialogSaving = false;
    if (!ok) return;
    dialogOpen = false;
    pendingRoute = null;
    if (target) requestSettingsSubRoute(target);
  }

  function onStay() {
    pendingRoute = null;
    dialogOpen = false;
  }

  // Registry exposed to sub-routes via window-level helpers. Kept minimal:
  // sub-routes call `registerSettingsForm(route, { save, reset })` once.
  const registeredResetters = new Map<SettingsSubRoute, () => void>();

  if (typeof window !== "undefined") {
    (window as unknown as { __apolliaRegisterSettingsForm?: unknown }).__apolliaRegisterSettingsForm =
      (route: SettingsSubRoute, handle: { save: SaveFn; reset: () => void }) => {
        registeredSavers.set(route, handle.save);
        registeredResetters.set(route, handle.reset);
        return () => {
          if (registeredSavers.get(route) === handle.save) registeredSavers.delete(route);
          if (registeredResetters.get(route) === handle.reset) registeredResetters.delete(route);
        };
      };
  }
</script>

<div class="mx-auto w-full max-w-6xl space-y-6" data-testid="settings-page">
  <div class="flex items-center justify-between">
    <div class="flex items-center gap-3">
      <h1 class="text-2xl font-semibold">{$t('settings.title')}</h1>
      <UnsavedChangesBadge />
    </div>
    <p class="text-xs text-muted-foreground" data-testid="settings-subtitle">{$t('settings.subtitle')}</p>
  </div>

  {#if loadError}
    <div class="rounded-md border border-destructive/30 bg-destructive/5 px-4 py-2 text-sm text-destructive">
      {loadError}
    </div>
  {/if}

  <SettingsLayout active={$settingsSubRoute} onnavigate={onNavigate}>
    <div
      bind:this={scrollContainer}
      data-settings-scroll
      class="max-h-[calc(100vh-10rem)] overflow-y-auto pr-2"
    >
      {#key $settingsSubRoute}
        {#if activeComponent}
          {@const ActiveComponent = activeComponent}
          <ActiveComponent />
        {:else}
          <SettingSectionSkeleton />
        {/if}
      {/key}
    </div>
  </SettingsLayout>

  <UnsavedChangesDialog
    open={dialogOpen}
    saving={dialogSaving}
    onDiscard={onDiscard}
    onSave={onSaveAndContinue}
    onStay={onStay}
  />
</div>
