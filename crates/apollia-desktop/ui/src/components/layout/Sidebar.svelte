<script lang="ts">
  import { fade, fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { t } from "svelte-i18n";
  import { currentRoute, navigateTo, type Route } from "$lib/stores/navigation";
  import {
    sidebarState,
    drawerOpen,
    layoutActions,
  } from "$lib/stores/layout";
  import { connectionStatus } from "$lib/stores/sse";
  import { pendingCount } from "$lib/stores/hitl";
  import { activeChatCount, pendingChatApprovalCount } from "$lib/stores/chat";
  import { uiMode } from "$lib/stores/mode";
  import { Badge } from "$lib/components/ui/badge";
  import { Separator } from "$lib/components/ui/separator";
  import { showOnboardingBadge, onboardingStore } from "$lib/stores/onboarding";
  import CompanionToggle from "../companion/CompanionToggle.svelte";
  import { runningTasks } from "$lib/stores/tasks";
  import {
    LayoutDashboard,
    Bot,
    ListChecks,
    MessageSquare,
    ShieldCheck,
    Plug,
    Brain,
    Timer,
    GitBranch,
    Database,
    Bell,
    Activity,
    Settings,
    Layers,
    PanelLeftClose,
    PanelLeftOpen,
    Sparkles,
    Mic,
    FolderOpen,
    X,
  } from "lucide-svelte";
  import type { ComponentType } from "svelte";

  type NavItem = { route: Route; labelKey: string; icon: ComponentType };
  type NavGroup = { labelKey: string; items: NavItem[] };

  const operatorNav: NavItem[] = [
    { route: "dashboard", labelKey: "nav.home", icon: LayoutDashboard },
    { route: "agents", labelKey: "nav.my_assistants", icon: Bot },
    { route: "projects", labelKey: "nav.projects", icon: FolderOpen },
    { route: "tasks", labelKey: "nav.activity", icon: ListChecks },
    { route: "chat", labelKey: "nav.chat", icon: MessageSquare },
    { route: "automations", labelKey: "nav.automations", icon: Timer },
    { route: "integrations", labelKey: "nav.connections", icon: Plug },
    { route: "inbox", labelKey: "nav.inbox_operator", icon: ShieldCheck },
  ];

  const builderNavGroups: NavGroup[] = [
    {
      labelKey: "nav.operations",
      items: [
        { route: "dashboard", labelKey: "nav.dashboard", icon: LayoutDashboard },
        { route: "agents", labelKey: "nav.agents", icon: Bot },
        { route: "projects", labelKey: "nav.projects", icon: FolderOpen },
        { route: "tasks", labelKey: "nav.tasks", icon: ListChecks },
        { route: "chat", labelKey: "nav.chat", icon: MessageSquare },
        { route: "inbox", labelKey: "nav.inbox_builder", icon: ShieldCheck },
      ],
    },
    {
      labelKey: "nav.infrastructure",
      items: [
        { route: "llm", labelKey: "nav.llm", icon: Brain },
        { route: "triggers", labelKey: "nav.triggers", icon: Timer },
        { route: "pipelines", labelKey: "nav.pipelines", icon: GitBranch },
        { route: "integrations", labelKey: "nav.mcp_servers", icon: Plug },
      ],
    },
    {
      labelKey: "nav.data",
      items: [
        { route: "memory", labelKey: "nav.memory", icon: Database },
        { route: "transcriptions", labelKey: "nav.transcriptions", icon: Mic },
        { route: "notifications", labelKey: "nav.notifications", icon: Bell },
        { route: "observability", labelKey: "nav.observability", icon: Activity },
      ],
    },
  ];

  const settingsItem: NavItem = {
    route: "settings",
    labelKey: "nav.settings",
    icon: Settings,
  };

  function navigate(route: Route) {
    navigateTo(route);
    // Route change auto-closes the drawer (AC US-SP42-003).
    layoutActions.closeDrawer();
  }

  /**
   * Open the Apollia Guide meta-chat (US-SP42-057). Deep-links to the chat
   * surface with `?agent=apollia-guide&mode=guide` so `Chat.svelte` can
   * open (or resume) the pinned guide session and paint the warm shell.
   */
  function openApolliaGuide() {
    if (typeof window !== "undefined") {
      const url = new URL(window.location.href);
      url.search = "?agent=apollia-guide&mode=guide";
      window.history.replaceState({}, "", url.toString());
    }
    navigate("chat");
  }

  function toggleMode() {
    uiMode.update((m) => (m === "operator" ? "builder" : "operator"));
  }

  const CONNECTION_KEYS: Record<string, string> = {
    connecting: "nav.connection.connecting",
    connected: "nav.connection.connected",
    reconnecting: "nav.connection.reconnecting",
    error: "nav.connection.error",
  };

  const isOperator = $derived($uiMode === "operator");
  const currentState = $derived($sidebarState);
  const isDrawer = $derived(currentState === "drawer");
  const isIcon = $derived(currentState === "icon");
  const isExpanded = $derived(currentState === "expanded");
  // For the drawer, the aside is rendered expanded inside the overlay.
  const showLabels = $derived(isExpanded || (isDrawer && $drawerOpen));
  const runningCount = $derived($runningTasks.length);

  // ── A11y : focus trap + autofocus + route-change auto-close ────────────────

  let drawerRef: HTMLElement | null = $state(null);
  let previouslyFocused: HTMLElement | null = null;

  $effect(() => {
    // Close the drawer as soon as the route changes.
    $currentRoute; // subscribe
    if (!isDrawer) return;
    layoutActions.closeDrawer();
  });

  $effect(() => {
    if (!(isDrawer && $drawerOpen)) return;
    previouslyFocused = document.activeElement as HTMLElement | null;
    // Defer focus to next tick so the drawer is mounted.
    const id = requestAnimationFrame(() => {
      const focusable = drawerRef?.querySelector<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
      );
      focusable?.focus();
    });
    return () => {
      cancelAnimationFrame(id);
      previouslyFocused?.focus?.();
      previouslyFocused = null;
    };
  });

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      layoutActions.closeDrawer();
      return;
    }
    if (event.key !== "Tab" || !drawerRef) return;
    const focusables = drawerRef.querySelectorAll<HTMLElement>(
      'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
    );
    if (focusables.length === 0) return;
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  // Scroll lock while the drawer is open.
  $effect(() => {
    if (!(isDrawer && $drawerOpen)) return;
    const original = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = original;
    };
  });

  // ── A11y : keyboard navigation across nav buttons (ArrowUp/Down) ───────────
  //
  // The sidebar is a vertical list of links. WAI-ARIA pattern `listbox`/
  // `navigation` both accept arrow keys — we keep the default Tab flow
  // (one stop per button) AND layer arrow-nav on top so power users can
  // jump around quickly (US-SP42-007 E.46).
  let navRef: HTMLElement | null = $state(null);

  function handleNavKeydown(event: KeyboardEvent) {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp" &&
        event.key !== "Home" && event.key !== "End") return;
    if (!navRef) return;

    const buttons = Array.from(
      navRef.querySelectorAll<HTMLButtonElement>('button[data-nav-item="true"]'),
    );
    if (buttons.length === 0) return;

    const currentIndex = buttons.findIndex((btn) => btn === document.activeElement);
    let nextIndex = currentIndex;
    if (event.key === "ArrowDown") {
      nextIndex = currentIndex < 0 ? 0 : (currentIndex + 1) % buttons.length;
    } else if (event.key === "ArrowUp") {
      nextIndex = currentIndex < 0
        ? buttons.length - 1
        : (currentIndex - 1 + buttons.length) % buttons.length;
    } else if (event.key === "Home") {
      nextIndex = 0;
    } else if (event.key === "End") {
      nextIndex = buttons.length - 1;
    }
    event.preventDefault();
    buttons[nextIndex]?.focus();
  }
</script>

{#snippet sidebarInner(drawer: boolean)}
  <!-- Logo + Collapse toggle -->
  <div
    class="flex items-center gap-2.5 px-4 py-5"
    class:justify-center={isIcon && !drawer}
  >
    <img src="/logo.svg" alt="Apollia" width="32" height="32" class="h-8 w-8 shrink-0" />
    {#if showLabels}
      <span class="text-lg font-semibold text-foreground" data-testid="sidebar-logo">Apollia OS</span>
      {#if drawer}
        <button
          class="ml-auto inline-flex min-h-11 min-w-11 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
          onclick={() => layoutActions.closeDrawer()}
          title={$t("nav.close_sidebar")}
          aria-label={$t("nav.close_sidebar")}
          data-testid="sidebar-drawer-close"
        >
          <X size={18} strokeWidth={1.75} />
        </button>
      {:else if isExpanded}
        <button
          class="ml-auto rounded-md p-1 text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
          onclick={() => layoutActions.toggleSidebar()}
          title={$t("nav.collapse_sidebar")}
          data-testid="sidebar-collapse-btn"
        >
          <PanelLeftClose size={16} strokeWidth={1.75} />
        </button>
      {/if}
    {/if}
  </div>

  {#if isIcon && !drawer}
    <!-- Expand toggle is only meaningful at lg+ ; hidden at md (forced icon). -->
    <button
      class="mx-auto mb-2 rounded-md p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground transition-colors lg:inline-flex hidden"
      onclick={() => layoutActions.toggleSidebar()}
      title={$t("nav.expand_sidebar")}
      data-testid="sidebar-expand-btn"
    >
      <PanelLeftOpen size={16} strokeWidth={1.75} />
    </button>
  {/if}

  <Separator />

  <!-- Navigation -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions
       The keydown handler only captures arrow/Home/End to move
       focus between child buttons (US-SP42-007 E.46). It does not
       make <nav> itself interactive. -->
  <nav
    bind:this={navRef}
    class="flex flex-1 flex-col overflow-y-auto p-2"
    class:p-3={showLabels}
    aria-label={$t("nav.sidebar_label")}
    onkeydown={handleNavKeydown}
    data-testid="sidebar-nav"
  >
    {#if isOperator}
      {#each operatorNav as item}
        {@const isActive = $currentRoute === item.route}
        <button
          class="list-item-spring relative flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium {isActive
            ? 'bg-primary/10 text-primary'
            : 'text-muted-foreground hover:bg-primary/[0.04] hover:text-foreground'}"
          class:justify-center={!showLabels}
          class:px-2={!showLabels}
          data-nav-item="true"
          aria-current={isActive ? "page" : undefined}
          aria-label={showLabels ? undefined : $t(item.labelKey)}
          data-testid="nav-{item.route}"
          onclick={() => navigate(item.route)}
          title={showLabels ? undefined : $t(item.labelKey)}
        >
          <item.icon size={18} strokeWidth={1.75} class="shrink-0" />
          {#if !showLabels && item.route === "tasks" && runningCount > 0}
            <span class="absolute -top-0.5 right-0.5 flex h-2 w-2" data-testid="running-tasks-nav-badge">
              <span class="absolute inline-flex h-full w-full animate-ping rounded-full bg-primary opacity-60"></span>
              <span class="relative inline-flex h-2 w-2 rounded-full bg-primary"></span>
            </span>
          {/if}
          {#if showLabels}
            <span>{$t(item.labelKey)}</span>
            {#if item.route === "inbox" && ($pendingCount + $pendingChatApprovalCount) > 0}
              <Badge variant="destructive" class="ml-auto text-[10px] px-1.5 py-0" data-testid="approvals-badge"
                >{$pendingCount + $pendingChatApprovalCount}</Badge
              >
            {/if}
            {#if item.route === "chat" && $activeChatCount > 0}
              <Badge variant="secondary" class="ml-auto text-[10px] px-1.5 py-0" data-testid="chat-badge"
                >{$activeChatCount}</Badge
              >
            {/if}
          {/if}
        </button>
      {/each}
    {:else}
      {#each builderNavGroups as group, groupIndex}
        {#if showLabels}
          <span
            class="mb-1 mt-3 px-3 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground/60"
            data-testid="nav-group-{group.labelKey.split('.')[1]}"
            >{$t(group.labelKey)}</span
          >
        {:else if groupIndex > 0}
          <Separator class="my-1.5" />
        {/if}
        {#each group.items as item}
          {@const isActive = $currentRoute === item.route}
          <button
            class="list-item-spring relative flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium {isActive
              ? 'bg-primary/10 text-primary'
              : 'text-muted-foreground hover:bg-primary/[0.04] hover:text-foreground'}"
            class:justify-center={!showLabels}
            class:px-2={!showLabels}
            data-nav-item="true"
            aria-current={isActive ? "page" : undefined}
            aria-label={showLabels ? undefined : $t(item.labelKey)}
            data-testid="nav-{item.route}"
            onclick={() => navigate(item.route)}
            title={showLabels ? undefined : $t(item.labelKey)}
          >
            <item.icon size={18} strokeWidth={1.75} class="shrink-0" />
            {#if !showLabels && item.route === "tasks" && runningCount > 0}
              <span class="absolute -top-0.5 right-0.5 flex h-2 w-2" data-testid="running-tasks-nav-badge">
                <span class="absolute inline-flex h-full w-full animate-ping rounded-full bg-primary opacity-60"></span>
                <span class="relative inline-flex h-2 w-2 rounded-full bg-primary"></span>
              </span>
            {/if}
            {#if showLabels}
              <span>{$t(item.labelKey)}</span>
              {#if item.route === "inbox" && ($pendingCount + $pendingChatApprovalCount) > 0}
                <Badge variant="destructive" class="ml-auto text-[10px] px-1.5 py-0" data-testid="approvals-badge"
                  >{$pendingCount + $pendingChatApprovalCount}</Badge
                >
              {/if}
              {#if item.route === "chat" && $activeChatCount > 0}
                <Badge variant="secondary" class="ml-auto text-[10px] px-1.5 py-0" data-testid="chat-badge"
                  >{$activeChatCount}</Badge
                >
              {/if}
            {/if}
          </button>
        {/each}
        {#if showLabels && groupIndex < builderNavGroups.length - 1}
          <Separator class="my-2" />
        {/if}
      {/each}
    {/if}

    <!-- Apollia Guide — dedicated meta-chat entry (US-SP42-057).
         Deep-links to the chat surface in "guide" mode (warm-tinted shell).
         Query is preserved in window.location.search so Chat.svelte can
         pick it up on mount and open the pinned apollia-guide session. -->
    <button
      class="list-item-spring relative mt-1 flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium text-muted-foreground hover:bg-secondary/10 hover:text-foreground"
      class:justify-center={!showLabels}
      class:px-2={!showLabels}
      data-nav-item="true"
      data-testid="nav-apollia-guide"
      onclick={openApolliaGuide}
      title={showLabels ? undefined : $t("nav.apollia_guide_tooltip")}
      aria-label={showLabels ? undefined : $t("nav.apollia_guide_tooltip")}
    >
      <Sparkles size={18} strokeWidth={1.75} class="shrink-0 text-secondary" />
      {#if showLabels}
        <span>{$t("nav.apollia_guide")}</span>
      {/if}
    </button>

    <div class="flex-1"></div>

    <Separator class="my-2" />

    <!-- Companion toggle (post-onboarding only) -->
    {#if $onboardingStore.completed || $onboardingStore.phase === "done"}
      <CompanionToggle collapsed={!showLabels} />
    {/if}

    <!-- Settings -->
    <button
      class="list-item-spring flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium {$currentRoute === settingsItem.route
        ? 'bg-primary/10 text-primary'
        : 'text-muted-foreground hover:bg-primary/[0.04] hover:text-foreground'}"
      class:justify-center={!showLabels}
      class:px-2={!showLabels}
      data-nav-item="true"
      aria-current={$currentRoute === settingsItem.route ? "page" : undefined}
      aria-label={showLabels ? undefined : $t(settingsItem.labelKey)}
      data-testid="nav-{settingsItem.route}"
      onclick={() => navigate(settingsItem.route)}
      title={showLabels ? undefined : $t(settingsItem.labelKey)}
    >
      <settingsItem.icon size={18} strokeWidth={1.75} class="shrink-0" />
      {#if showLabels}
        <span>{$t(settingsItem.labelKey)}</span>
      {/if}
    </button>

    <!-- Running tasks indicator -->
    {#if runningCount > 0}
      <button
        class="flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium text-primary/80 transition-colors hover:bg-primary/[0.06]"
        class:justify-center={!showLabels}
        class:px-2={!showLabels}
        data-testid="running-tasks-indicator"
        onclick={() => navigate("tasks")}
        title={showLabels ? undefined : `${runningCount} running`}
      >
        <span class="relative flex h-2 w-2 shrink-0">
          <span class="absolute inline-flex h-full w-full animate-ping rounded-full bg-primary opacity-60"></span>
          <span class="relative inline-flex h-2 w-2 rounded-full bg-primary"></span>
        </span>
        {#if showLabels}
          <span class="text-xs">
            {runningCount === 1 ? $t('nav.running_task_one') : $t('nav.running_tasks', { values: { count: runningCount } })}
          </span>
        {/if}
      </button>
    {/if}

    <!-- Onboarding badge -->
    {#if $showOnboardingBadge}
      <button
        class="flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium text-secondary transition-colors hover:bg-secondary/10"
        class:justify-center={!showLabels}
        class:px-2={!showLabels}
        data-testid="onboarding-badge"
        onclick={() => navigate("onboarding")}
        title={showLabels ? undefined : $t("onboarding_welcome.badge")}
      >
        <Sparkles size={18} strokeWidth={1.75} class="shrink-0" />
        {#if showLabels}
          <span>{$t("onboarding_welcome.badge")}</span>
          <Badge variant="secondary" class="ml-auto text-[10px] px-1.5 py-0">!</Badge>
        {/if}
      </button>
    {/if}

    <!-- Mode toggle -->
    {#if showLabels}
      <button
        class="mt-1 flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-primary/[0.04] hover:text-foreground"
        data-testid="mode-toggle"
        onclick={toggleMode}
        title={isOperator ? $t("nav.switch_to_builder") : $t("nav.switch_to_operator")}
      >
        <Layers size={18} strokeWidth={1.75} />
        <span class="text-xs">{isOperator ? $t("nav.switch_to_builder") : $t("nav.switch_to_operator")}</span>
      </button>
    {:else}
      <button
        class="mt-1 flex w-full items-center justify-center rounded-md px-2 py-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-primary/[0.04] hover:text-foreground"
        data-testid="mode-toggle"
        onclick={toggleMode}
        title={isOperator ? $t("nav.switch_to_builder") : $t("nav.switch_to_operator")}
      >
        <Layers size={18} strokeWidth={1.75} />
      </button>
    {/if}
  </nav>

  <Separator />

  <!-- Connection indicator -->
  <div
    class="flex items-center gap-2 px-4 py-3"
    class:justify-center={!showLabels}
    class:px-2={!showLabels}
    data-testid="connection-status"
    data-status={$connectionStatus}
  >
    {#if $connectionStatus === "connected"}
      <span class="h-2 w-2 shrink-0 rounded-full bg-success" data-testid="connection-dot"></span>
    {:else if $connectionStatus === "reconnecting"}
      <span class="h-2 w-2 shrink-0 animate-pulse rounded-full bg-warning" data-testid="connection-dot"></span>
    {:else}
      <span class="h-2 w-2 shrink-0 rounded-full bg-destructive" data-testid="connection-dot"></span>
    {/if}
    {#if showLabels}
      <span class="text-xs text-muted-foreground" data-testid="connection-label"
        >{$t(CONNECTION_KEYS[$connectionStatus] ?? 'common.unknown')}</span
      >
    {/if}
  </div>
{/snippet}

{#if isDrawer}
  <!-- Drawer : overlay render under md — hidden from the layout flow until opened. -->
  {#if $drawerOpen}
    <!-- Backdrop -->
    <div
      class="fixed inset-0 z-40 bg-black/30 backdrop-blur-sm"
      role="button"
      tabindex="-1"
      aria-label={$t("nav.close_sidebar")}
      onclick={() => layoutActions.closeDrawer()}
      onkeydown={handleKeydown}
      transition:fade={{ duration: 180 }}
      data-testid="sidebar-drawer-backdrop"
    ></div>
    <!-- Drawer panel (div + role="dialog" to satisfy a11y — `aside` refuses interactive roles). -->
    <div
      bind:this={drawerRef}
      class="fixed inset-y-0 left-0 z-50 flex w-64 max-w-[85vw] flex-col glass-panel border-r glass-border shadow-xl"
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-label={$t("nav.sidebar_label")}
      onkeydown={handleKeydown}
      transition:fly={{ x: -280, duration: 220, easing: cubicOut }}
      data-testid="sidebar"
      data-state="drawer"
    >
      {@render sidebarInner(true)}
    </div>
  {/if}
{:else}
  <aside
    class="flex h-screen flex-col glass-panel border-r glass-border transition-[width] duration-200 ease-apple"
    class:w-48={isExpanded}
    class:lg:w-60={isExpanded}
    class:w-14={isIcon}
    class:lg:w-16={isIcon}
    aria-label={$t("nav.sidebar_label")}
    data-testid="sidebar"
    data-state={currentState}
  >
    {@render sidebarInner(false)}
  </aside>
{/if}
