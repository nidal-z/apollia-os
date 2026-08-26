<script lang="ts">
  import { onMount, onDestroy, tick, type Component } from "svelte";
  import { t } from "svelte-i18n";
  import {
    Palette,
    User,
    Mic,
    Cpu,
    Sliders,
    Info,
    Keyboard,
    AlertTriangle,
    Activity,
    Download,
    Wrench,
    ShieldCheck,
    Check,
    Menu,
    PlugZap,
    LifeBuoy,
    BadgeInfo,
  } from "lucide-svelte";
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
  import { sidebarState } from "$lib/stores/layout";
  import { Sheet } from "$lib/components/ui/sheet";
  import { ErrorBanner, ListRow, PageHeader } from "$lib/components/operator";
  import { listNavigation } from "$lib/components/operator/listNavigation";
  import { RouteTransition } from "$lib/components/ui/route-transition";
  import SettingSectionSkeleton from "../components/settings/SettingSectionSkeleton.svelte";
  import UnsavedChangesBadge from "../components/settings/UnsavedChangesBadge.svelte";
  import UnsavedChangesDialog from "../components/settings/UnsavedChangesDialog.svelte";
  import { Button } from "$lib/components/ui/button";

  // ─── Lazy-loaded tab content (existing sub-routes own invoke()/stores) ──
  const LOADERS: Record<SettingsSubRoute, () => Promise<{ default: Component }>> = {
    appearance: () => import("./settings/Appearance.svelte"),
    profile: () => import("./settings/Profile.svelte"),
    stt: () => import("./settings/Stt.svelte"),
    llm: () => import("./settings/Llm.svelte"),
    "model-hub": () => import("./settings/ModelHub.svelte"),
    configuration: () => import("./settings/Configuration.svelte"),
    tools: () => import("./settings/Tools.svelte"),
    permissions: () => import("./settings/Permissions.svelte"),
    integrations: () => import("./settings/Integrations.svelte"),
    system: () => import("./settings/System.svelte"),
    shortcuts: () => import("./settings/Shortcuts.svelte"),
    observability: () => import("./settings/Observability.svelte"),
    help: () => import("./settings/Help.svelte"),
    about: () => import("./settings/About.svelte"),
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

  // Mobile drawer state for the grouped sub-nav.
  const isMobile = $derived($sidebarState === "drawer");
  let mobileOpen = $state(false);

  // ─── Nav-guard / unsaved-changes dialog ──────────────────────────
  let pendingRoute = $state<SettingsSubRoute | null>(null);
  let dialogOpen = $state(false);
  let dialogSaving = $state(false);

  type SaveFn = () => Promise<boolean>;
  const registeredSavers = new Map<SettingsSubRoute, SaveFn>();
  const registeredResetters = new Map<SettingsSubRoute, () => void>();

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
    const state = getRouteState(from);
    if (!state.dirty || state.autoSave !== "explicit") return true;
    pendingRoute = to;
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

  if (typeof window !== "undefined") {
    window.__apolliaRegisterSettingsForm =
      (route: SettingsSubRoute, handle: { save: SaveFn; reset: () => void }) => {
        registeredSavers.set(route, handle.save);
        registeredResetters.set(route, handle.reset);
        return () => {
          if (registeredSavers.get(route) === handle.save) registeredSavers.delete(route);
          if (registeredResetters.get(route) === handle.reset) registeredResetters.delete(route);
        };
      };
  }

  // ─── Grouped sub-nav definition (V3 design) ───────────────────────
  type NavEntry = {
    key: SettingsSubRoute;
    labelKey: string;
    icon: typeof Palette;
  };
  type NavCluster = {
    labelKey: string;
    entries: NavEntry[];
    danger?: boolean;
  };

  const CLUSTERS: NavCluster[] = [
    {
      labelKey: "settings.nav.cluster_personalization",
      entries: [
        { key: "appearance", labelKey: "settings.nav.appearance", icon: Palette },
        { key: "profile", labelKey: "settings.nav.profile", icon: User },
      ],
    },
    {
      labelKey: "settings.nav.cluster_ai",
      entries: [
        { key: "stt", labelKey: "settings.nav.stt", icon: Mic },
        { key: "llm", labelKey: "settings.nav.llm", icon: Cpu },
        { key: "model-hub", labelKey: "settings.nav.model_hub", icon: Download },
      ],
    },
    {
      labelKey: "settings.nav.cluster_system",
      entries: [
        { key: "configuration", labelKey: "settings.nav.configuration", icon: Sliders },
        { key: "tools", labelKey: "settings.nav.tools", icon: Wrench },
        { key: "permissions", labelKey: "settings.nav.permissions", icon: ShieldCheck },
        { key: "integrations", labelKey: "settings.nav.integrations", icon: PlugZap },
        { key: "system", labelKey: "settings.nav.system", icon: Info },
        { key: "shortcuts", labelKey: "settings.nav.shortcuts", icon: Keyboard },
        { key: "observability", labelKey: "settings.nav.observability", icon: Activity },
      ],
    },
    {
      labelKey: "settings.nav.cluster_help",
      entries: [
        { key: "help", labelKey: "settings.nav.help", icon: LifeBuoy },
        { key: "about", labelKey: "settings.nav.about", icon: BadgeInfo },
      ],
    },
    {
      labelKey: "settings.nav.cluster_danger",
      danger: true,
      entries: [
        { key: "danger", labelKey: "settings.nav.danger", icon: AlertTriangle },
      ],
    },
  ];

  // ─── Per-tab heading copy ──────────────────────────────────────────
  // Title and kicker are derived from the cluster the route belongs to (single
  // source of truth = CLUSTERS above), so nothing is retyped per entry. Only
  // the subtitle needs its own key; reuse an existing one where a sub-page
  // already ships copy, otherwise a dedicated `settings.<route>.subtitle`.
  const SUBTITLE_KEY: Record<SettingsSubRoute, string> = {
    appearance: "settings.appearance.subtitle",
    profile: "settings.profile_subtitle",
    stt: "settings.stt_subtitle",
    llm: "settings.llm.subtitle",
    "model-hub": "settings.model_hub.subtitle",
    configuration: "settings.configuration.subtitle",
    tools: "settings.tools_page.subtitle",
    permissions: "settings.permissions.subtitle",
    integrations: "settings.integrations.subtitle",
    system: "settings.system.subtitle",
    shortcuts: "settings.shortcuts.subtitle",
    observability: "settings.observability.subtitle",
    help: "settings.help.subtitle",
    about: "settings.about.subtitle",
    danger: "settings.danger.subtitle",
  };

  const ROUTE_META = new Map<SettingsSubRoute, { titleKey: string; kickerKey: string }>();
  for (const cluster of CLUSTERS) {
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
  const currentDirty = $derived($settingsDirtyStore[$settingsSubRoute]?.dirty === true);

  // ─── Sub-nav rendering helpers ─────────────────────────────────────
  const FOCUS_RING =
    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background";

  function clusterTextClass(danger: boolean | undefined): string {
    return danger ? "text-destructive" : "text-muted-foreground";
  }

  function entryClass(active: boolean, danger: boolean): string {
    const base = danger
      ? active
        ? "border border-destructive/20 bg-destructive/10 text-destructive font-semibold shadow-sm"
        : "border border-transparent text-destructive/80 hover:bg-destructive/5 hover:text-destructive"
      : active
        ? "border border-border bg-card text-foreground font-semibold shadow-sm"
        : "border border-transparent text-muted-foreground hover:bg-muted/60 hover:text-foreground";
    return `${base} ${FOCUS_RING}`;
  }

  function entryIconClass(active: boolean, danger: boolean): string {
    if (danger) return "text-destructive";
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
  {#each CLUSTERS as cluster (cluster.labelKey)}
    <div>
      <div
        class="mb-1.5 px-2.5 font-mono text-overline uppercase {clusterTextClass(cluster.danger)}"
      >
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
              class={entryClass(isActive, cluster.danger === true)}
            >
              <SvelteIcon
                size={14}
                strokeWidth={1.75}
                aria-hidden="true"
                class={entryIconClass(isActive, cluster.danger === true)}
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
