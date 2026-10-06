<script lang="ts" module>
  import type { Route } from "$lib/stores/navigation";
  import {
    MessageSquare, Bot, ShieldCheck, Plug, Code, Settings, KeyRound,
    Repeat, Brain, Database, Activity, Mic, Library, ScrollText,
  } from "lucide-svelte";

  type Badge = "none" | "chat" | "approvals";

  /** A primary entry of the sidebar; `developer` hides it with the mode off. */
  export interface PrimaryNavEntry {
    route: Route;
    Icon: typeof MessageSquare;
    labelKey: string;
    badge: Badge;
    developer?: boolean;
  }

  // Each entry's `labelKey` resolves through svelte-i18n at render time. Keeping
  // labels out of the source array lets the bundle preserve a single
  // translation map (`nav.sidebar.*`) shared by the column, the rail and the
  // drawer.
  //
  // The first rank: what is used every day, in this order. The developer
  // screen closes it, with the developer mode on only.
  export const PRIMARY_NAV: readonly PrimaryNavEntry[] = [
    { route: "chat"           as Route, Icon: MessageSquare,   labelKey: "nav.sidebar.chat",           badge: "chat"      },
    { route: "agents"         as Route, Icon: Bot,             labelKey: "nav.sidebar.agents",         badge: "none"      },
    { route: "integrations"   as Route, Icon: Plug,            labelKey: "nav.sidebar.integrations",   badge: "none"      },
    { route: "llm"            as Route, Icon: Brain,           labelKey: "nav.sidebar.llm",            badge: "none"      },
    { route: "inbox"          as Route, Icon: ShieldCheck,     labelKey: "nav.sidebar.inbox",          badge: "approvals" },
    { route: "developer"      as Route, Icon: Code,            labelKey: "nav.sidebar.developer",      badge: "none", developer: true },
  ];

  /** The primary entries shown for a given developer mode. */
  export function visiblePrimaryNav(developerMode: boolean): PrimaryNavEntry[] {
    return PRIMARY_NAV.filter((entry) => developerMode || !entry.developer);
  }

  /** An entry of the second rank, or of the developer group below it. */
  export interface NavEntry {
    route: Route;
    Icon: typeof MessageSquare;
    labelKey: string;
  }

  /** An entry of the developer group, below the secondary entries. */
  export type DeveloperNavEntry = NavEntry;

  // The second rank: what is consulted now and then, drawn quieter than the
  // first. It is the same with the developer mode off and on.
  export const SECONDARY_NAV: readonly NavEntry[] = [
    { route: "library"        as Route, Icon: Library,    labelKey: "nav.sidebar.library" },
    { route: "automations"    as Route, Icon: Repeat,     labelKey: "nav.sidebar.automations" },
    { route: "authorizations" as Route, Icon: KeyRound,   labelKey: "nav.sidebar.authorizations" },
    { route: "journal"        as Route, Icon: ScrollText, labelKey: "nav.sidebar.journal" },
    { route: "settings"       as Route, Icon: Settings,   labelKey: "nav.sidebar.settings" },
  ];

  // The technical screens, grouped apart from the everyday navigation. Their
  // routes are the ones the router keeps out of reach with the mode off
  // (`DEVELOPER_ROUTES`); a test holds the two lists equal.
  export const DEVELOPER_NAV: readonly DeveloperNavEntry[] = [
    { route: "transcriptions" as Route, Icon: Mic,      labelKey: "nav.sidebar.transcriptions" },
    { route: "observability"  as Route, Icon: Activity, labelKey: "nav.sidebar.observability" },
    { route: "memory"         as Route, Icon: Database, labelKey: "nav.sidebar.memory" },
  ];

  /** The developer group for a given developer mode: empty with the mode off. */
  export function visibleDeveloperNav(developerMode: boolean): DeveloperNavEntry[] {
    return developerMode ? [...DEVELOPER_NAV] : [];
  }

  /**
   * Every route the sidebar lists for a given developer mode, in reading
   * order: the first rank, the second, then the developer group.
   */
  export function visibleSidebarRoutes(developerMode: boolean): Route[] {
    return [
      ...visiblePrimaryNav(developerMode),
      ...SECONDARY_NAV,
      ...visibleDeveloperNav(developerMode),
    ].map((entry) => entry.route);
  }
</script>

<script lang="ts">
  import { fade, fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { t } from "svelte-i18n";
  import { currentRoute, navigateTo, sidebarRoute } from "$lib/stores/navigation";
  import { sidebarState, drawerOpen, layoutActions } from "$lib/stores/layout";
  import { pendingCount } from "$lib/stores/hitl";
  import {
    activeChatCount,
    pendingChatApprovalCount,
    pendingUserInputCount,
    openNewChatRequested,
  } from "$lib/stores/chat";
  import { developerMode } from "$lib/stores/developerMode";
  import { navPosition } from "$lib/stores/chatAppearance";
  import { navSide } from "$lib/layout/navSide";
  import { resourceWidgetVisible } from "$lib/stores/developerSettings";
  import { whileShown } from "$lib/settings/resourceMonitor";
  import { openSettingsWindow } from "$lib/stores/settingsWindow";
  import ResourceWidget from "./ResourceWidget.svelte";
  import NavRow from "./NavRow.svelte";
  import { Button } from "$lib/components/ui/button";
  import { X, Plus, User } from "lucide-svelte";

  const primaryNav = $derived(visiblePrimaryNav($developerMode));
  const developerNav = $derived(visibleDeveloperNav($developerMode));

  const isDrawer = $derived($sidebarState === "drawer");
  // A medium window keeps the icons alone; a wide one names every entry.
  const isRail = $derived($sidebarState === "icon");

  // The consumption widget reads the monitor of System > Hardware, and only
  // while it is shown: hidden, the subscription is dropped and nothing is
  // measured for it.
  const widgetUsage = whileShown(resourceWidgetVisible);

  function openResourceMonitor() {
    openSettingsWindow("hardware");
    if (isDrawer) layoutActions.closeDrawer();
  }

  // The entry marked: the one the screen shown belongs to.
  const shownEntry = $derived(sidebarRoute($currentRoute));

  const approvalsCount = $derived(
    $pendingCount + $pendingChatApprovalCount + $pendingUserInputCount,
  );

  let drawerRef: HTMLElement | null = $state(null);
  const side = $derived(navSide($navPosition));
  let previouslyFocused: HTMLElement | null = null;

  $effect(() => {
    $currentRoute;
    if (!isDrawer) return;
    layoutActions.closeDrawer();
  });

  $effect(() => {
    if (!(isDrawer && $drawerOpen)) return;
    previouslyFocused = document.activeElement as HTMLElement | null;
    const id = requestAnimationFrame(() => {
      const focusable = drawerRef?.querySelector<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
      );
      focusable?.focus();
    });
    return () => { cancelAnimationFrame(id); previouslyFocused?.focus?.(); previouslyFocused = null; };
  });

  $effect(() => {
    if (!(isDrawer && $drawerOpen)) return;
    const original = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => { document.body.style.overflow = original; };
  });

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") { event.preventDefault(); layoutActions.closeDrawer(); return; }
  }

  function navigate(route: Route) {
    navigateTo(route);
    if (isDrawer) layoutActions.closeDrawer();
  }

  function startNewChat() {
    navigate("chat");
    openNewChatRequested.set(Date.now());
  }

  function getBadgeCount(badge: string): number {
    if (badge === "chat") return $activeChatCount;
    if (badge === "approvals") return approvalsCount;
    return 0;
  }
</script>

<!--
  The two ranks and the developer group, as labelled rows: the body of the
  column and of the drawer. `titleId` names the developer group for assistive
  technology, and differs between the two so an id is never drawn twice.
-->
{#snippet ranks(titleId: string)}
  <div class="flex flex-col gap-0.5 px-3 pb-3" data-testid="sidebar-primary-rank">
    {#each primaryNav as item (item.route)}
      <NavRow
        route={item.route}
        Icon={item.Icon}
        label={$t(item.labelKey)}
        rank="primary"
        active={shownEntry === item.route}
        count={getBadgeCount(item.badge)}
        onselect={navigate}
      />
    {/each}
  </div>

  <div class="mx-3 flex flex-col gap-0.5 border-t border-border py-3" data-testid="sidebar-secondary-rank">
    {#each SECONDARY_NAV as item (item.route)}
      <NavRow
        route={item.route}
        Icon={item.Icon}
        label={$t(item.labelKey)}
        rank="secondary"
        active={shownEntry === item.route}
        onselect={navigate}
      />
    {/each}
  </div>

  {#if developerNav.length > 0}
    <div
      class="mx-3 flex flex-col gap-0.5 border-t border-border py-3"
      role="group"
      aria-labelledby={titleId}
      data-testid="sidebar-developer-group"
    >
      <span
        id={titleId}
        class="px-3 pb-1 font-mono text-overline uppercase tracking-widest text-muted-foreground"
      >{$t("nav.developer_group")}</span>
      {#each developerNav as item (item.route)}
        <NavRow
          route={item.route}
          Icon={item.Icon}
          label={$t(item.labelKey)}
          rank="developer"
          active={shownEntry === item.route}
          onselect={navigate}
        />
      {/each}
    </div>
  {/if}
{/snippet}

{#if isDrawer}
  {#if $drawerOpen}
    <div
      class="fixed inset-0 z-backdrop bg-backdrop backdrop-blur-sm"
      role="button"
      tabindex="-1"
      aria-label={$t("nav.close_sidebar")}
      onclick={() => layoutActions.closeDrawer()}
      onkeydown={handleKeydown}
      transition:fade={{ duration: 180 }}
      data-testid="sidebar-drawer-backdrop"
    ></div>
    <div
      bind:this={drawerRef}
      class="fixed inset-y-0 {side.drawerEdge} z-overlay flex w-64 max-w-[85vw] flex-col glass-panel glass-border shadow-xl"
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-label={$t("nav.sidebar_label")}
      onkeydown={handleKeydown}
      transition:fly={{ x: side.drawerFlyX, duration: 220, easing: cubicOut }}
      data-testid="sidebar"
      data-state="drawer"
    >
      <!-- Drawer header -->
      <div class="flex items-center gap-2 px-4 py-4">
        <img src="/logo.svg" alt="Apollia" width="28" height="28" class="h-7 w-7 shrink-0" />
        <span class="text-base font-semibold text-foreground">Apollia OS</span>
        <div class="ml-auto">
          <button
            class="inline-flex min-h-9 min-w-9 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
            onclick={() => layoutActions.closeDrawer()}
            aria-label={$t("nav.close_sidebar")}
          >
            <X size={18} strokeWidth={1.5} />
          </button>
        </div>
      </div>

      <div class="px-3 pb-3">
        <Button variant="ghost" size="sm"
          class="flex w-full items-center gap-2 rounded-lg bg-primary px-3 py-2 text-sm font-medium text-primary-foreground shadow-primary-sm hover:bg-primary/90"
          onclick={startNewChat}
        >
          <Plus size={15} strokeWidth={2} class="shrink-0" />
          <span>{$t("chat.new_chat")}</span>
        </Button>
      </div>

      <nav class="flex flex-1 flex-col overflow-y-auto" aria-label={$t("nav.sidebar_label")}>
        {@render ranks("sidebar-drawer-developer-title")}
      </nav>

      {#if $resourceWidgetVisible}
        <div class="border-t border-border/60 p-3">
          <ResourceWidget usage={$widgetUsage} onopen={openResourceMonitor} />
        </div>
      {/if}
    </div>
  {/if}
{:else if isRail}
  <!-- Medium window: the same entries, icons alone, the name in a tooltip. -->
  <aside
    class="v4-hair flex h-full w-16 flex-shrink-0 flex-col items-center {side.railBorder} border-border bg-surface-1 py-3"
    class:rail-right={side.nav === "right"}
    role="navigation"
    aria-label={$t("nav.sidebar_label")}
    data-testid="sidebar"
    data-state="rail"
    data-nav-position={side.nav}
  >
    <div class="mb-2 p-1.5">
      <img src="/logo.svg" alt="Apollia" class="block h-8 w-8 rounded-lg" />
    </div>

    <div class="flex flex-col items-center gap-0.5" data-testid="sidebar-primary-rank">
      {#each primaryNav as item (item.route)}
        <NavRow
          variant="rail"
          tooltipClass={side.railTooltip}
          route={item.route}
          Icon={item.Icon}
          label={$t(item.labelKey)}
          rank="primary"
          active={shownEntry === item.route}
          count={getBadgeCount(item.badge)}
          onselect={navigate}
        />
      {/each}
    </div>

    <div class="mt-2 flex flex-col items-center gap-0.5 border-t border-border pt-2" data-testid="sidebar-secondary-rank">
      {#each SECONDARY_NAV as item (item.route)}
        <NavRow
          variant="rail"
          tooltipClass={side.railTooltip}
          route={item.route}
          Icon={item.Icon}
          label={$t(item.labelKey)}
          rank="secondary"
          active={shownEntry === item.route}
          onselect={navigate}
        />
      {/each}
    </div>

    {#if developerNav.length > 0}
      <div
        class="mt-2 flex flex-col items-center gap-0.5 border-t border-border pt-2"
        role="group"
        aria-label={$t("nav.developer_group")}
        data-testid="sidebar-developer-group"
      >
        {#each developerNav as item (item.route)}
          <NavRow
            variant="rail"
            tooltipClass={side.railTooltip}
            route={item.route}
            Icon={item.Icon}
            label={$t(item.labelKey)}
            rank="developer"
            active={shownEntry === item.route}
            onselect={navigate}
          />
        {/each}
      </div>
    {/if}

    <div class="flex-1"></div>

    {#if $resourceWidgetVisible}
      <ResourceWidget usage={$widgetUsage} compact onopen={openResourceMonitor} />
    {/if}

    <!-- User avatar -->
    <div class="mt-1">
      <span
        class="avatar-warm flex h-7 w-7 items-center justify-center rounded-lg text-primary-foreground shadow-elev-1"
        title={$t("settings.profile.title")}
      ><User size={14} /></span>
    </div>
  </aside>
{:else}
  <!-- Wide window: the column, every entry named, in two ranks. -->
  <aside
    class="v4-hair flex h-full w-60 flex-shrink-0 flex-col {side.railBorder} border-border bg-surface-1"
    role="navigation"
    aria-label={$t("nav.sidebar_label")}
    data-testid="sidebar"
    data-state="column"
    data-nav-position={side.nav}
  >
    <div class="flex items-center gap-2.5 px-6 pb-4 pt-5">
      <img src="/logo.svg" alt="Apollia" width="28" height="28" class="h-7 w-7 shrink-0" />
      <span class="text-base font-medium text-foreground">Apollia OS</span>
    </div>

    <div class="flex min-h-0 flex-1 flex-col overflow-y-auto">
      {@render ranks("sidebar-developer-group-title")}
    </div>

    {#if $resourceWidgetVisible}
      <div class="border-t border-border p-3">
        <ResourceWidget usage={$widgetUsage} onopen={openResourceMonitor} />
      </div>
    {/if}
  </aside>
{/if}

<style>
  /* Terracotta "current user" avatar chip. The gradient token keeps its
     hand-tuned tone independent of the brand palette. */
  .avatar-warm {
    background: var(--avatar-gradient-warm);
  }
</style>
