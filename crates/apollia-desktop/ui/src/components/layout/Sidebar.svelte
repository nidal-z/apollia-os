<script lang="ts">
  import { fade, fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { t } from "svelte-i18n";
  import { currentRoute, navigateTo, type Route } from "$lib/stores/navigation";
  import { sidebarState, drawerOpen, layoutActions } from "$lib/stores/layout";
  import { pendingCount } from "$lib/stores/hitl";
  import { activeChatCount, pendingChatApprovalCount, openNewChatRequested } from "$lib/stores/chat";
  import { runningTasks, tasksRunningCount } from "$lib/stores/tasks";
  import { uiMode } from "$lib/stores/mode";
  import {
    LayoutDashboard, MessageSquare, Bot, FolderKanban, ListChecks, ShieldCheck, Plug,
    Settings, X, Plus, Repeat,
    Brain, Database, Activity, Mic, Bell,
  } from "lucide-svelte";

  const PRIMARY_NAV = [
    { route: "dashboard"     as Route, Icon: LayoutDashboard, label: "Accueil",         badge: "none"      },
    { route: "chat"          as Route, Icon: MessageSquare,   label: "Chat",            badge: "chat"      },
    { route: "agents"        as Route, Icon: Bot,             label: "Assistants",      badge: "none"      },
    { route: "projects"      as Route, Icon: FolderKanban,    label: "Projets",         badge: "none"      },
    { route: "tasks"         as Route, Icon: ListChecks,      label: "Mon travail",     badge: "tasks"     },
    { route: "automations"   as Route, Icon: Repeat,          label: "Automatisations", badge: "none"      },
    { route: "inbox"         as Route, Icon: ShieldCheck,     label: "Approbations",    badge: "approvals" },
    { route: "memory"        as Route, Icon: Database,        label: "Mémoire",         badge: "none"      },
    { route: "observability" as Route, Icon: Activity,        label: "Observabilité",   badge: "none"      },
  ];

  // Cluster Builder — vues purement techniques (apparaît uniquement en mode Builder).
  const BUILDER_NAV = [
    { route: "llm"            as Route, Icon: Brain, label: "Modèles LLM" },
    { route: "transcriptions" as Route, Icon: Mic,   label: "Transcriptions" },
    { route: "notifications"  as Route, Icon: Bell,  label: "Notifications" },
  ];

  const SECONDARY_NAV = [
    { route: "integrations" as Route, Icon: Plug,     label: "Connexions" },
    { route: "settings"     as Route, Icon: Settings, label: "Paramètres" },
  ];

  const isBuilder = $derived($uiMode === "builder");


  const isDrawer = $derived($sidebarState === "drawer");

  const approvalsCount = $derived($pendingCount + $pendingChatApprovalCount);
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
        <button
          class="flex w-full items-center gap-2 rounded-lg bg-primary px-3 py-2 text-sm font-medium text-primary-foreground shadow-primary-sm hover:bg-primary/90"
          onclick={startNewChat}
        >
          <Plus size={15} strokeWidth={2} class="shrink-0" />
          <span>{$t("chat.new_chat")}</span>
        </button>
      </div>

      <nav class="flex flex-1 flex-col gap-0.5 overflow-y-auto p-3" aria-label={$t("nav.sidebar_label")}>
        {#each PRIMARY_NAV as item (item.route)}
          {@const isActive = $currentRoute === item.route}
          {@const badgeCount = getBadgeCount(item.badge)}
          <button
            type="button"
            class="relative flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors {isActive ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-primary/[0.04] hover:text-foreground'}"
            onclick={() => navigate(item.route)}
            aria-current={isActive ? "page" : undefined}
          >
            <item.Icon size={17} strokeWidth={1.5} class="shrink-0" />
            <span class="truncate">{item.label}</span>
            {#if badgeCount > 0}
              <span class="ml-auto min-w-[18px] rounded-full bg-primary px-1.5 py-0.5 text-center text-[10px] font-semibold text-white leading-none">{badgeCount}</span>
            {:else if item.badge === "tasks" && runningCount > 0}
              <span class="ml-auto flex h-2 w-2"><span class="absolute inline-flex h-2 w-2 animate-ping rounded-full bg-primary opacity-60"></span><span class="relative inline-flex h-2 w-2 rounded-full bg-primary"></span></span>
            {/if}
          </button>
        {/each}

        {#if isBuilder}
          <div class="mt-3 mb-1 px-3 font-mono text-[10px] font-semibold uppercase tracking-[1.5px] text-muted-foreground/70" data-testid="sidebar-builder-cluster-label">
            {$t("nav.builder_cluster")}
          </div>
          {#each BUILDER_NAV as item (item.route)}
            {@const isActive = $currentRoute === item.route}
            <button
              type="button"
              class="relative flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors {isActive ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-primary/[0.04] hover:text-foreground'}"
              onclick={() => navigate(item.route)}
              aria-current={isActive ? "page" : undefined}
              data-testid="nav-{item.route}"
            >
              <item.Icon size={17} strokeWidth={1.5} class="shrink-0" />
              <span class="truncate">{item.label}</span>
            </button>
          {/each}
        {/if}
      </nav>

      <div class="border-t border-border/60 p-3 flex flex-col gap-0.5">
        {#each SECONDARY_NAV as item (item.route)}
          {@const isActive = $currentRoute === item.route}
          <button
            type="button"
            class="flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors {isActive ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-muted hover:text-foreground'}"
            onclick={() => navigate(item.route)}
            data-testid="nav-{item.route}"
          >
            <item.Icon size={17} strokeWidth={1.5} class="shrink-0" />
            <span>{item.label}</span>
          </button>
        {/each}
      </div>
    </div>
  {/if}
{:else}
  <!-- V3 64px icon rail — permanent, no expand -->
  <aside
    class="rail flex h-full w-16 flex-shrink-0 flex-col items-center border-r border-border bg-muted py-3 gap-1"
    style="border-width: 0.5px;"
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
      <div class="rail-item group relative mb-1">
        <button
          type="button"
          class="relative flex h-10 w-10 items-center justify-center rounded-[10px] transition-all duration-[120ms] {isActive ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-surface-2 hover:text-foreground'}"
          onclick={() => navigate(item.route)}
          aria-current={isActive ? "page" : undefined}
          aria-label={item.label}
          title={item.label}
          data-nav-item="true"
          data-testid="nav-{item.route}"
        >
          {#if isActive}
            <span class="active-bar absolute rounded-r-[3px] bg-primary" style="left: -14px; top: 10px; bottom: 10px; width: 3px;"></span>
          {/if}
          <item.Icon size={17} strokeWidth={1.5} />
          {#if badgeCount > 0}
            <span class="absolute -right-0.5 -top-0.5 min-w-[14px] rounded-full bg-primary px-1 py-px text-center text-[9px] font-bold text-white leading-none">{badgeCount > 9 ? "9+" : badgeCount}</span>
          {:else if isPulse}
            <span class="absolute right-0.5 top-0.5 flex h-1.5 w-1.5">
              <span class="absolute inline-flex h-full w-full animate-ping rounded-full bg-primary opacity-60"></span>
              <span class="relative inline-flex h-1.5 w-1.5 rounded-full bg-primary"></span>
            </span>
          {/if}
        </button>
        <!-- Tooltip -->
        <span class="tooltip pointer-events-none absolute left-full top-1/2 z-50 ml-2 -translate-y-1/2 whitespace-nowrap rounded-md bg-foreground px-2 py-1 text-[11px] font-medium text-background opacity-0 shadow-elev-3 transition-opacity duration-[120ms] group-hover:opacity-100">
          {item.label}
        </span>
      </div>
    {/each}

    {#if isBuilder}
      <!-- Builder cluster — Inspection -->
      <div class="my-1 h-px w-8 bg-border/60" data-testid="sidebar-builder-separator"></div>
      {#each BUILDER_NAV as item (item.route)}
        {@const isActive = $currentRoute === item.route}
        <div class="rail-item group relative mb-1">
          <button
            type="button"
            class="relative flex h-10 w-10 items-center justify-center rounded-[10px] transition-all duration-[120ms] {isActive ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-surface-2 hover:text-foreground'}"
            onclick={() => navigate(item.route)}
            aria-current={isActive ? "page" : undefined}
            aria-label={item.label}
            title={item.label}
            data-nav-item="true"
            data-testid="nav-{item.route}"
          >
            {#if isActive}
              <span class="active-bar absolute rounded-r-[3px] bg-primary" style="left: -14px; top: 10px; bottom: 10px; width: 3px;"></span>
            {/if}
            <item.Icon size={17} strokeWidth={1.5} />
          </button>
          <span class="tooltip pointer-events-none absolute left-full top-1/2 z-50 ml-2 -translate-y-1/2 whitespace-nowrap rounded-md bg-foreground px-2 py-1 text-[11px] font-medium text-background opacity-0 shadow-elev-3 transition-opacity duration-[120ms] group-hover:opacity-100">
            {item.label}
          </span>
        </div>
      {/each}
    {/if}

    <!-- Spacer -->
    <div class="flex-1"></div>

    <!-- Secondary nav (Connexions, Paramètres) -->
    {#each SECONDARY_NAV as item (item.route)}
      {@const isActive = $currentRoute === item.route}
      <div class="rail-item group relative mb-1">
        <button
          type="button"
          class="relative flex h-10 w-10 items-center justify-center rounded-[10px] transition-all duration-[120ms] {isActive ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-surface-2 hover:text-foreground'}"
          onclick={() => navigate(item.route)}
          aria-current={isActive ? "page" : undefined}
          aria-label={item.label}
          title={item.label}
          data-testid="nav-{item.route}"
        >
          {#if isActive}
            <span class="active-bar absolute rounded-r-[3px] bg-primary" style="left: -14px; top: 10px; bottom: 10px; width: 3px;"></span>
          {/if}
          <item.Icon size={17} strokeWidth={1.5} />
        </button>
        <span class="tooltip pointer-events-none absolute left-full top-1/2 z-50 ml-2 -translate-y-1/2 whitespace-nowrap rounded-md bg-foreground px-2 py-1 text-[11px] font-medium text-background opacity-0 shadow-elev-3 transition-opacity duration-[120ms] group-hover:opacity-100">
          {item.label}
        </span>
      </div>
    {/each}

    <!-- User avatar -->
    <div class="mt-1">
      <span
        class="flex h-7 w-7 items-center justify-center rounded-lg text-[11px] font-semibold text-white shadow-elev-1"
        style="background: linear-gradient(135deg, #c2715a, #9a4f3c);"
        title="Nidal Z"
      >NZ</span>
    </div>
  </aside>
{/if}

<style>
  .rail-item { position: relative; }
  .active-bar { position: absolute; }
</style>
