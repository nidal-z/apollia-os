<script lang="ts">
  /**
   * Topbar V4 — glass command center.
   *
   *   [Mobile menu] [Logo box] [Breadcrumb] ··· [Command bar] ··· [ModeChip] [ConnectionChip] [Companion] [Bell] [UserMenu]
   */
  import { t } from "svelte-i18n";
  import { Menu, Search, Bell } from "lucide-svelte";
  import { navigateTo } from "$lib/stores/navigation";
  import { sidebarState, layoutActions } from "$lib/stores/layout";
  import { uiMode } from "$lib/stores/mode";
  import { onboardingStore } from "$lib/stores/onboarding";
  import { homeRouteFor } from "$lib/navigation/routeMeta";
  import Breadcrumb from "./Breadcrumb.svelte";
  import ConnectionChip from "./ConnectionChip.svelte";
  import UserMenu from "./UserMenu.svelte";
  import ModeChip from "./ModeChip.svelte";
  import CompanionToggle from "../companion/CompanionToggle.svelte";

  function goHome() {
    navigateTo(homeRouteFor($uiMode));
  }

  function openSearch() {
    // Trigger command palette via keyboard event (same as Cmd+K)
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  }
</script>

<header
  class="topbar sticky top-0 z-40 flex h-[52px] items-center gap-3 border-b border-border bg-muted/80 px-4 backdrop-blur-xl"
  style="border-bottom-width: 0.5px;"
  data-testid="topbar"
>
  <!-- Left: identity -->
  <div class="flex min-w-0 items-center gap-2.5">
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

    <!-- Logo/home button -->
    <button
      type="button"
      class="inline-flex h-9 items-center gap-2 rounded-md px-1.5 text-[13px] font-semibold text-foreground transition-colors hover:bg-surface-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      onclick={goHome}
      aria-label={$t("topbar.home_aria")}
      data-testid="topbar-logo"
    >
      <span
        class="flex h-[28px] w-[28px] flex-shrink-0 items-center justify-center rounded-[7px] border bg-surface-1 text-foreground shadow-elev-0"
        style="border-width: 0.5px;"
      >
        <img src="/logo.svg" alt="" width="16" height="16" class="h-4 w-4" />
      </span>
      <span class="hidden md:inline" style="letter-spacing: -0.1px;">Apollia OS</span>
    </button>

    <!-- Breadcrumb chevron + current page -->
    <Breadcrumb />
  </div>

  <!-- Center: command bar -->
  <div class="flex flex-1 justify-center">
    <button
      type="button"
      class="command-bar flex h-[30px] w-full max-w-[360px] items-center gap-2 rounded-md border bg-surface-1 px-2.5 text-left text-muted-foreground shadow-elev-0 transition-colors hover:bg-surface-1/80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      style="border-width: 0.5px; font-size: 11.5px; cursor: text;"
      onclick={openSearch}
      aria-label={$t("topbar.search_aria") || "Chercher ou lancer une commande"}
      data-testid="topbar-search"
    >
      <Search size={12} strokeWidth={1.75} class="flex-shrink-0" />
      <span class="flex-1 truncate">Chercher, lancer une commande…</span>
      <kbd class="hidden items-center gap-0.5 text-[10px] sm:flex">
        <span class="rounded border bg-muted px-1 py-px" style="border-width: 0.5px;">⌘</span>
        <span class="rounded border bg-muted px-1 py-px" style="border-width: 0.5px;">K</span>
      </kbd>
    </button>
  </div>

  <!-- Right: controls -->
  <div class="flex flex-shrink-0 items-center gap-1.5">
    <ModeChip collapsed={false} />
    <ConnectionChip />
    {#if $onboardingStore.completed || $onboardingStore.phase === "done"}
      <div class="hidden sm:block">
        <CompanionToggle collapsed />
      </div>
    {/if}
    <button
      type="button"
      class="flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-md border bg-surface-1 text-muted-foreground shadow-elev-0 transition-colors hover:bg-surface-2 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      style="border-width: 0.5px;"
      aria-label="Notifications"
    >
      <Bell size={13} strokeWidth={1.5} />
    </button>
    <UserMenu />
  </div>
</header>

