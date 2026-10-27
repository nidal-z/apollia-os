<script lang="ts">
  import { navigateTo } from "$lib/stores/navigation";
  /**
   * Topbar - V3 Operator cockpit header.
   *
   *   [Mobile menu] [Breadcrumb] ··· [Command bar] ··· [Agents-at-work] [Downloads] [Help] [Detail sidebar] [UserMenu]
   */
  import { t } from "svelte-i18n";
  import {
    CircleHelp,
    Menu,
    PanelLeft,
    PanelLeftClose,
    PanelLeftOpen,
    PanelRight,
    PanelRightClose,
    PanelRightOpen,
    Search,
  } from "lucide-svelte";
  import { detailSidebar, guideVisible } from "$lib/stores/detailSidebar";
  import { navPosition } from "$lib/stores/chatAppearance";
  import { onboardingModalOpen } from "$lib/stores/onboarding";
  import { zLayer } from "$lib/design/tokens";
  import { keycapsFor } from "$lib/navigation/shortcuts";
  import { commandPaletteOpen } from "$lib/stores/commandPalette";
  import { sidebarState, sidebarFoldable, layoutActions } from "$lib/stores/layout";
  import { runningTasks } from "$lib/stores/tasks";
  import { OperatorBreadcrumb } from "$lib/components/layout";
  import { StatusDot } from "$lib/components/operator";
  import UserMenu from "./UserMenu.svelte";
  import DownloadsPanel from "./DownloadsPanel.svelte";

  function toggleSearch() {
    // The store the shortcut drives, not a synthetic keystroke: the button
    // used to dispatch a keydown carrying metaKey, the macOS modifier, while
    // the dispatcher reads ctrlKey off a Mac. Clicking the search bar
    // therefore did nothing at all on Windows and on Linux. Measured on
    // 2026-09-08 by the gestural automaton, on Ubuntu.
    //
    // It toggles, like the shortcut it stands for: a second click on the bar
    // closes the palette it opened. Setting it open unconditionally left the
    // bar unable to close anything, which is what the automaton measured next.
    commandPaletteOpen.update((open) => !open);
  }

  const agentsAtWork = $derived($runningTasks.length);

  // The keys that open the palette on this system.
  const paletteKeys = keycapsFor("palette.open");
</script>

<header
  class="topbar sticky top-0 z-drawer flex items-center gap-3 border-b border-border bg-muted/80 px-4 backdrop-blur-xl"
  style:z-index={$onboardingModalOpen ? zLayer.onboardingAbove : undefined}
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
    {:else if $sidebarFoldable}
      <!-- A wide window: the menu folds into its rail and unfolds again, on
           every screen, at the user's choice only. -->
      <button
        type="button"
        class="inline-flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-surface-2 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
        onclick={() => layoutActions.toggleSidebar()}
        aria-label={$sidebarState === "expanded" ? $t("nav.fold_sidebar") : $t("nav.unfold_sidebar")}
        aria-expanded={$sidebarState === "expanded"}
        title={$sidebarState === "expanded" ? $t("nav.fold_sidebar") : $t("nav.unfold_sidebar")}
        data-testid="topbar-sidebar-fold"
      >
        {#if $navPosition === "right"}
          {#if $sidebarState === "expanded"}<PanelRightClose size={16} strokeWidth={1.75} />{:else}<PanelRightOpen size={16} strokeWidth={1.75} />{/if}
        {:else if $sidebarState === "expanded"}
          <PanelLeftClose size={16} strokeWidth={1.75} />
        {:else}
          <PanelLeftOpen size={16} strokeWidth={1.75} />
        {/if}
      </button>
    {/if}

    <OperatorBreadcrumb />
  </div>

  <!-- Center: command bar -->
  <div class="flex flex-1 justify-center">
    <button
      type="button"
      class="command-bar v4-hair flex w-full cursor-text items-center gap-2 rounded-md border bg-surface-1 px-2.5 text-left text-body-xs text-muted-foreground shadow-elev-0 transition-colors hover:bg-surface-1/80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      onclick={toggleSearch}
      aria-label={$t("topbar.search_aria")}
      data-testid="topbar-search"
    >
      <Search size={13} strokeWidth={1.75} class="flex-shrink-0" />
      <span class="flex-1 truncate">{$t("topbar.search.placeholder")}</span>
      <kbd class="hidden items-center gap-0.5 text-overline sm:flex" data-testid="topbar-search-shortcut">
        {#each paletteKeys as key (key)}
          <span class="v4-hair rounded border bg-muted px-1 py-px">{key}</span>
        {/each}
      </kbd>
    </button>
  </div>

  <!-- Right: status + controls -->
  <div class="flex flex-shrink-0 items-center gap-1.5">
    {#if agentsAtWork > 0}
      <!-- The work going on, one click away: the batches, the running ones
           first. -->
      <button
        type="button"
        class="v4-hair inline-flex items-center gap-1.5 rounded-full leading-none border bg-surface-1 px-2.5 py-1 text-caption text-muted-foreground transition-colors hover:bg-surface-2 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
        data-testid="topbar-agents-at-work"
        aria-label={$t("topbar.agents_at_work", { values: { count: agentsAtWork } })}
        title={$t("topbar.agents_at_work_open")}
        onclick={() => navigateTo("batches")}
      >
        <StatusDot color="hsl(var(--success))" glow size={6} />
        {$t("topbar.agents_at_work", { values: { count: agentsAtWork } })}
      </button>
    {/if}

    <DownloadsPanel />

    <button
      type="button"
      class="flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-surface-2 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      class:topbar-icon-active={$guideVisible}
      onclick={() => detailSidebar.toggleGuide()}
      aria-label={$t("topbar.help_aria")}
      aria-pressed={$guideVisible}
      data-testid="topbar-help"
      title={$t("topbar.help_aria")}
    >
      <CircleHelp size={14} strokeWidth={1.5} />
    </button>

    <button
      type="button"
      class="flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-surface-2 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      class:topbar-icon-active={$detailSidebar.open}
      onclick={() => detailSidebar.toggle()}
      aria-label={$t("topbar.detail_sidebar_aria")}
      aria-pressed={$detailSidebar.open}
      data-testid="topbar-detail-sidebar-toggle"
      title={$t("topbar.detail_sidebar_aria")}
    >
      {#if $navPosition === "right"}
        <PanelLeft size={14} strokeWidth={1.5} />
      {:else}
        <PanelRight size={14} strokeWidth={1.5} />
      {/if}
    </button>

    <UserMenu />
  </div>
</header>

<style>
  /* 52px cockpit header with a 0.5px hairline underline. Height is kept in rem
     (no Tailwind token maps to 52px) and the hairline mirrors the global
     `.v4-hair` utility, which cannot be applied bottom-only. */
  .topbar {
    height: 3.25rem;
    border-bottom-width: 0.5px;
  }

  /* Pressed state of the help and detail sidebar icons. */
  .topbar-icon-active {
    background: hsl(var(--surface-2));
    color: hsl(var(--foreground));
  }

  /* Command bar width breathes across the operator breakpoints (200 / 320 /
     440 px in the approved mockup), expressed in rem so no px literal leaks
     into the markup. */
  .command-bar {
    height: 1.875rem;
    max-width: 12.5rem;
  }
  @media (min-width: 640px) {
    .command-bar {
      max-width: 20rem;
    }
  }
  @media (min-width: 1024px) {
    .command-bar {
      max-width: 27.5rem;
    }
  }
</style>
