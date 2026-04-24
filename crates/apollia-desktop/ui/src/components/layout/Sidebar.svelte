<script lang="ts">
  /**
   * Sidebar — Apollia OS main navigation (US-SP42-079 refonte).
   *
   * Structure:
   *   [Logo + ModeChip]      ← identity + mode switch
   *   [NavGrouped scrollable] ← semantic groups, overflow-y in the middle
   *   [Bottom actions fixed]  ← settings / running tasks / onboarding
   *
   * Removed in this revision:
   *   - Connection status block  → relocated to Topbar (US-SP42-077)
   *   - Companion toggle         → relocated to Topbar (US-SP42-077)
   *   - Inline search            → replaced by Cmd+K (US-SP42-078)
   */
  import { fade, fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { t } from "svelte-i18n";
  import { currentRoute, navigateTo, type Route } from "$lib/stores/navigation";
  import { sidebarState, drawerOpen, layoutActions } from "$lib/stores/layout";
  import { pendingCount } from "$lib/stores/hitl";
  import { activeChatCount, pendingChatApprovalCount, openNewChatRequested } from "$lib/stores/chat";
  import { uiMode } from "$lib/stores/mode";
  import { Badge } from "$lib/components/ui/badge";
  import { Separator } from "$lib/components/ui/separator";
  import { showOnboardingBadge } from "$lib/stores/onboarding";
  import { runningTasks, tasksRunningCount } from "$lib/stores/tasks";
  import {
    Settings,
    PanelLeftClose,
    PanelLeftOpen,
    Plus,
    X,
  } from "lucide-svelte";
  import { operatorGroups, type NavGroup } from "$lib/navigation/operatorNav";
  import { builderGroups } from "$lib/navigation/builderNav";
  import SidebarLink from "./SidebarLink.svelte";
  import SidebarBadge from "./SidebarBadge.svelte";
  import SidebarGroup from "./SidebarGroup.svelte";
  import ModeChip from "./ModeChip.svelte";
  import CompanionToggle from "../companion/CompanionToggle.svelte";

  // ── Nav data — memoised per mode so an unrelated store change does not
  //    rebuild the groups array. ────────────────────────────────────────────
  const groups: NavGroup[] = $derived(
    $uiMode === "operator" ? operatorGroups : builderGroups,
  );

  const settingsRoute: Route = "settings";

  function navigate(route: Route) {
    navigateTo(route);
    layoutActions.closeDrawer();
  }

  function startNewChat() {
    navigate("chat");
    openNewChatRequested.set(Date.now());
  }

  const isOperator = $derived($uiMode === "operator");
  const currentState = $derived($sidebarState);
  const isDrawer = $derived(currentState === "drawer");
  const isIcon = $derived(currentState === "icon");
  const isExpanded = $derived(currentState === "expanded");
  const showLabels = $derived(isExpanded || (isDrawer && $drawerOpen));
  const collapsed = $derived(!showLabels);
  const runningCount = $derived($runningTasks.length);
  const inFlightCount = $derived($tasksRunningCount);
  const approvalsCount = $derived($pendingCount + $pendingChatApprovalCount);

  // Pulse when the in-flight count changes — short-lived class.
  let pulseKey = $state(0);
  let lastInFlight = 0;
  $effect(() => {
    if (inFlightCount !== lastInFlight) {
      lastInFlight = inFlightCount;
      pulseKey += 1;
      const n = pulseKey;
      setTimeout(() => {
        if (n === pulseKey) pulseKey = 0;
      }, 400);
    }
  });

  // ── A11y : focus trap + autofocus + route-change auto-close ────────────────
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

  $effect(() => {
    if (!(isDrawer && $drawerOpen)) return;
    const original = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = original;
    };
  });

  // ── Arrow-key navigation across nav buttons (E.46) ─────────────────────────
  let navRef: HTMLElement | null = $state(null);

  function handleNavKeydown(event: KeyboardEvent) {
    if (
      event.key !== "ArrowDown" &&
      event.key !== "ArrowUp" &&
      event.key !== "Home" &&
      event.key !== "End"
    )
      return;
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
      nextIndex =
        currentIndex < 0
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

  // Resolve the urgency tone for the approvals badge. Over 30 min → urgent.
  // We do not track age here (would require extra store wiring); default
  // is `urgent` since any pending approval blocks an agent.
  const approvalsTone: "urgent" | "attention" = $derived(
    approvalsCount > 0 ? "urgent" : "attention",
  );
</script>

{#snippet sidebarInner(drawer: boolean)}
  <!-- Header : logo + mode chip -->
  <div
    class="flex items-center gap-2 px-4 py-4"
    class:justify-center={collapsed && !drawer}
    class:flex-col={collapsed && !drawer}
  >
    <div class="flex items-center gap-2">
      <img
        src="/logo.svg"
        alt="Apollia"
        width="28"
        height="28"
        class="h-7 w-7 shrink-0"
      />
      {#if showLabels}
        <span class="text-base font-semibold text-foreground" data-testid="sidebar-logo"
          >Apollia OS</span
        >
      {/if}
    </div>
    {#if showLabels}
      <div class="ml-auto flex items-center gap-1">
        <ModeChip {collapsed} />
        {#if drawer}
          <button
            class="inline-flex min-h-9 min-w-9 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
            onclick={() => layoutActions.closeDrawer()}
            title={$t("nav.close_sidebar")}
            aria-label={$t("nav.close_sidebar")}
            data-testid="sidebar-drawer-close"
          >
            <X size={18} strokeWidth={1.75} />
          </button>
        {:else if isExpanded}
          <button
            class="rounded-md p-1 text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
            onclick={() => layoutActions.toggleSidebar()}
            title={$t("nav.collapse_sidebar")}
            data-testid="sidebar-collapse-btn"
          >
            <PanelLeftClose size={16} strokeWidth={1.75} />
          </button>
        {/if}
      </div>
    {:else}
      <ModeChip {collapsed} />
    {/if}
  </div>

  {#if collapsed && !drawer}
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

  <!-- Scrollable nav region -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <nav
    bind:this={navRef}
    class="sidebar-scroll flex flex-1 flex-col gap-0.5 overflow-y-auto p-2"
    class:p-3={showLabels}
    aria-label={$t("nav.sidebar_label")}
    onkeydown={handleNavKeydown}
    data-testid="sidebar-nav"
  >
    {#if isOperator}
      <button
        class="list-item-spring mb-1 flex w-full items-center gap-2 rounded-md bg-primary px-3 py-2 text-sm font-medium text-primary-foreground shadow-sm hover:bg-primary/90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/60"
        class:justify-center={collapsed}
        class:px-2={collapsed}
        data-testid="sidebar-new-chat-btn"
        onclick={startNewChat}
        title={$t("chat.new_chat")}
        aria-label={$t("chat.new_chat")}
      >
        <Plus size={16} strokeWidth={2} class="shrink-0" />
        {#if showLabels}
          <span>{$t("chat.new_chat")}</span>
        {/if}
      </button>
    {/if}

    {#each groups as group, groupIndex (group.labelKey)}
      <SidebarGroup
        label={$t(group.labelKey)}
        {collapsed}
        testId={`nav-group-${group.labelKey.split(".").pop()}`}
      >
        {#each group.items as item (item.route)}
          {@const isActive = $currentRoute === item.route}
          <SidebarLink
            icon={item.icon}
            label={$t(item.labelKey)}
            active={isActive}
            {collapsed}
            testId={`nav-${item.route}`}
            onSelect={() => navigate(item.route)}
          >
            {#snippet indicator()}
              {#if item.route === "tasks" && runningCount > 0}
                <span
                  class="absolute -top-0.5 right-0.5 flex h-2 w-2"
                  data-testid="running-tasks-nav-badge"
                >
                  <span
                    class="absolute inline-flex h-full w-full animate-ping rounded-full bg-primary opacity-60"
                  ></span>
                  <span class="relative inline-flex h-2 w-2 rounded-full bg-primary"></span>
                </span>
              {/if}
            {/snippet}
            {#snippet badge()}
              {#if item.route === "inbox" && approvalsCount > 0}
                <SidebarBadge
                  count={approvalsCount}
                  tone={approvalsTone}
                  testId="approvals-badge"
                  ariaLabel={$t("sidebar.badge.approvals", {
                    values: { count: approvalsCount },
                  })}
                />
              {:else if item.route === "chat" && $activeChatCount > 0}
                <SidebarBadge
                  count={$activeChatCount}
                  tone="info"
                  testId="chat-badge"
                  ariaLabel={$t("sidebar.badge.chats", {
                    values: { count: $activeChatCount },
                  })}
                />
              {:else if item.route === "tasks" && inFlightCount > 0}
                <SidebarBadge
                  count={inFlightCount}
                  tone="info"
                  pulse={pulseKey > 0}
                  testId="tasks-running-badge"
                  ariaLabel={$t("sidebar.badge.tasks_running", {
                    values: { count: inFlightCount },
                  })}
                />
              {/if}
            {/snippet}
          </SidebarLink>
        {/each}
      </SidebarGroup>
      {#if collapsed && groupIndex < groups.length - 1}
        <Separator class="my-1.5" />
      {/if}
    {/each}

  </nav>

  <!-- Fixed bottom actions -->
  <div
    class="mt-auto flex flex-shrink-0 flex-col gap-0.5 border-t border-border/60 p-2"
    class:p-3={showLabels}
    data-testid="sidebar-bottom-actions"
  >
    <CompanionToggle {collapsed} />
    <SidebarLink
      icon={Settings}
      label={$t("nav.settings")}
      active={$currentRoute === settingsRoute}
      {collapsed}
      testId="nav-settings"
      onSelect={() => navigate(settingsRoute)}
    />

    {#if runningCount > 0}
      <button
        class="flex w-full items-center gap-3 rounded-md px-3 py-2 text-xs font-medium text-primary/80 transition-colors hover:bg-primary/[0.06]"
        class:justify-center={collapsed}
        class:px-2={collapsed}
        data-testid="running-tasks-indicator"
        onclick={() => navigate("tasks")}
        title={collapsed ? `${runningCount} running` : undefined}
      >
        <span class="relative flex h-2 w-2 shrink-0">
          <span
            class="absolute inline-flex h-full w-full animate-ping rounded-full bg-primary opacity-60"
          ></span>
          <span class="relative inline-flex h-2 w-2 rounded-full bg-primary"></span>
        </span>
        {#if showLabels}
          <span>
            {runningCount === 1
              ? $t("nav.running_task_one")
              : $t("nav.running_tasks", { values: { count: runningCount } })}
          </span>
        {/if}
      </button>
    {/if}

    {#if $showOnboardingBadge}
      <button
        class="flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium text-secondary transition-colors hover:bg-secondary/10"
        class:justify-center={collapsed}
        class:px-2={collapsed}
        data-testid="onboarding-badge"
        onclick={() => navigate("onboarding")}
        title={collapsed ? $t("onboarding_welcome.badge") : undefined}
      >
        <Sparkles size={18} strokeWidth={1.75} class="shrink-0" />
        {#if showLabels}
          <span>{$t("onboarding_welcome.badge")}</span>
          <Badge variant="secondary" class="ml-auto text-[10px] px-1.5 py-0">!</Badge>
        {/if}
      </button>
    {/if}
  </div>
{/snippet}

{#if isDrawer}
  {#if $drawerOpen}
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
    class="flex h-full flex-col glass-panel border-r glass-border transition-[width] duration-200 ease-apple"
    class:w-48={isExpanded}
    class:lg:w-60={isExpanded}
    class:w-14={isIcon}
    class:lg:w-16={isIcon}
    role="navigation"
    aria-label={$t("nav.sidebar_label")}
    data-testid="sidebar"
    data-state={currentState}
  >
    {@render sidebarInner(false)}
  </aside>
{/if}

<style>
  /* Hide the scrollbar until the user interacts with the nav region. */
  .sidebar-scroll {
    scrollbar-width: thin;
    scrollbar-color: transparent transparent;
    transition: scrollbar-color 120ms ease;
  }
  .sidebar-scroll:hover,
  .sidebar-scroll:focus-within {
    scrollbar-color: hsl(var(--border)) transparent;
  }
  .sidebar-scroll::-webkit-scrollbar {
    width: 6px;
  }
  .sidebar-scroll::-webkit-scrollbar-thumb {
    background: transparent;
    border-radius: 3px;
  }
  .sidebar-scroll:hover::-webkit-scrollbar-thumb,
  .sidebar-scroll:focus-within::-webkit-scrollbar-thumb {
    background: hsl(var(--border));
  }
</style>
