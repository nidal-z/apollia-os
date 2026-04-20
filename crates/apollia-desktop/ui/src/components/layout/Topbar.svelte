<script lang="ts">
  /**
   * Persistent application topbar (US-SP42-077).
   *
   *   [Logo] [Back/Forward] [Breadcrumb] ··· [Search] [Connection] [Companion] [User]
   *
   * Sidebar drawer toggle is surfaced on the leftmost slot when the
   * sidebar is in `drawer` mode (mobile breakpoint).
   */
  import { t } from "svelte-i18n";
  import { Menu } from "lucide-svelte";
  import { navigateTo } from "$lib/stores/navigation";
  import { sidebarState, layoutActions } from "$lib/stores/layout";
  import { uiMode } from "$lib/stores/mode";
  import { onboardingStore } from "$lib/stores/onboarding";
  import { homeRouteFor } from "$lib/navigation/routeMeta";
  import BackForwardControls from "./BackForwardControls.svelte";
  import Breadcrumb from "./Breadcrumb.svelte";
  import SearchTrigger from "./SearchTrigger.svelte";
  import ConnectionChip from "./ConnectionChip.svelte";
  import UserMenu from "./UserMenu.svelte";
  import CompanionToggle from "../companion/CompanionToggle.svelte";

  function goHome() {
    navigateTo(homeRouteFor($uiMode));
  }
</script>

<header
  class="sticky top-0 z-40 flex h-12 md:h-14 items-center gap-2 border-b border-border bg-background/80 px-4 sm:px-6 md:px-8 backdrop-blur-md"
  data-testid="topbar"
>
  {#if $sidebarState === "drawer"}
    <button
      type="button"
      class="inline-flex h-10 w-10 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
      onclick={() => layoutActions.openDrawer()}
      aria-label={$t('nav.open_sidebar')}
      aria-haspopup="dialog"
      data-testid="topbar-sidebar-toggle"
    >
      <Menu size={18} strokeWidth={1.75} />
    </button>
  {/if}

  <button
    type="button"
    class="inline-flex h-10 items-center gap-2 rounded-md px-2 text-sm font-semibold text-foreground transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
    onclick={goHome}
    aria-label={$t('topbar.home_aria')}
    data-testid="topbar-logo"
  >
    <img src="/logo.svg" alt="" width="24" height="24" class="h-6 w-6 shrink-0" />
    <span class="hidden md:inline">Apollia OS</span>
  </button>

  <BackForwardControls />

  <Breadcrumb />

  <div class="flex items-center gap-2">
    <SearchTrigger />
    <ConnectionChip />
    {#if $onboardingStore.completed || $onboardingStore.phase === 'done'}
      <div class="hidden sm:block">
        <CompanionToggle collapsed />
      </div>
    {/if}
    <UserMenu />
  </div>
</header>
