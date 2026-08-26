<script lang="ts">
  import { fade, fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { t } from "svelte-i18n";
  import { currentRoute, navigateTo, type Route } from "$lib/stores/navigation";
  import { sidebarState, drawerOpen, layoutActions } from "$lib/stores/layout";
  import { pendingCount } from "$lib/stores/hitl";
  import {
    activeChatCount,
    pendingChatApprovalCount,
    pendingUserInputCount,
    openNewChatRequested,
  } from "$lib/stores/chat";
  import { runningTasks, tasksRunningCount } from "$lib/stores/tasks";
  import { uiMode } from "$lib/stores/mode";
  import { Button } from "$lib/components/ui/button";
  import {
    LayoutDashboard, MessageSquare, Bot, FolderKanban, ListChecks, ShieldCheck, Plug,
    Settings, X, Plus, Repeat,
    Brain, Database, Activity, Mic, Bell, User,
  } from "lucide-svelte";

  // Each entry's `labelKey` resolves through svelte-i18n at render time. Keeping
  // labels out of the source array lets the bundle preserve a single
  // translation map (`nav.sidebar.*`) shared by drawer + rail.
  const PRIMARY_NAV = [
    { route: "dashboard"     as Route, Icon: LayoutDashboard, labelKey: "nav.sidebar.dashboard",     badge: "none"      },
    { route: "chat"          as Route, Icon: MessageSquare,   labelKey: "nav.sidebar.chat",          badge: "chat"      },
    { route: "agents"        as Route, Icon: Bot,             labelKey: "nav.sidebar.agents",        badge: "none"      },
    { route: "projects"      as Route, Icon: FolderKanban,    labelKey: "nav.sidebar.projects",      badge: "none"      },
    { route: "tasks"         as Route, Icon: ListChecks,      labelKey: "nav.sidebar.tasks",         badge: "tasks"     },
    { route: "automations"   as Route, Icon: Repeat,          labelKey: "nav.sidebar.automations",   badge: "none"      },
    { route: "inbox"         as Route, Icon: ShieldCheck,     labelKey: "nav.sidebar.inbox",         badge: "approvals" },
    { route: "notifications" as Route, Icon: Bell,            labelKey: "nav.sidebar.notifications", badge: "none"      },
    { route: "memory"        as Route, Icon: Database,        labelKey: "nav.sidebar.memory",        badge: "none"      },
    { route: "observability" as Route, Icon: Activity,        labelKey: "nav.sidebar.observability", badge: "none"      },
  ];

  // Builder cluster - purely technical views (shown in Builder mode only).
  const BUILDER_NAV = [
    { route: "llm"            as Route, Icon: Brain, labelKey: "nav.sidebar.llm" },
    { route: "transcriptions" as Route, Icon: Mic,   labelKey: "nav.sidebar.transcriptions" },
  ];

  const SECONDARY_NAV = [
    { route: "integrations" as Route, Icon: Plug,     labelKey: "nav.sidebar.integrations" },
    { route: "settings"     as Route, Icon: Settings, labelKey: "nav.sidebar.settings" },
  ];

  const isBuilder = $derived($uiMode === "builder");


  const isDrawer = $derived($sidebarState === "drawer");

  const approvalsCount = $derived(
    $pendingCount + $pendingChatApprovalCount + $pendingUserInputCount,
  );
  const runningCount = $derived($runningTasks.length);
  const inFlightCount = $derived($tasksRunningCount);

  let pulseKey = $state(0);
  let lastInFlight = 0;
  $effect(() => {
    if (inFlightCount !== lastInFlight) {
      lastInFlight = inFlightCount;
      pulseKey += 1;
      const n = pulseKey;
      setTimeout(() => { if (n === pulseKey) pulseKey = 0; }, 400);
    }
  });

  let drawerRef: HTMLElement | null = $state(null);
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
    if (badge === "tasks") return inFlightCount;
    return 0;
  }
</script>

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
      class="fixed inset-y-0 left-0 z-overlay flex w-64 max-w-[85vw] flex-col glass-panel border-r glass-border shadow-xl"
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-label={$t("nav.sidebar_label")}
      onkeydown={handleKeydown}
      transition:fly={{ x: -280, duration: 220, easing: cubicOut }}
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

      <div class="px-3 pb-2">
        <Button variant="ghost" size="sm"
          class="flex w-full items-center gap-2 rounded-lg bg-primary px-3 py-2 text-sm font-medium text-primary-foreground shadow-primary-sm hover:bg-primary/90"
          onclick={startNewChat}
        >
          <Plus size={15} strokeWidth={2} class="shrink-0" />
          <span>{$t("chat.new_chat")}</span>
        </Button>
      </div>

      <nav class="flex flex-1 flex-col gap-0.5 overflow-y-auto p-3" aria-label={$t("nav.sidebar_label")}>
        {#each PRIMARY_NAV as item (item.route)}
          {@const isActive = $currentRoute === item.route}
          {@const badgeCount = getBadgeCount(item.badge)}
          {@const label = $t(item.labelKey)}
          <button
            type="button"
            class="relative flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors {isActive ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-primary/[0.04] hover:text-foreground'}"
            onclick={() => navigate(item.route)}
            aria-current={isActive ? "page" : undefined}
          >
            <item.Icon size={17} strokeWidth={1.5} class="shrink-0" />
            <span class="truncate">{label}</span>
            {#if badgeCount > 0}
              <span class="ml-auto min-w-5 rounded-full bg-primary px-1.5 py-0.5 text-center text-overline font-semibold text-primary-foreground leading-none">{badgeCount}</span>
            {:else if item.badge === "tasks" && runningCount > 0}
              <span class="ml-auto flex h-2 w-2"><span class="absolute inline-flex h-2 w-2 animate-ping rounded-full bg-primary opacity-60"></span><span class="relative inline-flex h-2 w-2 rounded-full bg-primary"></span></span>
            {/if}
          </button>
        {/each}

        {#if isBuilder}
          <div class="section-meta mt-3 mb-1 px-3" data-testid="sidebar-builder-cluster-label">
            {$t("nav.builder_cluster")}
          </div>
          {#each BUILDER_NAV as item (item.route)}
            {@const isActive = $currentRoute === item.route}
            {@const label = $t(item.labelKey)}
            <button
              type="button"
              class="relative flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors {isActive ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-primary/[0.04] hover:text-foreground'}"
              onclick={() => navigate(item.route)}
              aria-current={isActive ? "page" : undefined}
              data-testid="nav-{item.route}"
            >
              <item.Icon size={17} strokeWidth={1.5} class="shrink-0" />
              <span class="truncate">{label}</span>
            </button>
          {/each}
        {/if}
      </nav>

      <div class="border-t border-border/60 p-3 flex flex-col gap-0.5">
        {#each SECONDARY_NAV as item (item.route)}
          {@const isActive = $currentRoute === item.route}
          {@const label = $t(item.labelKey)}
          <button
            type="button"
            class="flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors {isActive ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-muted hover:text-foreground'}"
            onclick={() => navigate(item.route)}
            data-testid="nav-{item.route}"
          >
            <item.Icon size={17} strokeWidth={1.5} class="shrink-0" />
            <span>{label}</span>
          </button>
        {/each}
      </div>
    </div>
  {/if}
{:else}
  <!-- V3 64px icon rail - permanent, no expand -->
  <aside
    class="rail v4-hair flex h-full w-16 flex-shrink-0 flex-col items-center border-r border-border bg-muted py-3 gap-1"
    role="navigation"
    aria-label={$t("nav.sidebar_label")}
    data-testid="sidebar"
    data-state="rail"
  >
    <!-- Logo -->
    <div class="mb-4 p-1.5">
      <img src="/logo.svg" alt="Apollia" class="h-9 w-9 rounded-lg block" />
    </div>

    <!-- Primary nav -->
    {#each PRIMARY_NAV as item (item.route)}
      {@const isActive = $currentRoute === item.route}
      {@const badgeCount = getBadgeCount(item.badge)}
      {@const isPulse = item.badge === "tasks" && runningCount > 0}
      {@const label = $t(item.labelKey)}
      <div class="rail-item group relative mb-1">
        <button
          type="button"
          class="relative flex h-10 w-10 items-center justify-center rounded-lg transition-all duration-fast {isActive ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-surface-2 hover:text-foreground'}"
          onclick={() => navigate(item.route)}
          aria-current={isActive ? "page" : undefined}
          aria-label={label}
          data-nav-item="true"
          data-testid="nav-{item.route}"
        >
          {#if isActive}
            <span class="active-bar"></span>
          {/if}
          <item.Icon size={17} strokeWidth={1.5} />
          {#if badgeCount > 0}
            <span class="absolute -right-0.5 -top-0.5 min-w-3.5 rounded-full bg-primary px-1 py-px text-center text-overline font-semibold text-primary-foreground leading-none">{badgeCount > 9 ? "9+" : badgeCount}</span>
          {:else if isPulse}
            <span class="absolute right-0.5 top-0.5 flex h-1.5 w-1.5">
              <span class="absolute inline-flex h-full w-full animate-ping rounded-full bg-primary opacity-60"></span>
              <span class="relative inline-flex h-1.5 w-1.5 rounded-full bg-primary"></span>
            </span>
          {/if}
        </button>
        <!--
          Tooltip. The rail entries carry `aria-label` and no `title`: a native
          `title` would draw a second label, in the system chrome and after the
          system delay, over the one below.
        -->
        <span class="tooltip pointer-events-none absolute left-full top-1/2 z-overlay ml-2 -translate-y-1/2 whitespace-nowrap rounded-md bg-foreground px-2 py-1 text-caption font-medium text-background opacity-0 shadow-elev-3 transition-opacity duration-fast group-hover:opacity-100">
          {label}
        </span>
      </div>
    {/each}

    {#if isBuilder}
      <!-- Builder cluster - Inspection -->
      <div class="my-1 h-px w-8 bg-border/60" data-testid="sidebar-builder-separator"></div>
      {#each BUILDER_NAV as item (item.route)}
        {@const isActive = $currentRoute === item.route}
        {@const label = $t(item.labelKey)}
        <div class="rail-item group relative mb-1">
          <button
            type="button"
            class="relative flex h-10 w-10 items-center justify-center rounded-lg transition-all duration-fast {isActive ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-surface-2 hover:text-foreground'}"
            onclick={() => navigate(item.route)}
            aria-current={isActive ? "page" : undefined}
            aria-label={label}
            data-nav-item="true"
            data-testid="nav-{item.route}"
          >
            {#if isActive}
              <span class="active-bar"></span>
            {/if}
            <item.Icon size={17} strokeWidth={1.5} />
          </button>
          <span class="tooltip pointer-events-none absolute left-full top-1/2 z-overlay ml-2 -translate-y-1/2 whitespace-nowrap rounded-md bg-foreground px-2 py-1 text-caption font-medium text-background opacity-0 shadow-elev-3 transition-opacity duration-fast group-hover:opacity-100">
            {label}
          </span>
        </div>
      {/each}
    {/if}

    <!-- Spacer -->
    <div class="flex-1"></div>

    <!-- Secondary nav (Connections, Settings) -->
    {#each SECONDARY_NAV as item (item.route)}
      {@const isActive = $currentRoute === item.route}
      {@const label = $t(item.labelKey)}
      <div class="rail-item group relative mb-1">
        <button
          type="button"
          class="relative flex h-10 w-10 items-center justify-center rounded-lg transition-all duration-fast {isActive ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-surface-2 hover:text-foreground'}"
          onclick={() => navigate(item.route)}
          aria-current={isActive ? "page" : undefined}
          aria-label={label}
          data-testid="nav-{item.route}"
        >
          {#if isActive}
            <span class="active-bar"></span>
          {/if}
          <item.Icon size={17} strokeWidth={1.5} />
        </button>
        <span class="tooltip pointer-events-none absolute left-full top-1/2 z-overlay ml-2 -translate-y-1/2 whitespace-nowrap rounded-md bg-foreground px-2 py-1 text-caption font-medium text-background opacity-0 shadow-elev-3 transition-opacity duration-fast group-hover:opacity-100">
          {label}
        </span>
      </div>
    {/each}

    <!-- User avatar -->
    <div class="mt-1">
      <span
        class="avatar-warm flex h-7 w-7 items-center justify-center rounded-lg text-primary-foreground shadow-elev-1"
        title={$t("settings.profile.title")}
      ><User size={14} /></span>
    </div>
  </aside>
{/if}

<style>
  .rail-item {
    position: relative;
  }

  /* The 64px rail seats a 40px button, leaving a 12px gutter each side. The
     active indicator anchors to the rail's left edge via that gutter, so the
     offset lives in one place instead of a repeated inline `left: -14px`. */
  .rail {
    --rail-gutter: 0.75rem;
  }

  /* Active-route indicator: a 3px indigo->violet bar that grows in from the
     rail edge. The gradient reuses the signature `--grad-a/--grad-b` stops, so
     both themes resolve automatically. Under reduced motion the global rule in
     app.css neutralizes the entry animation. */
  .active-bar {
    position: absolute;
    left: calc(-1 * var(--rail-gutter));
    top: 0.625rem;
    bottom: 0.625rem;
    width: 3px;
    border-radius: 0 3px 3px 0;
    background: linear-gradient(180deg, hsl(var(--grad-a)), hsl(var(--grad-b)));
    transform-origin: center;
    animation: active-bar-in var(--motion-base) var(--ease-apple);
  }

  @keyframes active-bar-in {
    from {
      transform: scaleY(0);
      opacity: 0;
    }
    to {
      transform: scaleY(1);
      opacity: 1;
    }
  }

  /* Terracotta "current user" avatar chip. The gradient token keeps its
     hand-tuned tone independent of the brand palette. */
  .avatar-warm {
    background: var(--avatar-gradient-warm);
  }
</style>
