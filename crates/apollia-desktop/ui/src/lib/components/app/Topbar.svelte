<script lang="ts">
  /**
   * Topbar — V3 Operator cockpit header.
   *
   *   [Mobile menu] [Breadcrumb] ··· [Command bar] ··· [Agents-at-work] [ModeChip] [Activity] [Settings] [UserMenu]
   *
   * The ModeChip is the single deviation from the V3 mockup — kept here per
   * product call so the OP/Builder toggle stays in the cockpit.
   */
  import { t } from "svelte-i18n";
  import { Menu, Search, ShieldCheck, Settings as SettingsIcon } from "lucide-svelte";
  import { navigateTo } from "$lib/stores/navigation";
  import { sidebarState, layoutActions } from "$lib/stores/layout";
  import { runningTasks } from "$lib/stores/tasks";
  import { OperatorBreadcrumb } from "$lib/components/layout";
  import UserMenu from "./UserMenu.svelte";
  import ModeChip from "./ModeChip.svelte";

  function openSearch() {
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  }

  const agentsAtWork = $derived($runningTasks.length);
</script>

<header
  class="topbar sticky top-0 z-40 flex h-[52px] items-center gap-3 border-b border-border bg-muted/80 px-4 backdrop-blur-xl"
  style="border-bottom-width: 0.5px;"
  data-testid="topbar"
>
  <!-- Left: breadcrumb (+ hamburger on drawer breakpoint) -->
  <div class="flex min-w-0 items-center gap-2">
    {#if $sidebarState === "drawer"}
      <button
        type="button"
        class="inline-flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-surface-2 hover:text-foreground"
        onclick={() => layoutActions.openDrawer()}
        aria-label={$t("nav.open_sidebar")}
        aria-haspopup="dialog"
        data-testid="topbar-sidebar-toggle"
      >
        <Menu size={17} strokeWidth={1.5} />
      </button>
    {/if}

    <OperatorBreadcrumb />
  </div>

  <!-- Center: command bar -->
  <div class="flex flex-1 justify-center">
    <button
      type="button"
      class="command-bar flex h-[30px] w-full max-w-[440px] items-center gap-2 rounded-md border bg-surface-1 px-2.5 text-left text-muted-foreground shadow-elev-0 transition-colors hover:bg-surface-1/80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      style="border-width: 0.5px; font-size: 12px; cursor: text;"
      onclick={openSearch}
      aria-label={$t("topbar.search_aria") || "Chercher, lancer un agent…"}
      data-testid="topbar-search"
    >
      <Search size={13} strokeWidth={1.75} class="flex-shrink-0" />
      <span class="flex-1 truncate">Rechercher, lancer un agent…</span>
      <kbd class="hidden items-center gap-0.5 text-[10px] sm:flex">
        <span class="rounded border bg-muted px-1 py-px" style="border-width: 0.5px;">⌘</span>
        <span class="rounded border bg-muted px-1 py-px" style="border-width: 0.5px;">K</span>
      </kbd>
    </button>
  </div>

  <!-- Right: status + controls -->
  <div class="flex flex-shrink-0 items-center gap-1.5">
    {#if agentsAtWork > 0}
      <span
        class="inline-flex items-center gap-1.5 rounded-full border bg-surface-1 px-2.5 py-1 text-muted-foreground"
        style="border-width: 0.5px; font-size: 11px;"
        data-testid="topbar-agents-at-work"
        aria-label="{agentsAtWork} agents au travail"
      >
        <span class="agents-dot relative inline-flex h-1.5 w-1.5 rounded-full" style="background: hsl(var(--success));"></span>
        {agentsAtWork} agents au travail
      </span>
    {/if}

    <!-- Single deviation from V3: ModeChip kept in header -->
    <ModeChip collapsed={false} />

    <button
      type="button"
      class="flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-surface-2 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      onclick={() => navigateTo("inbox")}
      aria-label={$t("nav.inbox")}
      data-testid="topbar-inbox"
      title={$t("nav.inbox")}
    >
      <ShieldCheck size={14} strokeWidth={1.5} />
    </button>

    <button
      type="button"
      class="flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-surface-2 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      onclick={() => navigateTo("settings")}
      aria-label={$t("nav.settings")}
      data-testid="topbar-settings"
      title={$t("nav.settings")}
    >
      <SettingsIcon size={14} strokeWidth={1.5} />
    </button>

    <UserMenu />
  </div>
</header>

<style>
  .agents-dot {
    box-shadow: 0 0 0 0 hsl(var(--success) / 0.4);
    animation: agents-soft-pulse 1.8s ease-in-out infinite;
  }
  @keyframes agents-soft-pulse {
    0%, 100% { box-shadow: 0 0 0 0 hsl(var(--success) / 0.45); opacity: 1; }
    50%      { box-shadow: 0 0 0 4px hsl(var(--success) / 0); opacity: 0.75; }
  }
</style>
