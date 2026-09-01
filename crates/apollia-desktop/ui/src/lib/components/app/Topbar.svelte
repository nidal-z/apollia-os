<script lang="ts">
  /**
   * Topbar - V3 Operator cockpit header.
   *
   *   [Mobile menu] [Breadcrumb] ··· [Command bar] ··· [Agents-at-work] [ModeChip] [Activity] [Settings] [UserMenu]
   *
   * The ModeChip is the single deviation from the V3 mockup - kept here per
   * product call so the OP/Builder toggle stays in the cockpit.
   */
  import { t } from "svelte-i18n";
  import { Menu, Search, ShieldCheck, Settings as SettingsIcon } from "lucide-svelte";
  import { navigateTo } from "$lib/stores/navigation";
  import { sidebarState, layoutActions } from "$lib/stores/layout";
  import { runningTasks } from "$lib/stores/tasks";
  import { OperatorBreadcrumb } from "$lib/components/layout";
  import { StatusDot } from "$lib/components/operator";
  import UserMenu from "./UserMenu.svelte";
  import ModeChip from "./ModeChip.svelte";

  function openSearch() {
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true, bubbles: true }));
  }

  const agentsAtWork = $derived($runningTasks.length);
</script>

<header
  class="topbar sticky top-0 z-drawer flex items-center gap-3 border-b border-border bg-muted/80 px-4 backdrop-blur-xl"
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
      class="command-bar v4-hair flex w-full cursor-text items-center gap-2 rounded-md border bg-surface-1 px-2.5 text-left text-body-xs text-muted-foreground shadow-elev-0 transition-colors hover:bg-surface-1/80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      onclick={openSearch}
      aria-label={$t("topbar.search_aria")}
      data-testid="topbar-search"
    >
      <Search size={13} strokeWidth={1.75} class="flex-shrink-0" />
      <span class="flex-1 truncate">{$t("topbar.search.placeholder")}</span>
      <kbd class="hidden items-center gap-0.5 text-overline sm:flex">
        <span class="v4-hair rounded border bg-muted px-1 py-px">⌘</span>
        <span class="v4-hair rounded border bg-muted px-1 py-px">K</span>
      </kbd>
    </button>
  </div>

  <!-- Right: status + controls -->
  <div class="flex flex-shrink-0 items-center gap-1.5">
    {#if agentsAtWork > 0}
      <span
        class="v4-hair inline-flex items-center gap-1.5 rounded-full leading-none border bg-surface-1 px-2.5 py-1 text-caption text-muted-foreground"
        data-testid="topbar-agents-at-work"
        aria-label={$t("topbar.agents_at_work", { values: { count: agentsAtWork } })}
      >
        <StatusDot color="hsl(var(--success))" glow size={6} />
        {$t("topbar.agents_at_work", { values: { count: agentsAtWork } })}
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
  /* 52px cockpit header with a 0.5px hairline underline. Height is kept in rem
     (no Tailwind token maps to 52px) and the hairline mirrors the global
     `.v4-hair` utility, which cannot be applied bottom-only. */
  .topbar {
    height: 3.25rem;
    border-bottom-width: 0.5px;
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
