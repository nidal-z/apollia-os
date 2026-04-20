<script lang="ts">
  import type { Snippet } from "svelte";
  import { t } from "svelte-i18n";
  import { Menu } from "lucide-svelte";
  import { sidebarState } from "$lib/stores/layout";
  import { Sheet } from "$lib/components/ui/sheet";
  import type { SettingsSubRoute } from "$lib/stores/settings";
  import SettingsNav from "./SettingsNav.svelte";

  interface Props {
    active: SettingsSubRoute;
    onnavigate: (route: SettingsSubRoute) => void;
    children: Snippet;
  }

  let { active, onnavigate, children }: Props = $props();

  // `drawer` state == viewport <768px (single source of truth).
  const isMobile = $derived($sidebarState === "drawer");
  let mobileOpen = $state(false);

  function handleNavigate(route: SettingsSubRoute) {
    onnavigate(route);
    mobileOpen = false;
  }
</script>

<div class="flex gap-6" data-testid="settings-layout">
  {#if isMobile}
    <!-- Mobile: sticky trigger + side sheet nav drawer -->
    <div class="sticky top-0 z-30 -mx-4 sm:-mx-6 mb-4 flex items-center gap-2 border-b border-border bg-background/80 px-4 sm:px-6 py-2 backdrop-blur-md">
      <button
        type="button"
        class="inline-flex h-10 items-center gap-2 rounded-md border border-border bg-muted/50 px-3 text-sm font-medium text-foreground transition-colors hover:bg-muted"
        onclick={() => (mobileOpen = true)}
        aria-haspopup="dialog"
        aria-expanded={mobileOpen}
        data-testid="settings-mobile-nav-toggle"
      >
        <Menu size={16} strokeWidth={1.75} />
        <span>{$t("settings.nav.mobile_open")}</span>
      </button>
    </div>

    <Sheet open={mobileOpen} onclose={() => (mobileOpen = false)} side="left" width="sm">
      <div class="flex h-full flex-col overflow-y-auto p-4" data-testid="settings-mobile-nav-sheet">
        <h2 class="mb-4 text-sm font-semibold text-foreground">
          {$t("settings.nav.mobile_title")}
        </h2>
        <SettingsNav {active} onnavigate={handleNavigate} />
      </div>
    </Sheet>

    <div class="min-w-0 flex-1" data-testid="settings-content">
      {@render children()}
    </div>
  {:else}
    <SettingsNav {active} {onnavigate} />
    <div class="min-w-0 flex-1" data-testid="settings-content">
      {@render children()}
    </div>
  {/if}
</div>
