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
  import SettingSectionSkeleton from "../components/settings/SettingSectionSkeleton.svelte";
  import UnsavedChangesBadge from "../components/settings/UnsavedChangesBadge.svelte";
  import UnsavedChangesDialog from "../components/settings/UnsavedChangesDialog.svelte";

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
    system: () => import("./settings/System.svelte"),
    shortcuts: () => import("./settings/Shortcuts.svelte"),
    observability: () => import("./settings/Observability.svelte"),
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
        { key: "system", labelKey: "settings.nav.system", icon: Info },
        { key: "shortcuts", labelKey: "settings.nav.shortcuts", icon: Keyboard },
        { key: "observability", labelKey: "settings.nav.observability", icon: Activity },
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
  type Head = { title: string; subtitle: string; kicker?: string };
  const HEAD: Record<SettingsSubRoute, Head> = $derived({
    appearance: {
      title: $t("settings.nav.appearance"),
      subtitle: "Langue, thème et mode d'interface.",
      kicker: "PERSONNALISATION",
    },
    profile: {
      title: $t("settings.nav.profile"),
      subtitle: "Vos informations personnelles et préférences.",
      kicker: "PERSONNALISATION",
    },
    stt: {
      title: $t("settings.nav.stt"),
      subtitle: "Configurez le moteur de transcription vocale embarqué (whisper.cpp).",
      kicker: "IA",
    },
    llm: {
      title: $t("settings.nav.llm"),
      subtitle: "Configurez les backends de modèle de langage utilisés par Apollia et vos agents.",
      kicker: "IA",
    },
    "model-hub": {
      title: $t("settings.nav.model_hub"),
      subtitle: "Parcourez, téléchargez et gérez les modèles GGUF depuis HuggingFace.",
      kicker: "IA",
    },
    memories: {
      title: "Mes Mémoires",
      subtitle: "Consultez, validez ou corrigez les informations que le système a apprises sur vous.",
      kicker: "IA",
    },
    configuration: {
      title: $t("settings.nav.configuration"),
      subtitle: "Préférences générales du système.",
      kicker: "SYSTÈME",
    },
    tools: {
      title: "Outils natifs",
      subtitle: "Activez, désactivez et configurez chaque outil mis à disposition des agents.",
      kicker: "SYSTÈME",
    },
    permissions: {
      title: $t("settings.nav.permissions"),
      subtitle: "Visualisez et révoquez les autorisations accordées aux outils, par portée.",
      kicker: "SYSTÈME",
    },
    system: {
      title: $t("settings.nav.system"),
      subtitle: "Détails de l'installation et chemins de configuration.",
      kicker: "SYSTÈME",
    },
    shortcuts: {
      title: "Raccourcis clavier",
      subtitle: "Raccourcis disponibles dans Apollia.",
      kicker: "SYSTÈME",
    },
    observability: {
      title: $t("settings.observability.title"),
      subtitle: $t("settings.observability.subtitle"),
      kicker: "SYSTÈME",
    },
    danger: {
      title: "Zone dangereuse",
      subtitle:
        "Ces actions sont irréversibles. Assurez-vous d'avoir ce dont vous avez besoin avant de continuer.",
      kicker: "ZONE DE DANGER",
    },
  });

  const head = $derived(HEAD[$settingsSubRoute]);
  const currentDirty = $derived($settingsDirtyStore[$settingsSubRoute]?.dirty === true);

  // ─── Sub-nav rendering helper ──────────────────────────────────────
  function clusterTextClass(danger: boolean | undefined): string {
    return danger ? "text-destructive" : "text-muted-foreground";
  }

  function entryClass(active: boolean, danger: boolean): string {
    if (danger) {
      return active
        ? "bg-destructive/10 text-destructive border-destructive/20 font-semibold shadow-sm"
        : "text-destructive/80 hover:bg-destructive/5 hover:text-destructive border-transparent";
    }
    return active
      ? "bg-card text-foreground border-border font-semibold shadow-sm"
      : "text-muted-foreground hover:bg-muted/60 hover:text-foreground border-transparent";
  }
</script>

<!--
  V3 Operator Settings — three-column shell.
  - Outer rail/topbar lives outside this route.
  - Left sub-nav (240px): grouped tabs with section headers.
  - Right content: per-tab PageHeader + lazy-loaded sub-route component.
-->
<div
  class="flex h-full min-h-0 flex-1 flex-col"
  data-testid="settings-page"
>
  <div class="flex min-h-0 flex-1">
    {#if isMobile}
      <!-- Mobile: trigger + side sheet hosting the same nav. -->
      <div
        class="sticky top-0 z-30 mb-2 flex w-full items-center gap-2 border-b border-border bg-background/80 px-4 py-2 backdrop-blur-md"
      >
        <button
          type="button"
          class="inline-flex h-9 items-center gap-2 rounded-md border border-border bg-muted/50 px-3 text-sm font-medium text-foreground transition-colors hover:bg-muted"
          onclick={() => (mobileOpen = true)}
          aria-haspopup="dialog"
          aria-expanded={mobileOpen}
          data-testid="settings-mobile-nav-toggle"
        >
          <Menu size={16} strokeWidth={1.75} />
          <span>{$t("settings.nav.mobile_open")}</span>
        </button>
      </div>

      <Sheet open={mobileOpen} onclose={() => (mobileOpen = false)} side="left" width="sm">
        <div
          class="flex h-full flex-col overflow-y-auto p-4"
          data-testid="settings-mobile-nav-sheet"
        >
          <h2
            class="mb-4 text-foreground"
            style="font-size: 18px; font-weight: 600; letter-spacing: -0.3px;"
          >
            {$t("settings.nav.mobile_title")}
          </h2>
          {#each CLUSTERS as cluster (cluster.labelKey)}
            <div class="mb-4">
              <div
                class="mb-1.5 px-2 font-mono text-[10px] font-semibold uppercase tracking-[1.5px] {clusterTextClass(
                  cluster.danger,
                )}"
              >
                {$t(cluster.labelKey)}
              </div>
              <ul class="space-y-1">
                {#each cluster.entries as entry (entry.key)}
                  {@const SvelteIcon = entry.icon}
                  {@const isActive = $settingsSubRoute === entry.key}
                  <li>
                    <button
                      type="button"
                      onclick={() => onNavigate(entry.key)}
                      aria-current={isActive ? "page" : undefined}
                      data-settings-route={entry.key}
                      data-testid="settings-nav-{entry.key}"
                      class="flex w-full items-center gap-2.5 rounded-lg border px-2.5 py-1.5 text-[12.5px] transition-colors {entryClass(
                        isActive,
                        cluster.danger === true,
                      )}"
                    >
                      <SvelteIcon size={14} strokeWidth={1.75} aria-hidden="true" />
                      <span>{$t(entry.labelKey)}</span>
                    </button>
                  </li>
                {/each}
              </ul>
            </div>
          {/each}
        </div>
      </Sheet>
    {:else}
      <!-- Desktop: 240px left sub-nav. -->
      <nav
        class="flex w-[240px] flex-shrink-0 flex-col gap-[18px] border-r border-border bg-muted/30 px-3 py-5 overflow-y-auto"
        aria-label={$t("settings.nav.aria_label")}
        data-testid="settings-nav"
      >
        {#each CLUSTERS as cluster (cluster.labelKey)}
          <div>
            <div
              class="mb-1.5 px-2.5 font-mono text-[10px] font-semibold uppercase tracking-[1.5px] {clusterTextClass(
                cluster.danger,
              )}"
            >
              {$t(cluster.labelKey)}
            </div>
            <ul class="space-y-0.5">
              {#each cluster.entries as entry (entry.key)}
                {@const SvelteIcon = entry.icon}
                {@const isActive = $settingsSubRoute === entry.key}
                <li>
                  <button
                    type="button"
                    onclick={() => onNavigate(entry.key)}
                    aria-current={isActive ? "page" : undefined}
                    data-settings-route={entry.key}
                    data-testid="settings-nav-{entry.key}"
                    class="flex w-full items-center gap-[9px] rounded-lg border px-2.5 py-[7px] text-[12.5px] transition-colors {entryClass(
                      isActive,
                      cluster.danger === true,
                    )}"
                  >
                    <SvelteIcon
                      size={13}
                      strokeWidth={1.75}
                      aria-hidden="true"
                      class={cluster.danger
                        ? "text-destructive"
                        : isActive
                          ? "text-primary"
                          : "text-muted-foreground"}
                    />
                    <span>{$t(entry.labelKey)}</span>
                  </button>
                </li>
              {/each}
            </ul>
          </div>
        {/each}
      </nav>
    {/if}

    <!-- Right content column. -->
    <div class="flex min-w-0 flex-1 flex-col overflow-hidden" data-testid="settings-content">
      <!-- Per-tab PageHeader + saved-hint. -->
      <div
        class="flex items-end justify-between gap-6 border-b border-border/60 px-10 pt-[26px] pb-[18px]"
      >
        <div class="min-w-0">
          {#if head.kicker}
            <div
              class="mb-1.5 font-mono text-[10.5px] font-semibold uppercase tracking-[1.5px] text-muted-foreground/70"
            >
              {head.kicker}
            </div>
          {/if}
          <h1
            class="m-0 text-foreground"
            style="font-size: 26px; font-weight: 600; letter-spacing: -0.5px; line-height: 1.15;"
          >
            {head.title}
          </h1>
          <p class="mt-1.5 max-w-[640px] text-[12.5px] leading-[1.5] text-muted-foreground">
            {head.subtitle}
          </p>
        </div>
        <div class="flex items-center gap-3 pb-1">
          <UnsavedChangesBadge />
          {#if !currentDirty}
            <span
              class="inline-flex items-center gap-1.5 text-[11px] text-success-a11y"
              data-testid="settings-saved-hint"
            >
              <Check size={11} strokeWidth={2} />
              Tous les changements enregistrés
            </span>
          {/if}
        </div>
      </div>

      {#if loadError}
        <div
          class="mx-10 mt-4 rounded-md border border-destructive/30 bg-destructive/5 px-4 py-2 text-sm text-destructive"
        >
          {loadError}
        </div>
      {/if}

      <!-- Scrollable tab body — sub-routes own their cards/forms. -->
      <div
        bind:this={scrollContainer}
        data-settings-scroll
        class="flex-1 overflow-y-auto px-10 pt-2 pb-8"
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
