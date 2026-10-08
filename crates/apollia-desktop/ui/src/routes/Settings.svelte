<script lang="ts">
  import { onMount, onDestroy, tick, type Component } from "svelte";
  import { t } from "svelte-i18n";
  import { Check, Menu, X } from "lucide-svelte";
  import {
    settingsSubRoute,
    goToSettingsSubRoute,
    requestSettingsSubRoute,
    installSettingsNavigationGuard,
    DEFAULT_SETTINGS_SUB_ROUTE,
    type SettingsSubRoute,
    type SettingsFormRoute,
  } from "$lib/stores/settings";
  import {
    settingsDirtyStore,
    hasExplicitDirty,
    holdsExplicitChanges,
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
  import { sidebarState } from "$lib/stores/layout";
  import { Sheet } from "$lib/components/ui/sheet";
  import { ErrorBanner, ListRow, PageHeader } from "$lib/components/operator";
  import { listNavigation } from "$lib/components/operator/listNavigation";
  import { RouteTransition } from "$lib/components/ui/route-transition";
  import SettingSectionSkeleton from "../components/settings/SettingSectionSkeleton.svelte";
  import UnsavedChangesBadge from "../components/settings/UnsavedChangesBadge.svelte";
  import HardwareHeaderActions from "../components/settings/system/HardwareHeaderActions.svelte";
  import UnsavedChangesDialog from "../components/settings/UnsavedChangesDialog.svelte";
  import { Button } from "$lib/components/ui/button";
  import { SETTINGS_NAV, visibleSettingsNav } from "$lib/settings/settingsNav";
  import { developerMode } from "$lib/stores/developerMode";
  import { settingsSubRouteVisible } from "$lib/stores/settings";
  import { installSettingsCloseGuard, closeSettingsWindow } from "$lib/stores/settingsWindow";

  // ─── Lazy-loaded tab content (existing sub-routes own invoke()/stores) ──
  const LOADERS: Record<SettingsSubRoute, () => Promise<{ default: Component }>> = {
    general: () => import("./settings/General.svelte"),
    appearance: () => import("./settings/Appearance.svelte"),
    profile: () => import("./settings/Profile.svelte"),
    chat: () => import("./settings/ChatSettings.svelte"),
    stt: () => import("./settings/Stt.svelte"),
    "model-defaults": () => import("./settings/ModelDefaults.svelte"),
    integrations: () => import("./settings/Integrations.svelte"),
    notifications: () => import("./settings/Notifications.svelte"),
    developer: () => import("./settings/Developer.svelte"),
    configuration: () => import("./settings/Configuration.svelte"),
    engines: () => import("./settings/Engines.svelte"),
    hardware: () => import("./settings/System.svelte"),
  };

  const cache = new Map<SettingsSubRoute, Component>();

  interface Props {
    sub?: string | null;
    /** Close the settings window (the cross of the header). */
    onclose?: () => void;
  }

  let { sub = null, onclose }: Props = $props();

  let activeComponent = $state<Component | null>(null);
  let loadError = $state<string | null>(null);
  let scrollContainer = $state<HTMLDivElement | null>(null);

  // Mobile drawer state for the grouped sub-nav.
  const isMobile = $derived($sidebarState === "drawer");
  let mobileOpen = $state(false);

  // ─── Nav-guard / unsaved-changes dialog ──────────────────────────
  let pendingRoute = $state<SettingsSubRoute | null>(null);
  // Set while the dialog was opened by closing the window: what to run once closed.
  let pendingClose = $state<{ then: (() => void) | null } | null>(null);
  let dialogOpen = $state(false);
  let dialogSaving = $state(false);

  type SaveFn = () => Promise<boolean>;
  const registeredSavers = new Map<SettingsFormRoute, SaveFn>();
  const registeredResetters = new Map<SettingsFormRoute, () => void>();

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

  const uninstallGuard = installSettingsNavigationGuard((from, to) => {
    if (!holdsExplicitChanges(from)) return true;
    pendingRoute = to;
    dialogOpen = true;
    return false;
  });

  const uninstallCloseGuard = installSettingsCloseGuard((then) => {
    if (!holdsExplicitChanges($settingsSubRoute)) return true;
    pendingRoute = null;
    pendingClose = { then };
    dialogOpen = true;
    return false;
  });

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
    uninstallCloseGuard();
    unregisterSaveShortcut();
    if (typeof window !== "undefined") {
      window.removeEventListener("beforeunload", handleBeforeUnload);
    }
  });

  function onNavigate(route: SettingsSubRoute): void {
    requestSettingsSubRoute(route);
    mobileOpen = false;
  }

  // Dialog handlers ───────────────────────────────────────────────
  /** Close the window the dialog was holding open, then run its continuation. */
  function finishPendingClose(): void {
    const close = pendingClose;
    pendingClose = null;
    if (!close) return;
    closeSettingsWindow();
    close.then?.();
  }

  async function onDiscard() {
    const target = pendingRoute;
    pendingRoute = null;
    dialogOpen = false;
    const current = $settingsSubRoute;
    settingsDirtyStore.update((m) => {
      const next = { ...m };
      if (next[current]) next[current] = { ...next[current]!, dirty: false };
      return next;
    });
    registeredResetters.get(current)?.();
    if (target) requestSettingsSubRoute(target);
    finishPendingClose();
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
    finishPendingClose();
  }

  function onStay() {
    pendingRoute = null;
    pendingClose = null;
    dialogOpen = false;
  }

  if (typeof window !== "undefined") {
    window.__apolliaRegisterSettingsForm =
      (route: SettingsFormRoute, handle: { save: SaveFn; reset: () => void }) => {
        registeredSavers.set(route, handle.save);
        registeredResetters.set(route, handle.reset);
        return () => {
          if (registeredSavers.get(route) === handle.save) registeredSavers.delete(route);
          if (registeredResetters.get(route) === handle.reset) registeredResetters.delete(route);
        };
      };
  }

  // ─── Per-tab heading copy ──────────────────────────────────────────
  // Title and kicker are derived from the cluster the route belongs to (single
  // source of truth = SETTINGS_NAV), so nothing is retyped per entry. Only
  // the subtitle needs its own key; reuse an existing one where a sub-page
  // already ships copy, otherwise a dedicated `settings.<route>.subtitle`.
  const SUBTITLE_KEY: Record<SettingsSubRoute, string> = {
    general: "settings.general.subtitle",
    appearance: "settings.appearance.subtitle",
    profile: "settings.profile_subtitle",
    chat: "settings.chat.subtitle",
    stt: "settings.stt_subtitle",
    "model-defaults": "settings.model_defaults.subtitle",
    integrations: "settings.integrations_section.subtitle",
    notifications: "settings.notifications.subtitle",
    developer: "settings.developer.subtitle",
    configuration: "settings.config_editor.subtitle",
    engines: "settings.engines.subtitle",
    hardware: "settings.system.subtitle",
  };

  const ROUTE_META = new Map<SettingsSubRoute, { titleKey: string; kickerKey: string }>();
  for (const cluster of SETTINGS_NAV) {
    for (const entry of cluster.entries) {
      ROUTE_META.set(entry.key, { titleKey: entry.labelKey, kickerKey: cluster.labelKey });
    }
  }

  const head = $derived.by(() => {
    const meta = ROUTE_META.get($settingsSubRoute);
    return {
      kicker: meta ? $t(meta.kickerKey) : "",
      title: meta ? $t(meta.titleKey) : "",
      subtitle: $t(SUBTITLE_KEY[$settingsSubRoute]),
    };
  });
  // The column with the developer mode in its position; a section of the mode
  // left open when the mode goes off gives way to Developer, where the switch is.
  const shownNav = $derived(visibleSettingsNav($developerMode));
  $effect(() => {
    if (!settingsSubRouteVisible($settingsSubRoute, $developerMode)) {
      goToSettingsSubRoute("developer");
    }
  });

  const currentDirty = $derived($settingsDirtyStore[$settingsSubRoute]?.dirty === true);

  // ─── Sub-nav rendering helpers ─────────────────────────────────────
  const FOCUS_RING =
    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background";

  function entryClass(active: boolean): string {
    const base = active
      ? "border border-border bg-card text-foreground font-semibold shadow-sm"
      : "border border-transparent text-muted-foreground hover:bg-muted/60 hover:text-foreground";
    return `${base} ${FOCUS_RING}`;
  }

  function entryIconClass(active: boolean): string {
    return active ? "text-primary" : "text-muted-foreground";
  }

</script>

<!--
  Operator Settings - two-column shell (the outer app rail lives outside this
  route). Left: grouped sub-nav (cluster titles + ListRow nav rows, roving
  keyboard focus). Right: shell PageHeader with the unsaved affordance, then the
  lazy-loaded sub-route wrapped in the canonical route transition.
-->
{#snippet clusterNav()}
  {#each shownNav as cluster (cluster.labelKey)}
    <div>
      <div class="mb-1.5 px-2.5 font-mono text-overline uppercase text-muted-foreground">
        {$t(cluster.labelKey)}
      </div>
      <ul class="space-y-0.5">
        {#each cluster.entries as entry (entry.key)}
          {@const SvelteIcon = entry.icon}
          {@const isActive = $settingsSubRoute === entry.key}
          {@const isDirty = $settingsDirtyStore[entry.key]?.dirty === true}
          <li aria-current={isActive ? "page" : undefined}>
            <ListRow
              variant="nav"
              align="center"
              onclick={() => onNavigate(entry.key)}
              data-testid="settings-nav-{entry.key}"
              class={entryClass(isActive)}
            >
              <SvelteIcon
                size={14}
                strokeWidth={1.75}
                aria-hidden="true"
                class={entryIconClass(isActive)}
              />
              <span class="min-w-0 flex-1 truncate text-body-sm" title={$t(entry.labelKey)}>
                {$t(entry.labelKey)}
              </span>
              {#if isDirty}
                <span
                  class="h-1.5 w-1.5 flex-none rounded-full bg-warning"
                  title={$t("settings.unsaved_changes")}
                ></span>
              {/if}
            </ListRow>
          </li>
        {/each}
      </ul>
    </div>
  {/each}
{/snippet}

<div
  class="flex h-full min-h-0 flex-1 flex-col"
  data-testid="settings-page"
>
  <div class="flex min-h-0 flex-1">
    {#if isMobile}
      <!-- Mobile: trigger + side sheet hosting the same nav. -->
      <div
        class="sticky top-0 z-sticky mb-2 flex w-full items-center gap-2 border-b border-border bg-background/80 px-4 py-2 backdrop-blur-md"
      >
        <Button
          variant="outline"
          size="sm"
          onclick={() => (mobileOpen = true)}
          aria-haspopup="dialog"
          aria-expanded={mobileOpen}
          data-testid="settings-mobile-nav-toggle"
        >
          {#snippet icon()}
            <Menu size={16} strokeWidth={1.75} />
          {/snippet}
          {$t("settings.nav.mobile_open")}
        </Button>
      </div>

      <Sheet open={mobileOpen} onclose={() => (mobileOpen = false)} side="left" width="sm">
        <nav
          use:listNavigation
          class="flex h-full flex-col gap-5 overflow-y-auto p-4"
          aria-label={$t("settings.nav.aria_label")}
          data-testid="settings-mobile-nav-sheet"
        >
          <h2 class="text-heading-md text-foreground">
            {$t("settings.nav.mobile_title")}
          </h2>
          {@render clusterNav()}
        </nav>
      </Sheet>
    {:else}
      <!-- Desktop: 240px left sub-nav. -->
      <nav
        use:listNavigation
        class="flex w-60 flex-shrink-0 flex-col gap-5 overflow-y-auto border-r border-border bg-muted/30 px-3 py-5"
        aria-label={$t("settings.nav.aria_label")}
        data-testid="settings-nav"
      >
        {@render clusterNav()}
      </nav>
    {/if}

    <!-- Right content column. -->
    <div class="flex min-w-0 flex-1 flex-col overflow-hidden" data-testid="settings-content">
      <PageHeader kicker={head.kicker} title={head.title} subtitle={head.subtitle}>
        {#snippet actions()}
          {#if $settingsSubRoute === "hardware"}
            <HardwareHeaderActions />
          {/if}
          <UnsavedChangesBadge />
          {#if !currentDirty}
            <span
              class="inline-flex items-center gap-1.5 text-caption text-success-a11y"
              data-testid="settings-saved-hint"
            >
              <Check size={12} strokeWidth={2} />
              {$t("settings.all_saved")}
            </span>
          {/if}
          {#if onclose}
            <button
              type="button"
              class="rounded-md p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              onclick={onclose}
              aria-label={$t("settings.window.close")}
              title={$t("settings.window.close")}
              data-testid="settings-window-close"
            >
              <X size={16} strokeWidth={1.75} aria-hidden="true" />
            </button>
          {/if}
        {/snippet}
      </PageHeader>

      {#if loadError}
        <ErrorBanner message={loadError} class="mx-8 mt-4" />
      {/if}

      <!-- Scrollable tab body - sub-routes own their cards/forms. -->
      <div
        bind:this={scrollContainer}
        data-settings-scroll
        class="flex-1 overflow-y-auto px-8 pb-8 pt-4"
      >
        {#key $settingsSubRoute}
          <RouteTransition>
            {#if activeComponent}
              {@const ActiveComponent = activeComponent}
              <ActiveComponent />
            {:else}
              <SettingSectionSkeleton />
            {/if}
          </RouteTransition>
        {/key}
      </div>
    </div>
  </div>

  <UnsavedChangesDialog
    open={dialogOpen}
    saving={dialogSaving}
    onDiscard={onDiscard}
    onSave={onSaveAndContinue}
    onStay={onStay}
  />
</div>
